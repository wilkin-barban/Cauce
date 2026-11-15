use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cauce_core::{
    DuplicatePolicy, ErrorCode, Job, JobError, JobState, Progress, Timestamp, build_ffmpeg_args,
    next_startable, plan_conversion, temp_path, validate_output,
};
use cauce_ffmpeg::runner::spawn;

use crate::dispatch::{Daemon, broadcast_queue, find_preset};
use crate::events::Event;

use super::{OrchestratorConfig, REASON_SCHEDULER, RunningJob, SAVE_PROGRESS_INTERVAL};

/// One scheduling tick. Returns the job it started, if any.
pub fn tick(daemon: &Daemon, config: &OrchestratorConfig) -> Result<Option<String>, JobError> {
    finish_finished(daemon, config);
    start_next(daemon, config)
}

/// Reaps and judges every conversion whose process already ended.
fn finish_finished(daemon: &Daemon, config: &OrchestratorConfig) {
    for (id, running) in take_finished(daemon) {
        if let Err(error) = finish_one(daemon, config, &id, running) {
            daemon.events().log(
                "error",
                "queue",
                format!("no se pudo cerrar el trabajo `{id}`: {}", error.message),
            );
        }
    }
}

/// Removes the finished children from the map, so nothing waits on them twice.
fn take_finished(daemon: &Daemon) -> Vec<(String, RunningJob)> {
    let mut children = daemon.children();
    let done: Vec<String> = children
        .iter()
        .filter(|(_, running)| running.exited.load(Ordering::SeqCst))
        .map(|(id, _)| id.clone())
        .collect();
    done.into_iter()
        .filter_map(|id| children.remove(&id).map(|running| (id, running)))
        .collect()
}

fn finish_one(
    daemon: &Daemon,
    config: &OrchestratorConfig,
    id: &str,
    mut running: RunningJob,
) -> Result<(), JobError> {
    // The process is gone, so `wait` only reaps and joins the reader threads.
    match running.child.wait() {
        Ok(_) => publish(daemon, config, id),
        Err(error) => fail(daemon, id, &error),
    }
}

/// `running -> validating`, validate, publish atomically, `validating -> completed`.
fn publish(daemon: &Daemon, config: &OrchestratorConfig, id: &str) -> Result<(), JobError> {
    let validating = {
        let mut store = daemon.store_mut();
        let Some(job) = store.get_job(id)? else {
            return Ok(());
        };
        // Cancelled or paused while the process was finishing: leave it alone.
        if job.state != JobState::Running {
            return Ok(());
        }
        store.transition(id, JobState::Validating, REASON_SCHEDULER)?
    };
    daemon.events().job_updated(&validating);

    match validate_and_publish(&validating, config) {
        Ok(()) => {
            let completed = {
                let mut store = daemon.store_mut();
                store.set_temp_path(id, None)?;
                let completed = store.transition(id, JobState::Completed, REASON_SCHEDULER)?;
                store.release_inactive_reservations()?;
                completed
            };
            daemon.events().job_updated(&completed);
            broadcast_queue(daemon);
            daemon.events().log(
                "info",
                "queue",
                format!("trabajo `{id}` completado en `{}`", completed.output_path),
            );
            Ok(())
        }
        Err(error) => fail(daemon, id, &error),
    }
}

