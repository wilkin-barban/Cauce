//! The v0.1 method table over a real Unix socket (`docs/contracts.md` §6-§8).
//! The server runs in-process on a `tokio` runtime; the client blocks per line.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cauce_core::{JobState, system_presets};
use cauce_daemon::{Daemon, serve, socket};
use cauce_jobs::Store;
use serde_json::{Value, json};

const FIXTURE: &str = "target/cauce-fixtures/run/sample-h264-aac.mp4";

fn unique_dir(tag: &str) -> PathBuf {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH);
    let nanos = elapsed.map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("cauce-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("directorio temporal");
    dir
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent().and_then(Path::parent);
    root.expect("workspace").to_path_buf()
}

/// The real fixture plus a working FFmpeg, or `None` after naming the gap.
fn ready() -> Option<PathBuf> {
    if let Err(error) = cauce_ffmpeg::locate() {
        eprintln!("salto el caso: FFmpeg no disponible ({error:?})");
        return None;
    }
    let candidate = repo_root().join(FIXTURE);
    let found = candidate.canonicalize().ok().filter(|path| path.is_file());
    if found.is_none() {
        eprintln!("salto el caso: falta {}", candidate.display());
    }
    found
}

/// Blocking client: one request line out, one message line in.
struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

impl Client {
    fn connect(path: &Path) -> Self {
        let stream = UnixStream::connect(path).expect("conectar al socket");
        let timeout = Some(Duration::from_secs(30));
        stream.set_read_timeout(timeout).expect("timeout");
        let reader = BufReader::new(stream.try_clone().expect("clonar el socket"));
        Self {
            reader,
            writer: stream,
            next_id: 1,
        }
    }

    fn read_message(&mut self) -> Value {
        let mut line = String::new();
        let read = self.reader.read_line(&mut line).expect("leer el socket");
        assert!(read > 0, "el daemon cerró la conexión");
        let message: Value = serde_json::from_str(line.trim_end()).expect("JSON-RPC");
        assert_eq!(message["jsonrpc"], json!("2.0"), "falta la versión");
        message
    }

    /// Notifications carry no `id`, so skipping them keeps a case independent of
    /// whether an event arrived between two requests.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.writer
            .write_all(format!("{request}\n").as_bytes())
            .expect("escribir");
        self.writer.flush().expect("flush del socket");
        loop {
            let response = self.read_message();
            if response["id"] == json!(id) {
                return response;
            }
        }
    }

    fn result(&mut self, method: &str, params: Value) -> Value {
        let response = self.call(method, params);
        assert!(
            response.get("error").is_none(),
            "`{method}` falló: {response}"
        );
        response["result"].clone()
    }

    /// Like [`Client::call`], but keeps the notifications that arrived before the
    /// answer: the daemon may broadcast an event before writing the response, and
    /// a count of events is only exact if neither side is discarded.
    fn call_with_events(&mut self, method: &str, params: Value) -> (Value, Vec<Value>) {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.writer
            .write_all(format!("{request}\n").as_bytes())
            .expect("escribir");
        self.writer.flush().expect("flush del socket");
        let mut events = Vec::new();
        loop {
            let response = self.read_message();
            if response["id"] == json!(id) {
                return (response, events);
            }
            events.push(response);
        }
    }

    /// Notifications already buffered by the connection, drained with a short
    /// deadline so a missing event is an empty list instead of a long wait.
    fn pending_events(&mut self) -> Vec<Value> {
        let timeout = Some(Duration::from_secs(30));
        self.reader
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(150)))
            .expect("timeout corto");
        let mut events = Vec::new();
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(message) = serde_json::from_str::<Value>(line.trim_end()) {
                        events.push(message);
                    }
                }
            }
        }
        self.reader
            .get_ref()
            .set_read_timeout(timeout)
            .expect("timeout");
        events
    }

    fn error_code(&mut self, method: &str, params: Value) -> String {
        let response = self.call(method, params);
        assert_eq!(
            response["error"]["data"]["retryable"],
            json!(false),
            "{response}"
        );
        let code = response["error"]["code"].as_str().unwrap_or_default();
        code.to_owned()
    }
}

