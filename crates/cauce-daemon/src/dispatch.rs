//! The method table of `docs/contracts.md` §6 and the daemon state behind it.
//!
//! Every handler is synchronous: the store is SQLite on one connection and
//! `ffprobe` is a child process, so the caller runs [`Daemon::dispatch`] in a
//! blocking task instead of stalling the socket loop.
//!
//! Known v0.1 gaps, all visible to the client:
//!
//! - `jobs.remove` returns `INTERNAL` "no disponible todavía": deleting a job
//!   needs `Store::remove_job`, which does not exist yet (T13b).
//! - `job.progress` and `log.entry` are not emitted: no conversion loop and no
//!   file logging exist in this half of the task (T13b, T14).
//! - `jobs.add` inserts one transaction per job, so the "insert nothing on a
//!   collision" guarantee is enforced by planning every output against the
//!   reserved set before the first insert; a reservation lost to a concurrent
//!   writer between those two steps still aborts after the earlier inserts.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use cauce_core::{
    DuplicatePolicy, ErrorCode, Job, JobAction, JobError, JobState, MediaSnapshot, Progress,
    Settings, Timestamp, plan_conversion, resolve_output_path, system_presets,
};
use cauce_jobs::Store;
use serde_json::{Value, json};

use crate::events::Events;
use crate::protocol::{self, PROTOCOL_VERSION};
use crate::socket;

/// Highest absolute priority accepted over IPC (`docs/contracts.md` §2.4).
const PRIORITY_LIMIT: i32 = 1000;

/// Daemon state: the single writer of the database plus the event bus.
pub struct Daemon {
    store: Mutex<Store>,
    events: Events,
    roots: Vec<PathBuf>,
}

impl Daemon {
    /// Builds the daemon over an open store and the roots it may read and write.
    ///
    /// Roots are canonicalized once: comparing canonical paths is the only way
    /// to keep a symlink from escaping an authorized folder.
    pub fn new(store: Store, roots: Vec<PathBuf>) -> Self {
        let roots = roots
            .into_iter()
            .filter_map(|root| root.canonicalize().ok())
            .collect();
        Self {
            store: Mutex::new(store),
            events: Events::new(),
            roots,
        }
    }

    pub fn events(&self) -> &Events {
        &self.events
    }

    /// Store access for the scheduler (T13b) and for tests that need to place a
    /// job in a state the IPC surface cannot reach yet.
    pub fn store_mut(&self) -> MutexGuard<'_, Store> {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Routes one method. Errors are contract errors, never panics.
    pub fn dispatch(&self, method: &str, params: Option<&Value>) -> Result<Value, JobError> {
        match method {
            "initialize" => self.initialize(params),
            "presets.list" => protocol::encode(system_presets()),
            "settings.get" => protocol::encode(&self.store_mut().settings_get()?),
            "settings.set" => self.settings_set(params),
            "system.capabilities" => self.capabilities(),
            "media.inspect" => self.media_inspect(params),
            "media.plan" => self.media_plan(params),
            "jobs.add" => self.jobs_add(params),
            "jobs.list" => self.jobs_list(params),
            "jobs.get" => self.jobs_get(params),
            "jobs.pause" => self.job_action(params, JobAction::Pause),
            "jobs.resume" => self.job_action(params, JobAction::Resume),
            "jobs.cancel" => self.job_action(params, JobAction::Cancel),
            "jobs.retry" => self.job_action(params, JobAction::Retry),
            // Documented gap: the store has no removal method yet (T13b).
            "jobs.remove" => Err(protocol::internal(
                "Eliminar trabajos no disponible todavía: el store todavía no borra trabajos (T13b); usá `jobs.cancel` mientras tanto.",
            )),
            "queue.pause" => self.set_queue(true),
            "queue.resume" => self.set_queue(false),
            "logs.list" => self.logs_list(params),
            other => {
                self.events
                    .log("warn", "ipc", format!("método desconocido `{other}`"));
                Err(protocol::unknown_method(other))
            }
        }
    }