/// `docs/architecture.md` §12: the temporary exists, FFprobe reads it, the plan
/// matches and only then is it renamed onto the destination.
fn validate_and_publish(job: &Job, config: &OrchestratorConfig) -> Result<(), JobError> {
    let temp = job.temp_path.clone().ok_or_else(|| {
        validation_failed(format!(
            "El trabajo no tiene temporal que validar: no se publica `{}`.",
            job.output_path
        ))
    })?;
    let probed = std::fs::metadata(&temp);
    match probed {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {}
        Ok(_) => {
            return Err(validation_failed(format!(
                "El conversor no produjo ningún archivo en `{temp}`; no se publica nada."
            )));
        }
        Err(error) => {
            return Err(validation_failed(format!(
                "El temporal `{temp}` no existe ({error}); no se publica nada."
            )));
        }
    }

    let source = job.probe.clone().ok_or_else(|| {
        validation_failed(format!(
            "El trabajo no guardó el probe de `{}`.",
            job.input_path
        ))
    })?;
    let plan = job.plan.clone().ok_or_else(|| {
        validation_failed("El trabajo no guardó el plan y un plan persistido nunca se recalcula.")
    })?;
    let probed_at = utc_now()?;
    let output = cauce_ffmpeg::probe::probe_file(&config.toolchain, Path::new(&temp), probed_at)?;
    validate_output(&plan, &source, &output)?;

    let destination = Path::new(&job.output_path);
    if config.duplicate_policy != DuplicatePolicy::Overwrite && destination.exists() {
        return Err(JobError::new(
            ErrorCode::OutputCollision,
            format!(
                "El destino `{}` apareció durante la conversión y la política `{}` no \
                 sobrescribe; el temporal quedó en `{temp}`.",
                job.output_path,
                policy_name(config.duplicate_policy),
            ),
            true,
        ));
    }
    std::fs::rename(&temp, destination).map_err(|error| {
        validation_failed(format!(
            "No se pudo publicar `{}` desde `{temp}`: {error}.",
            job.output_path
        ))
    })
}

/// Stores the error *before* the transition: the schema refuses a `failed` row
/// without `error_code` (`docs/data-model.md` §2.2).
fn fail(daemon: &Daemon, id: &str, error: &JobError) -> Result<(), JobError> {
    let keep_temp = {
        let store = daemon.store_mut();
        store.settings_get()?.keep_temp_on_failure
    };

    let failed = {
        let mut store = daemon.store_mut();
        let Some(job) = store.get_job(id)? else {
            return Ok(());
        };
        if matches!(
            job.state,
            JobState::Completed | JobState::Cancelled | JobState::Failed
        ) {
            return Ok(());
        }
        store.set_error(id, error)?;
        let failed = store.transition(id, JobState::Failed, REASON_SCHEDULER)?;
        if !keep_temp {
            if let Some(temp) = failed.temp_path.as_deref() {
                let _ = std::fs::remove_file(temp);
            }
            store.set_temp_path(id, None)?;
        }
        store.release_inactive_reservations()?;
        failed
    };

    daemon.events().job_updated(&failed);
    broadcast_queue(daemon);
    daemon.events().log(
        "warn",
        "queue",
        format!("trabajo `{id}` falló: {}", error.message),
    );
    Ok(())
}

/// Starts the next job the scheduler allows, or nothing.
fn start_next(daemon: &Daemon, config: &OrchestratorConfig) -> Result<Option<String>, JobError> {
    let (jobs, running) = {
        let store = daemon.store_mut();
        (store.list_jobs(None)?, store.active_count()?)
    };
    let paused = daemon.events().queue_paused();
    let Some(job) = next_startable(&jobs, running, config.max_concurrent, paused) else {
        return Ok(None);
    };
    let job = job.clone();

    match start(daemon, config, &job) {
        Ok(()) => Ok(Some(job.id)),
        Err(error) => {
            daemon.events().log(
                "error",
                "queue",
                format!("trabajo `{}` no pudo arrancar: {}", job.id, error.message),
            );
            fail(daemon, &job.id, &error)?;
            Ok(None)
        }
    }
}