/// Every `job_events` row of one job, as `(from_state, to_state, reason)`.
fn event_trail(case: &Case, id: &str) -> Vec<(Option<String>, String, Option<String>)> {
    let store = case.daemon.store_mut();
    let mut statement = store
        .connection()
        .prepare("SELECT job_id, from_state, to_state, reason FROM job_events ORDER BY id")
        .expect("preparar los eventos");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .expect("consultar los eventos");
    rows.filter_map(Result::ok)
        .filter(|row| row.0 == id)
        .map(|row| (row.1, row.2, row.3))
        .collect()
}

/// The `(id, state)` pairs of a `jobs.list` answer.
fn listed(list: &Value) -> Vec<(String, String)> {
    list.as_array()
        .expect("un array de trabajos")
        .iter()
        .map(|job| {
            let id = job["id"].as_str().unwrap_or_default().to_owned();
            let state = job["state"].as_str().unwrap_or_default().to_owned();
            (id, state)
        })
        .collect()
}

fn broadcasts(events: &[Value]) -> usize {
    events
        .iter()
        .filter(|event| event["method"] == json!("queue.updated"))
        .count()
}

/// One in-process daemon with its own directories and a connected client.
struct Case {
    dir: PathBuf,
    daemon: Arc<Daemon>,
    client: Client,
    _runtime: tokio::runtime::Runtime,
}

impl Case {
    fn new(tag: &str) -> Self {
        let dir = unique_dir(tag);
        let socket_path = dir.join("runtime/cauce/daemon.sock");
        let store = Store::open(&dir.join("cauce.db")).expect("abrir la base");
        let daemon = Arc::new(Daemon::new(store, vec![dir.clone(), repo_root()]));
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let listener = runtime
            .block_on(socket::bind(&socket_path))
            .expect("socket");
        runtime.spawn(serve(listener, Arc::clone(&daemon)));
        let client = Client::connect(&socket_path);
        Self {
            dir,
            daemon,
            client,
            _runtime: runtime,
        }
    }

    fn inspect_fixture(&mut self, fixture: &Path) -> Value {
        let params = json!({"paths": [fixture.to_string_lossy()]});
        self.client.result("media.inspect", params)[0].clone()
    }

    fn add_one_job(&mut self, fixture: &Path, preset: &str) -> Value {
        let dir = self.dir.join("salida");
        std::fs::create_dir_all(&dir).expect("carpeta de salida");
        let (input, output_dir) = (fixture.to_string_lossy(), dir.to_string_lossy());
        let params = json!({"inputs": [input], "output_dir": output_dir, "preset_id": preset});
        let jobs = self.client.result("jobs.add", params);
        assert_eq!(
            jobs.as_array().map(Vec::len),
            Some(1),
            "un input, un trabajo"
        );
        jobs[0].clone()
    }
}

#[test]
fn initialize_reports_protocol_version_one() {
    let mut case = Case::new("initialize");
    let init = json!({"client_name": "cauce-tests", "client_version": "0.1.0"});
    let result = case.client.result("initialize", init);
    assert_eq!(result["protocol_version"], json!(1));
    assert_eq!(result["server_version"], json!(env!("CARGO_PKG_VERSION")));
    let higher = json!({"client_name": "t", "client_version": "0", "protocol_version": 2});
    let response = case.client.call("initialize", higher);
    assert!(response.get("error").is_some(), "rechazo: {response}");
}

#[test]
fn lists_the_thirteen_presets() {
    let mut case = Case::new("presets");
    let presets = case.client.result("presets.list", json!({}));
    let ids: Vec<String> = presets
        .as_array()
        .expect("array de presets")
        .iter()
        .map(|preset| preset["id"].as_str().unwrap_or_default().to_owned())
        .collect();
    let expected: Vec<String> = system_presets().iter().map(|p| p.id.clone()).collect();
    assert_eq!(ids, expected, "el catálogo tiene trece presets");
}

#[test]
fn reads_and_writes_settings() {
    let mut case = Case::new("settings");
    let output_dir = case.dir.join("salida");
    let defaults = case.client.result("settings.get", json!({}));
    assert_eq!(defaults["max_concurrent_jobs"], json!(1));
    assert_eq!(defaults["theme"], json!("system"));
    let patch = json!({"output_dir": output_dir.to_string_lossy(), "max_concurrent_jobs": 3});
    let updated = case.client.result("settings.set", patch);
    assert_eq!(updated["max_concurrent_jobs"], json!(3));
    assert_eq!(updated["output_dir"], json!(output_dir.to_string_lossy()));
    assert_eq!(
        case.client.result("settings.get", json!({})),
        updated,
        "persistido"
    );
    let rejected = case
        .client
        .error_code("settings.set", json!({"max_concurrent_jobs": 9}));
    assert_eq!(rejected, "INVALID_INPUT", "la concurrencia se valida acá");
}