    fn initialize(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let requested = params
            .and_then(|params| params.get("protocol_version"))
            .and_then(Value::as_u64)
            .unwrap_or(PROTOCOL_VERSION);
        if requested > PROTOCOL_VERSION {
            return Err(protocol::invalid(format!(
                "El cliente pidió el protocolo {requested} y este daemon habla hasta la versión \
                 {PROTOCOL_VERSION}: actualizá Cauce o el cliente antes de continuar."
            )));
        }

        let name = params
            .and_then(|params| params.get("client_name"))
            .and_then(Value::as_str)
            .unwrap_or("desconocido");
        let version = params
            .and_then(|params| params.get("client_version"))
            .and_then(Value::as_str)
            .unwrap_or("desconocida");
        self.events.log(
            "info",
            "ipc",
            format!("cliente `{name}` {version} inicializado"),
        );

        Ok(json!({
            "server_version": env!("CARGO_PKG_VERSION"),
            "protocol_version": PROTOCOL_VERSION,
        }))
    }

    fn settings_set(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let patch = params.filter(|params| params.is_object()).ok_or_else(|| {
            protocol::invalid("`settings.set` necesita un objeto con los ajustes.")
        })?;

        let mut store = self.store_mut();
        let current = serde_json::to_value(store.settings_get()?).map_err(protocol::json_error)?;
        let merged = protocol::merge(current, patch);
        let settings: Settings = serde_json::from_value(merged).map_err(|error| {
            protocol::invalid(format!("Ajustes inválidos o incompletos: {error}"))
        })?;
        store.settings_set(&settings)?;
        self.events.log("info", "settings", "ajustes guardados");
        protocol::encode(&settings)
    }

    fn capabilities(&self) -> Result<Value, JobError> {
        let toolchain = cauce_ffmpeg::locate()?;
        let disk = self
            .roots
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        let mut capabilities = cauce_ffmpeg::inspect(&toolchain, &disk)?;
        capabilities.daemon_version = env!("CARGO_PKG_VERSION").to_owned();
        for warning in &capabilities.warnings {
            self.events.log("warn", "ffmpeg", warning.clone());
        }
        protocol::encode(&capabilities)
    }

    fn media_inspect(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let paths = protocol::string_list(params, "paths")?;
        if paths.is_empty() {
            return Err(protocol::invalid(
                "`media.inspect` necesita al menos una ruta en `paths`.",
            ));
        }

        let toolchain = cauce_ffmpeg::locate()?;
        let mut snapshots = Vec::with_capacity(paths.len());
        for raw in &paths {
            let path = self.authorize(raw)?;
            let probed_at = utc_now()?;
            snapshots.push(cauce_ffmpeg::probe::probe_file(
                &toolchain,
                Path::new(&path),
                probed_at,
            )?);
        }
        protocol::encode(&snapshots)
    }

    fn media_plan(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let snapshot: MediaSnapshot = protocol::decode(params, "snapshot")?;
        let preset_id = protocol::string_param(params, "preset_id")?;
        let preset = find_preset(&preset_id)?;
        protocol::encode(&plan_conversion(&snapshot, preset)?)
    }

    fn jobs_add(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let inputs = protocol::string_list(params, "inputs")?;
        if inputs.is_empty() {
            return Err(protocol::invalid(
                "`jobs.add` necesita al menos una entrada en `inputs`.",
            ));
        }
        let output_dir = self.authorize(&protocol::string_param(params, "output_dir")?)?;
        let preset_id = protocol::string_param(params, "preset_id")?;
        let preset = find_preset(&preset_id)?;
        let settings = self.store_mut().settings_get()?;
        let policy = protocol::optional_param(params, "duplicate_policy")?
            .map(|value| {
                serde_json::from_value::<DuplicatePolicy>(value).map_err(protocol::json_error)
            })
            .transpose()?
            .unwrap_or(settings.duplicate_policy);
        let priority = protocol::optional_param(params, "priority")?
            .map(|value| {
                value
                    .as_i64()
                    .filter(|priority| priority.unsigned_abs() <= PRIORITY_LIMIT as u64)
                    .map(|priority| priority as i32)
                    .ok_or_else(|| {
                        protocol::invalid(format!(
                            "La prioridad debe ser un entero entre -{PRIORITY_LIMIT} y {PRIORITY_LIMIT}."
                        ))
                    })
            })
            .transpose()?
            .unwrap_or(0);

        let toolchain = cauce_ffmpeg::locate()?;
        let mut reserved: Vec<String> = self
            .store_mut()
            .list_jobs(None)?
            .into_iter()
            .map(|job| job.output_path)
            .collect();

        // Plan every job before the first insert: a collision anywhere aborts
        // the request with nothing written, and each new destination joins the
        // reserved set so two inputs can never share one path.
        let mut planned = Vec::with_capacity(inputs.len());
        for raw in &inputs {
            let input_path = self.authorize(raw)?;
            let probed_at = utc_now()?;
            let snapshot =
                cauce_ffmpeg::probe::probe_file(&toolchain, Path::new(&input_path), probed_at)?;
            let plan = plan_conversion(&snapshot, preset)?;
            let output_path = resolve_output_path(
                &input_path,
                &output_dir,
                &preset.container,
                policy,
                &reserved,
            )?;
            reserved.push(output_path.clone());
            planned.push((input_path, output_path, snapshot, plan));
        }

        let mut jobs = Vec::with_capacity(planned.len());
        for (input_path, output_path, snapshot, plan) in planned {
            let created_at = utc_now()?;
            jobs.push(Job {
                id: crate::ids::mint_job_id()?,
                input_path,
                output_path,
                temp_path: None,
                preset_id: preset_id.clone(),
                state: JobState::Queued,
                priority,
                progress: Progress {
                    percent: Some(0.0),
                    processed_ms: 0,
                    total_ms: snapshot.duration_ms,
                    fps: None,
                    speed: None,
                    bitrate_bps: None,
                    eta_seconds: None,
                    updated_at: created_at.clone(),
                },
                probe: Some(snapshot),
                plan: Some(plan),
                error: None,
                created_at: created_at.clone(),
                updated_at: created_at,
                started_at: None,
                finished_at: None,
            });
        }

        {
            let mut store = self.store_mut();
            for job in &jobs {
                store.insert_job(job)?;
            }
        }
        for job in &jobs {
            self.events.job_updated(job);
            self.events
                .log("info", "queue", format!("trabajo `{}` encolado", job.id));
        }
        protocol::encode(&jobs)
    }