/// `queued -> probing`, probe, plan, persist, `probing -> running`, spawn.
fn start(daemon: &Daemon, config: &OrchestratorConfig, job: &Job) -> Result<(), JobError> {
    let probing = {
        let mut store = daemon.store_mut();
        store.transition(&job.id, JobState::Probing, REASON_SCHEDULER)?
    };
    daemon.events().job_updated(&probing);

    let probed_at = utc_now()?;
    let probe =
        cauce_ffmpeg::probe::probe_file(&config.toolchain, Path::new(&job.input_path), probed_at)?;
    let preset = find_preset(&job.preset_id)?;
    let plan = plan_conversion(&probe, preset)?;
    let temp = temp_path(&job.output_path, &job.id)?;
    let args = build_ffmpeg_args(&job.input_path, &temp, &plan)?;

    let running = {
        let mut store = daemon.store_mut();
        store.set_probe_and_plan(&job.id, &probe, &plan)?;
        store.set_temp_path(&job.id, Some(&temp))?;
        store.transition(&job.id, JobState::Running, REASON_SCHEDULER)?
    };
    daemon.events().job_updated(&running);

    let (exited, on_progress, on_stderr) = sinks(daemon, &job.id);
    let child = spawn(
        &config.toolchain.ffmpeg,
        &args,
        probe.duration_ms,
        on_progress,
        on_stderr,
    )?;
    daemon
        .children()
        .insert(job.id.clone(), RunningJob { child, exited });

    broadcast_queue(daemon);
    daemon.events().log(
        "info",
        "ffmpeg",
        format!(
            "trabajo `{}` convirtiendo `{}` en `{temp}`",
            job.id, job.input_path
        ),
    );
    Ok(())
}

/// Callbacks for the runner: progress to the store and the socket, stderr to the
/// log ring, and the [`StreamEnd`] marker that tells the next tick to reap.
type Sinks = (
    Arc<AtomicBool>,
    Box<dyn FnMut(Progress) + Send>,
    Box<dyn FnMut(&str) + Send>,
);

fn sinks(daemon: &Daemon, id: &str) -> Sinks {
    let exited = Arc::new(AtomicBool::new(false));
    let store = daemon.store_handle();
    let events = daemon.events_handle();

    let job_id = id.to_owned();
    let last_saved: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let progress_store = Arc::clone(&store);
    let progress_events = Arc::clone(&events);
    let progress_id = job_id.clone();
    let progress_last = Arc::clone(&last_saved);
    let on_progress: Box<dyn FnMut(Progress) + Send> = Box::new(move |progress: Progress| {
        progress_events.broadcast(Event {
            method: "job.progress".to_owned(),
            params: serde_json::json!({"id": progress_id, "progress": progress}),
        });
        if due(&progress_last) {
            if let Ok(mut store) = progress_store.lock() {
                let _ = store.save_progress(&progress_id, &progress);
            }
            if let Ok(mut last) = progress_last.lock() {
                *last = Some(Instant::now());
            }
        }
    });

    let marker = StreamEnd(Arc::clone(&exited));
    let log_events = events;
    let on_stderr: Box<dyn FnMut(&str) + Send> = Box::new(move |line: &str| {
        // The marker dies with this callback, which is how the tick learns the
        // process ended.
        let _held = &marker;
        log_events.log("debug", "ffmpeg", line);
    });

    (exited, on_progress, on_stderr)
}

/// True when a sample is old enough to be persisted (at most 1 Hz per job).
fn due(last: &Mutex<Option<Instant>>) -> bool {
    let last = last.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    last.is_none_or(|at| at.elapsed() >= SAVE_PROGRESS_INTERVAL)
}

/// Signals that FFmpeg's output streams closed, i.e. that the process ended.
struct StreamEnd(Arc<AtomicBool>);

impl Drop for StreamEnd {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn validation_failed(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::OutputValidationFailed, message, true)
}

fn policy_name(policy: DuplicatePolicy) -> &'static str {
    match policy {
        DuplicatePolicy::Suffix => "suffix",
        DuplicatePolicy::Skip => "skip",
        DuplicatePolicy::Overwrite => "overwrite",
    }
}

fn utc_now() -> Result<Timestamp, JobError> {
    cauce_jobs::migrations::utc_now()
}