#[test]
fn reports_capabilities_with_the_real_toolchain() {
    if cauce_ffmpeg::locate().is_err() {
        eprintln!("salto el caso: FFmpeg no disponible");
        return;
    }
    let mut case = Case::new("capabilities");
    let capabilities = case.client.result("system.capabilities", json!({}));
    let version = capabilities["ffmpeg_version"].as_str().unwrap_or_default();
    assert!(!version.is_empty(), "falta FFmpeg: {capabilities}");
    assert_eq!(
        capabilities["daemon_version"],
        json!(env!("CARGO_PKG_VERSION"))
    );
    let encoders = capabilities["encoders"].as_array().expect("encoders[]");
    assert!(encoders.iter().any(|encoder| encoder == "libx264"));
    assert!(capabilities["cpu_cores"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn inspects_a_real_fixture() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("inspect");
    let snapshot = case.inspect_fixture(&fixture);
    assert_eq!(snapshot["container"], json!("mp4"));
    assert_eq!(snapshot["duration_ms"].as_u64().unwrap_or(0), 2000);
    assert_eq!(snapshot["video_streams"][0]["codec"], json!("h264"));
    assert_eq!(snapshot["audio_streams"][0]["codec"], json!("aac"));
    assert!(snapshot["size_bytes"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn plans_a_conversion() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("plan");
    let snapshot = case.inspect_fixture(&fixture);
    let params = json!({"snapshot": snapshot, "preset_id": "system-smarttv-compat"});
    let plan = case.client.result("media.plan", params);
    assert_eq!(plan["container"], json!("mp4"));
    assert!(
        !plan["reasoning"].as_array().is_some_and(Vec::is_empty),
        "{plan}"
    );
    let unknown = json!({"snapshot": snapshot, "preset_id": "system-inventado"});
    let response = case.client.call("media.plan", unknown);
    assert_eq!(response["error"]["code"], json!("PLAN_INVALID"));
}

#[test]
fn adds_jobs_and_emits_updated_events() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("jobs-add");
    let job = case.add_one_job(&fixture, "system-smarttv-compat");
    assert_eq!(job["state"], json!("queued"));
    let id = job["id"].as_str().expect("el trabajo tiene id");
    assert_eq!(id.len(), 36, "el id es un UUID: {id}");
    assert_eq!(id.chars().nth(14), Some('7'), "el id debe ser UUIDv7: {id}");
    let output = job["output_path"].as_str().unwrap_or_default();
    assert!(output.ends_with(".mp4"), "la salida sigue al preset: {job}");
    let event = case.client.read_message();
    assert_eq!(event["method"], json!("job.updated"));
    assert_eq!(event["params"]["id"], json!(id), "el evento lleva el Job");
    assert_eq!(event["params"]["state"], json!("queued"));
}

#[test]
fn lists_and_gets_jobs() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("jobs-list");
    let job = case.add_one_job(&fixture, "system-smarttv-compat");
    let id = job["id"].as_str().expect("id").to_owned();
    let fetched = case.client.result("jobs.get", json!({"id": id}));
    assert_eq!(fetched["id"], json!(id));
    assert_eq!(fetched["input_path"], job["input_path"]);
    let queued = case
        .client
        .result("jobs.list", json!({"states": ["queued"]}));
    assert_eq!(queued.as_array().map(Vec::len), Some(1));
    let running = case
        .client
        .result("jobs.list", json!({"states": ["running"]}));
    assert_eq!(running.as_array().map(Vec::len), Some(0));
    let missing = case
        .client
        .error_code("jobs.get", json!({"id": "no-existe"}));
    assert_eq!(missing, "INVALID_INPUT");
}

#[test]
fn pauses_resumes_and_cancels_a_job_through_actions() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("jobs-actions");
    let job = case.add_one_job(&fixture, "system-smarttv-compat");
    let id = job["id"].as_str().expect("id").to_owned();
    // No live child here: the case drives the store directly.
    {
        let mut store = case.daemon.store_mut();
        store
            .transition(&id, JobState::Probing, "tests")
            .expect("queued/probing");
        store
            .transition(&id, JobState::Running, "tests")
            .expect("probing/running");
    }
    let paused = case.client.result("jobs.pause", json!({"id": id}));
    let resumed = case.client.result("jobs.resume", json!({"id": id}));
    let cancelled = case.client.result("jobs.cancel", json!({"id": id}));
    assert_eq!(paused["state"], json!("paused"));
    assert_eq!(resumed["state"], json!("running"));
    assert_eq!(cancelled["state"], json!("cancelling"));
    let rejected = case.client.error_code("jobs.pause", json!({"id": id}));
    assert_eq!(rejected, "INVALID_INPUT", "`cancelling` no se pausa");
}

#[test]
fn remove_deletes_a_job_and_reports_it() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("jobs-remove");
    let job = case.add_one_job(&fixture, "system-smarttv-compat");
    let id = job["id"].as_str().expect("id").to_owned();
    let removed = case.client.result("jobs.remove", json!({"id": id}));
    assert_eq!(removed, json!({"removed": true}), "se borra");
    let again = case.client.result("jobs.remove", json!({"id": id}));
    assert_eq!(
        again,
        json!({"removed": false}),
        "borrar dos veces no falla"
    );
    let listed = case.client.result("jobs.list", json!({}));
    assert_eq!(listed.as_array().map(Vec::len), Some(0), "cola vacía");
}

