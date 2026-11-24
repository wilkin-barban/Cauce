//! The scheduling tick over real FFmpeg and over fake converters
//! (`docs/architecture.md` §6-§7, §12; `docs/contracts.md` §4, §7).
//!
//! Each case drives `tick` by hand and waits for observable state until a
//! deadline instead of sleeping a fixed time. A case needing the fixture or
//! FFmpeg skips with an explicit message.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cauce_core::{DuplicatePolicy, HostFilters, JobError, JobState};
use cauce_daemon::orchestrator::{OrchestratorConfig, tick};
use cauce_daemon::{Daemon, Event};
use cauce_ffmpeg::toolchain::Toolchain;
use cauce_jobs::Store;
use serde_json::{Value, json};
use tokio::sync::broadcast::Receiver;

const FIXTURE: &str = "target/cauce-fixtures/run/sample-h264-aac.mp4";
const PRESET: &str = "system-smarttv-compat";
/// Upper bound for any observable state in a case.
const DEADLINE: Duration = Duration::from_secs(20);
const WRITES_NOTHING: &str = "#!/bin/sh\nexit 0\n"; // exits 0, writes nothing
const EXITS_ONE: &str = "#!/bin/sh\nexit 1\n"; // exits 1 at once
const LONG_RUNNING: &str =
    "#!/bin/sh\nfor arg in \"$@\"; do last=\"$arg\"; done\n: > \"$last\"\nsleep 30\n";

fn unique_dir(tag: &str) -> PathBuf {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH);
    let nanos = elapsed.map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("cauce-orch-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("directorio temporal");
    dir
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent().and_then(Path::parent);
    root.expect("workspace").to_path_buf()
}

/// The real fixture, or `None` after naming what is missing.
fn fixture() -> Option<PathBuf> {
    let candidate = repo_root().join(FIXTURE);
    let found = candidate.canonicalize().ok().filter(|path| path.is_file());
    if found.is_none() {
        eprintln!("salto el caso: falta {}", candidate.display());
    }
    found
}

/// The system toolchain, or `None` after naming the gap.
fn real_toolchain() -> Option<Toolchain> {
    let toolchain = cauce_ffmpeg::locate();
    if let Err(error) = &toolchain {
        eprintln!("salto el caso: FFmpeg no disponible ({})", error.message);
    }
    toolchain.ok()
}

/// A toolchain whose `ffmpeg` is `script`, published by rename so no thread ever
/// sees a half-written file (`ETXTBSY`), and whose `ffprobe` is the real one.
fn fake_toolchain(dir: &Path, script: &str) -> Option<Toolchain> {
    let real = real_toolchain()?;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("carpeta de binarios");
    let staged = bin.join("ffmpeg.staged");
    std::fs::write(&staged, script).expect("escribir el conversor falso");
    let mut permissions = std::fs::metadata(&staged).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&staged, permissions).expect("permiso de ejecución");
    let ffmpeg = bin.join("ffmpeg");
    std::fs::rename(&staged, &ffmpeg).expect("publicar el conversor");
    let ffprobe = bin.join("ffprobe");
    let _ = std::fs::remove_file(&ffprobe);
    std::os::unix::fs::symlink(&real.ffprobe, &ffprobe).expect("enlazar ffprobe");
    Some(Toolchain { ffmpeg, ffprobe })
}

/// A case over the real toolchain (or a fake converter), plus the fixture.
fn prepared(tag: &str) -> Option<(Case, PathBuf)> {
    let input = fixture()?;
    Some((Case::new(tag, real_toolchain()?), input))
}

fn prepared_with(tag: &str, script: &str) -> Option<(Case, PathBuf)> {
    let input = fixture()?;
    let toolchain = fake_toolchain(&unique_dir(&format!("{tag}-bin")), script)?;
    Some((Case::new(tag, toolchain), input))
}

struct Case {
    dir: PathBuf,
    daemon: Arc<Daemon>,
    config: OrchestratorConfig,
    events: Receiver<Event>,
    /// `job.updated` states and `job.progress` samples observed so far.
    trail: Vec<String>,
    samples: Vec<Value>,
}

impl Case {
    fn new(tag: &str, toolchain: Toolchain) -> Self {
        Self::in_dir(unique_dir(tag), toolchain)
    }