    fn jobs_list(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let states = match params.and_then(|params| params.get("states")) {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                serde_json::from_value::<Vec<JobState>>(value.clone()).map_err(|error| {
                    protocol::invalid(format!("Estado desconocido en `states`: {error}"))
                })?,
            ),
        };
        protocol::encode(&self.store_mut().list_jobs(states.as_deref())?)
    }

    fn jobs_get(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let id = protocol::string_param(params, "id")?;
        let job = self
            .store_mut()
            .get_job(&id)?
            .ok_or_else(|| protocol::invalid(format!("No existe el trabajo `{id}`.")))?;
        protocol::encode(&job)
    }

    fn job_action(&self, params: Option<&Value>, action: JobAction) -> Result<Value, JobError> {
        let id = protocol::string_param(params, "id")?;
        let job = self.store_mut().apply_action(&id, action)?;
        self.events.job_updated(&job);
        self.events.log(
            "info",
            "queue",
            format!("trabajo `{id}` ahora en `{}`", job.state.as_str()),
        );
        protocol::encode(&job)
    }

    fn set_queue(&self, paused: bool) -> Result<Value, JobError> {
        self.events.set_queue_paused(paused);
        let (running, queued) = {
            let store = self.store_mut();
            let running = store.active_count()?;
            let queued = store.list_jobs(Some(&[JobState::Queued]))?.len();
            (running, queued)
        };
        self.events.queue_updated(paused, running, queued);
        self.events.log(
            "info",
            "queue",
            if paused {
                "cola pausada"
            } else {
                "cola reanudada"
            },
        );
        Ok(json!({"paused": paused}))
    }

    fn logs_list(&self, params: Option<&Value>) -> Result<Value, JobError> {
        let limit = protocol::optional_param(params, "limit")?
            .map(|value| {
                value
                    .as_u64()
                    .map(|limit| limit as usize)
                    .ok_or_else(|| protocol::invalid("`limit` debe ser un entero no negativo."))
            })
            .transpose()?
            .unwrap_or(crate::events::DEFAULT_LOG_CAPACITY);
        protocol::encode(&self.events.recent_logs(limit))
    }

    /// Canonicalizes `raw` and checks it lives under an authorized root.
    pub fn authorize(&self, raw: &str) -> Result<String, JobError> {
        socket::authorize(&self.roots, raw)
    }
}

fn find_preset(id: &str) -> Result<&'static cauce_core::Preset, JobError> {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| {
            JobError::new(
                ErrorCode::PlanInvalid,
                format!("El preset `{id}` no existe en el catálogo del sistema."),
                false,
            )
        })
}

fn utc_now() -> Result<Timestamp, JobError> {
    cauce_jobs::migrations::utc_now()
}