#[test]
fn queue_pause_is_reported() {
    let mut case = Case::new("queue");
    let paused = case.client.result("queue.pause", json!({}));
    assert_eq!(paused["paused"], json!(true));
    let event = case.client.read_message();
    assert_eq!(event["method"], json!("queue.updated"));
    assert_eq!(event["params"]["paused"], json!(true));
    let resumed = case.client.result("queue.resume", json!({}));
    assert_eq!(resumed["paused"], json!(false));
}

#[test]
fn queue_start_unpauses_and_keeps_the_jobs() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("queue-start");
    let (paused, mut pause_events) = case.client.call_with_events("queue.pause", json!({}));
    assert_eq!(paused["result"]["paused"], json!(true));
    pause_events.extend(case.client.pending_events());
    assert_eq!(
        broadcasts(&pause_events),
        1,
        "`queue.pause` avisa una vez: {pause_events:?}"
    );

    let first = case.add_one_job(&fixture, "system-smarttv-compat");
    case.add_one_job(&fixture, "system-smarttv-compat");
    let before = case.client.result("jobs.list", json!({}));
    assert_eq!(
        before.as_array().map(Vec::len),
        Some(2),
        "dos trabajos: {before}"
    );

    let (started, mut events) = case.client.call_with_events("queue.start", json!({}));
    assert_eq!(
        started["result"],
        json!({"paused": false}),
        "`queue.start` activa la cola: {started}"
    );
    // Drained before the next request: a request swallows the events it skips.
    events.extend(case.client.pending_events());

    let after = case.client.result("jobs.list", json!({}));
    assert_eq!(
        listed(&after),
        listed(&before),
        "los mismos ids y estados: {after}"
    );
    assert_eq!(listed(&after)[0].0, first["id"], "la lista no se reordena");
    assert_eq!(
        broadcasts(&events),
        1,
        "un solo `queue.updated`: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event["params"]["paused"] == json!(false)),
        "el evento dice que la cola está activa: {events:?}"
    );
}

#[test]
fn queue_start_is_a_no_op_when_active() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("queue-start-noop");
    let job = case.add_one_job(&fixture, "system-smarttv-compat");
    let before = case.client.result("jobs.list", json!({}));
    let (started, mut events) = case.client.call_with_events("queue.start", json!({}));
    assert_eq!(started["result"], json!({"paused": false}), "{started}");
    // Drained before the next request: a request swallows the events it skips.
    events.extend(case.client.pending_events());
    let after = case.client.result("jobs.list", json!({}));
    assert_eq!(listed(&after), listed(&before), "nada cambia: {after}");
    assert_eq!(listed(&after)[0].0, job["id"], "el trabajo sigue ahí");
    assert_eq!(
        broadcasts(&events),
        1,
        "un solo `queue.updated`: {events:?}"
    );
}