    fn in_dir(dir: PathBuf, toolchain: Toolchain) -> Self {
        let store = Store::open(&dir.join("cauce.db")).expect("abrir la base");
        let daemon = Arc::new(Daemon::new(store, vec![dir.clone(), repo_root()]));
        let events = daemon.events().subscribe();
        Self {
            dir,
            daemon,
            config: OrchestratorConfig {
                toolchain,
                host_filters: HostFilters::ALL,
                max_concurrent: 1,
                duplicate_policy: DuplicatePolicy::Suffix,
            },
            events,
            trail: Vec::new(),
            samples: Vec::new(),
        }
    }

    /// Queues one job through the same IPC path the GUI uses.
    fn add_job(&self, input: &Path) -> String {
        let output_dir = self.dir.join("salida");
        std::fs::create_dir_all(&output_dir).expect("carpeta de salida");
        let params = json!({
            "inputs": [input.to_string_lossy()],
            "output_dir": output_dir.to_string_lossy(),
            "preset_id": PRESET,
        });
        let jobs = self.call("jobs.add", &params).expect("jobs.add");
        jobs[0]["id"].as_str().expect("id del trabajo").to_owned()
    }

    /// One IPC call, the same entry point the socket uses.
    fn call(&self, method: &str, params: &Value) -> Result<Value, JobError> {
        self.daemon.dispatch(method, Some(params))
    }

    fn job(&self, id: &str) -> Value {
        self.call("jobs.get", &json!({"id": id})).expect("jobs.get")
    }

    fn state(&self, id: &str) -> JobState {
        serde_json::from_value(self.job(id)["state"].clone()).expect("estado conocido")
    }

    /// Ticks until `done` holds, and fails the case on the deadline.
    fn run_until(&mut self, id: &str, done: impl FnMut(&Self, &str) -> bool) {
        self.run_until_within(id, DEADLINE, "el estado esperado", done);
    }

    /// Ticks until `done` holds, naming `what` never happened after `limit`.
    fn run_until_within(
        &mut self,
        id: &str,
        limit: Duration,
        what: &str,
        mut done: impl FnMut(&Self, &str) -> bool,
    ) {
        let deadline = Instant::now() + limit;
        loop {
            self.pump(id);
            if done(self, id) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{what} no ocurrió en {limit:?}: {}",
                self.job(id)
            );
            tick(&self.daemon, &self.config).expect("tick");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Collects pending events of `id`: states into `trail`, samples into `samples`.
    fn pump(&mut self, id: &str) {
        while let Ok(event) = self.events.try_recv() {
            if event.params["id"] != json!(id) {
                continue;
            }
            match event.method.as_str() {
                "job.updated" => {
                    if let Some(state) = event.params["state"].as_str() {
                        self.trail.push(state.to_owned());
                    }
                }
                "job.progress" => self.samples.push(event.params["progress"].clone()),
                _ => {}
            }
        }
    }
}

/// Asserts `expected` appears in `trail` in order, allowing repeats.
fn assert_trail(trail: &[String], expected: &[&str]) {
    let mut wanted = expected.iter();
    let mut next = wanted.next();
    for state in trail {
        if next.is_some_and(|target| *target == state) {
            next = wanted.next();
        }
    }
    assert!(
        next.is_none(),
        "la traza {trail:?} no incluye {expected:?} en orden"
    );
}

#[test]
fn runs_a_real_conversion_to_completion() {
    let Some((mut case, input)) = prepared("complete") else {
        return;
    };
    let id = case.add_job(&input);

    case.run_until(&id, |case, id| case.state(id) == JobState::Completed);

    assert_trail(
        &case.trail,
        &["queued", "probing", "running", "validating", "completed"],
    );
    let job = case.job(&id);
    assert!(job["temp_path"].is_null(), "el temporal se limpia");
    assert!(job["error"].is_null(), "un trabajo completo no lleva error");
    assert!(job["finished_at"].is_string(), "quedó sellado: {job}");
    let output = PathBuf::from(job["output_path"].as_str().expect("salida"));
    let published = std::fs::metadata(&output).expect("la salida existe");
    assert_ne!(published.len(), 0, "la salida no está vacía");
    let temp = output
        .parent()
        .expect("carpeta")
        .join(format!(".cauce-{id}.part.mp4"));
    assert!(!temp.exists(), "el temporal se publicó, no quedó copia");
}

#[test]
fn fails_when_the_converter_produces_nothing() {
    let Some((mut case, input)) = prepared_with("nothing", WRITES_NOTHING) else {
        return;
    };
    let id = case.add_job(&input);

    case.run_until(&id, |case, id| case.state(id) == JobState::Failed);

    assert_trail(&case.trail, &["queued", "probing", "running", "validating"]);
    let job = case.job(&id);
    assert_eq!(
        job["error"]["code"],
        json!("OUTPUT_VALIDATION_FAILED"),
        "un conversor que no escribe nunca completa: {job}"
    );
    let output = job["output_path"].as_str().expect("salida");
    assert!(!Path::new(output).exists(), "no se publica nada");
}

#[test]
fn a_converter_that_closes_stderr_does_not_hang_the_queue() {
    let Some(input) = fixture() else {
        return;
    };
    let dir = unique_dir("stderr-closed");
    let holder_file = dir.join("holder.pid");
    // stderr is inherited by a background helper, so the pipe stays open after
    // the converter itself is gone: the stream-end marker never fires and only
    // the exit status can finish the job. The converter writes nothing, so the
    // only legal end is `OUTPUT_VALIDATION_FAILED`.
    let script = format!(
        "#!/bin/sh\nsleep 30 &\necho $! > {}\nexit 0\n",
        holder_file.display()
    );
    let Some(toolchain) = fake_toolchain(&unique_dir("stderr-closed-bin"), &script) else {
        return;
    };
    let mut case = Case::in_dir(dir, toolchain);
    let id = case.add_job(&input);

    // 10 s: the exit is immediate, so anything slower is the hang this covers.
    case.run_until_within(
        &id,
        Duration::from_secs(10),
        "el trabajo sin marcador de stderr",
        |case, id| case.state(id) == JobState::Failed,
    );
    assert_trail(
        &case.trail,
        &["queued", "probing", "running", "validating", "failed"],
    );
    let job = case.job(&id);
    assert_eq!(
        job["error"]["code"],
        json!("OUTPUT_VALIDATION_FAILED"),
        "un conversor que no escribe nunca completa, aun sin marcador de stderr: {job}"
    );
    assert!(
        job["finished_at"].is_string(),
        "el trabajo quedó sellado: {job}"
    );
    // Idempotent: the handle already left the map, so later ticks cannot fail it
    // twice — neither through the marker nor through `try_exit`.
    for _ in 0..3 {
        tick(&case.daemon, &case.config).expect("tick");
    }
    case.pump(&id);
    assert_eq!(
        case.trail.iter().filter(|state| *state == "failed").count(),
        1,
        "el trabajo se falla una sola vez: {:?}",
        case.trail
    );

    // The queue is not blocked: with one slot, the next job still starts.
    case.trail.clear();
    let second = case.add_job(&input);
    case.run_until_within(
        &second,
        Duration::from_secs(10),
        "el segundo trabajo en cola en arrancar",
        |case, id| case.state(id) == JobState::Running,
    );
    assert_trail(&case.trail, &["queued", "probing", "running"]);

    kill_holder(&holder_file);
}

/// Kills the background helper that keeps the fake converter's pipes open.
fn kill_holder(file: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Ok(body) = std::fs::read_to_string(file) {
            if let Ok(pid) = body.trim().parse::<i32>() {
                break pid;
            }
        }
        assert!(
            Instant::now() < deadline,
            "el conversor falso no escribió el pid de su ayudante en {file:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    // SAFETY: `kill(2)` only reads its arguments; the pid belongs to the helper
    // this test started and is already gone once it exits.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
}

#[test]
fn fails_when_the_converter_exits_non_zero() {
    let Some((mut case, input)) = prepared_with("exit", EXITS_ONE) else {
        return;
    };
    let id = case.add_job(&input);

    case.run_until(&id, |case, id| case.state(id) == JobState::Failed);

    let job = case.job(&id);
    assert_eq!(job["error"]["code"], json!("PROCESS_FAILED"), "{job}");
    assert!(job["finished_at"].is_string(), "quedó sellado: {job}");
}

#[test]
fn queue_pause_prevents_starting() {
    let Some((case, input)) = prepared("paused-queue") else {
        return;
    };
    let id = case.add_job(&input);

    case.call("queue.pause", &json!({})).expect("pausar");
    for _ in 0..3 {
        assert_eq!(tick(&case.daemon, &case.config).expect("tick"), None);
    }
    assert_eq!(case.state(&id), JobState::Queued, "sigue en la cola");

    case.call("queue.resume", &json!({})).expect("reanudar");
    let started = tick(&case.daemon, &case.config).expect("tick");
    assert_eq!(
        started.as_deref(),
        Some(id.as_str()),
        "la cola reanudada arranca"
    );
}

#[test]
fn cancel_moves_a_running_job_to_cancelled() {
    let Some((mut case, input)) = prepared_with("cancel", LONG_RUNNING) else {
        return;
    };
    let keep = json!({"keep_temp_on_failure": true});
    case.call("settings.set", &keep).expect("settings.set");
    let id = case.add_job(&input);

    case.run_until(&id, |case, id| case.state(id) == JobState::Running);
    assert_trail(&case.trail, &["queued", "probing", "running"]);
    let temp = case.job(&id)["temp_path"]
        .as_str()
        .expect("el trabajo en curso tiene temporal")
        .to_owned();
    let deadline = Instant::now() + DEADLINE;
    while !Path::new(&temp).exists() {
        assert!(
            Instant::now() < deadline,
            "el conversor falso no llegó a crear el temporal en {DEADLINE:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let cancelled = case
        .call("jobs.cancel", &json!({"id": id}))
        .expect("jobs.cancel");

    assert_eq!(cancelled["state"], json!("cancelled"), "{cancelled}");
    case.pump(&id);
    assert_trail(&case.trail, &["cancelling", "cancelled"]);
    let job = case.job(&id);
    assert_eq!(job["state"], json!("cancelled"));
    assert_eq!(job["error"], Value::Null, "cancelar no es un fallo: {job}");
    assert!(
        Path::new(&temp).exists(),
        "con `keep_temp_on_failure` el temporal queda en disco: {temp}"
    );
}

#[test]
fn remove_is_reflected_over_ipc() {
    let Some((case, input)) = prepared("remove") else {
        return;
    };
    let mut queue_events = case.daemon.events().subscribe();
    let id = case.add_job(&input);

    let removed = case
        .call("jobs.remove", &json!({"id": id}))
        .expect("jobs.remove");
    assert_eq!(removed, json!({"removed": true}));
    assert!(
        case.call("jobs.get", &json!({"id": id})).is_err(),
        "el trabajo ya no existe"
    );
    let listed = case.call("jobs.list", &json!({})).expect("jobs.list");
    assert_eq!(
        listed.as_array().map(Vec::len),
        Some(0),
        "la cola quedó vacía"
    );

    let again = case
        .call("jobs.remove", &json!({"id": id}))
        .expect("jobs.remove");
    assert_eq!(
        again,
        json!({"removed": false}),
        "un id ausente no es un error"
    );

    let mut seen = 0;
    while let Ok(event) = queue_events.try_recv() {
        if event.method == "queue.updated" {
            seen += 1;
        }
    }
    assert!(seen > 0, "`jobs.remove` publica `queue.updated`");
}

#[test]
fn progress_is_persisted_and_broadcast() {
    let Some((mut case, input)) = prepared("progress") else {
        return;
    };
    let id = case.add_job(&input);

    case.run_until(&id, |case, id| {
        matches!(case.state(id), JobState::Completed | JobState::Failed)
    });

    assert!(
        case.state(&id) == JobState::Completed,
        "la conversión termina bien"
    );
    let percents: Vec<f64> = case
        .samples
        .iter()
        .filter_map(|s| s["percent"].as_f64())
        .collect();
    assert!(!percents.is_empty(), "al menos una muestra trae porcentaje");
    assert!(
        percents
            .iter()
            .all(|percent| (0.0..=100.0).contains(percent)),
        "el porcentaje queda en 0..=100: {percents:?}"
    );
    let stored = case.job(&id)["progress"].clone();
    assert!(
        stored["processed_ms"].as_u64().unwrap_or(0) > 0,
        "progreso persistido: {stored}"
    );
}