#[test]
fn jobs_recover_requeues_an_interrupted_job_once() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("jobs-recover");
    let job = case.add_one_job(&fixture, "system-smarttv-compat");
    let id = job["id"].as_str().expect("id").to_owned();
    {
        let mut store = case.daemon.store_mut();
        for state in [JobState::Probing, JobState::Running, JobState::Interrupted] {
            store.transition(&id, state, "tests").expect("sembrar");
        }
    }
    assert_eq!(
        case.client.result("jobs.get", json!({"id": id}))["state"],
        json!("interrupted")
    );

    let (recovered, mut events) = case
        .client
        .call_with_events("jobs.recover", json!({"id": id}));
    assert!(recovered.get("error").is_none(), "{recovered}");
    let recovered = recovered["result"].clone();
    assert_eq!(recovered["id"], json!(id));
    assert_eq!(recovered["state"], json!("queued"), "vuelve a la cola");
    events.extend(case.client.pending_events());
    let updated = events
        .iter()
        .filter(|event| event["method"] == json!("job.updated"))
        .count();
    assert_eq!(updated, 1, "un solo `job.updated`: {events:?}");

    let again = case.client.error_code("jobs.recover", json!({"id": id}));
    assert_eq!(again, "INVALID_INPUT", "reencolar dos veces se rechaza");
    let missing = case
        .client
        .error_code("jobs.recover", json!({"id": "no-existe"}));
    assert_eq!(
        missing, "INVALID_INPUT",
        "un id desconocido no entra en pánico"
    );

    let trail = event_trail(&case, &id);
    let requeues: Vec<_> = trail
        .iter()
        .filter(|(_, to, reason)| to == "queued" && reason.as_deref() == Some("recovery"))
        .collect();
    assert_eq!(requeues.len(), 1, "un único reencolado: {trail:?}");
    assert_eq!(
        trail.last(),
        Some(&(
            Some("interrupted".to_owned()),
            "queued".to_owned(),
            Some("recovery".to_owned())
        )),
        "el último evento explica el reencolado: {trail:?}"
    );
}

#[test]
fn jobs_recover_rejects_other_states() {
    let Some(fixture) = ready() else {
        return;
    };
    let mut case = Case::new("jobs-recover-states");
    let job = case.add_one_job(&fixture, "system-smarttv-compat");
    let id = job["id"].as_str().expect("id").to_owned();
    {
        let mut store = case.daemon.store_mut();
        store
            .transition(&id, JobState::Probing, "tests")
            .expect("queued/probing");
        store
            .transition(&id, JobState::Running, "tests")
            .expect("probing/running");
    }
    let code = case.client.error_code("jobs.recover", json!({"id": id}));
    assert_eq!(code, "INVALID_INPUT", "solo `interrupted` se reencola");
    let untouched = case.client.result("jobs.get", json!({"id": id}));
    assert_eq!(untouched["state"], json!("running"), "el estado no se toca");
}

#[test]
fn unknown_method_returns_a_jsonrpc_error() {
    let mut case = Case::new("unknown");
    let response = case.client.call("no.existe", json!({}));
    assert_eq!(response["error"]["code"], json!("INTERNAL"));
    let message = response["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("método desconocido"),
        "mensaje: {response}"
    );
    let alive = case
        .client
        .result("initialize", json!({"client_name": "t"}));
    assert!(
        alive.get("protocol_version").is_some(),
        "la conexión sigue viva"
    );
}

#[test]
fn logs_list_returns_recent_entries() {
    let mut case = Case::new("logs");
    let entries = case.client.result("logs.list", json!({}));
    let entries = entries.as_array().expect("logs.list devuelve un array");
    assert!(!entries.is_empty(), "el daemon registra su arranque");
    for field in ["level", "origin", "message", "created_at"] {
        let entry = &entries[0];
        assert!(entry[field].is_string(), "falta `{field}`: {entry}");
    }
    let limited = case.client.result("logs.list", json!({"limit": 1}));
    assert_eq!(
        limited.as_array().map(Vec::len),
        Some(1),
        "`limit` se respeta"
    );
}
