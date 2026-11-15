//! Contract of the asynchronous daemon client (`docs/contracts.md` §5 and §7).
//!
//! The GUI is only a client: it never opens SQLite and never runs FFmpeg, so the
//! whole transport is tested without a display. The module under test is the
//! crate's library target, so the transport is testable without a display.

use cauce_gui::client;

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use cauce_core::ErrorCode;
use client::Client;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// `XDG_RUNTIME_DIR` is process-wide, so the four tests take turns.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unique_dir(tag: &str) -> PathBuf {
    static NEXT: Mutex<u64> = Mutex::new(0);
    let mut counter = NEXT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *counter += 1;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("gui-client")
        .join(format!("{tag}-{}-{}", std::process::id(), *counter));
    fs::create_dir_all(dir.join("cauce")).expect("carpeta temporal");
    dir
}

/// A temporary `XDG_RUNTIME_DIR` that is restored when the test ends.
struct RuntimeDir {
    path: PathBuf,
    previous: Option<OsString>,
}

impl RuntimeDir {
    fn new(tag: &str) -> Self {
        let path = unique_dir(tag);
        let previous = std::env::var_os("XDG_RUNTIME_DIR");
        // SAFETY: the guard held by every test makes this the only mutation in
        // flight, and the client resolves the path per connection attempt.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &path) };
        Self { path, previous }
    }

    fn socket(&self) -> PathBuf {
        self.path.join("cauce/daemon.sock")
    }
}

impl Drop for RuntimeDir {
    fn drop(&mut self) {
        match self.previous.take() {
            // SAFETY: see `RuntimeDir::new`.
            Some(previous) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", previous) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime de prueba")
}

/// Binds the fake daemon socket; `bind` needs the reactor to be entered.
fn bind(runtime: &tokio::runtime::Runtime, socket: &std::path::Path) -> UnixListener {
    let _guard = runtime.enter();
    UnixListener::bind(socket).expect("socket del daemon falso")
}

fn initialize_params() -> Value {
    json!({"client_name": "cauce-tests", "client_version": "0.1.0"})
}

/// Reads one request line and writes one response to the same socket.
async fn answer_initialize(stream: UnixStream) {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let request: Value = serde_json::from_str(&line).expect("petición JSON");
        let mut response = json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "result": {"protocol_version": 1},
        });
        if request["method"] == json!("initialize") {
            let greeting = json!({
                "jsonrpc": "2.0",
                "method": "job.updated",
                "params": {"id": "job-1", "state": "queued"},
            });
            response = json!([response, greeting,]);
        }
        let frames = match response {
            Value::Array(frames) => frames,
            other => vec![other],
        };
        for frame in frames {
            write
                .write_all(format!("{frame}\n").as_bytes())
                .await
                .expect("respuesta");
        }
        if request["method"] == json!("initialize") {
            write.flush().await.expect("descarga");
        }
    }
}

#[test]
fn reports_daemon_unavailable_when_the_socket_is_missing() {
    let _guard = lock_env();
    let dir = RuntimeDir::new("missing");
    assert!(!dir.socket().exists(), "el test no debe tener daemon");
    let runtime = runtime();
    let client = Client::connect();

    let error = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("sin socket no hay respuesta");

    assert_eq!(error.code, ErrorCode::DaemonUnavailable);
    assert!(error.retryable, "un socket ausente es reintentable");
    assert!(
        error.message.to_lowercase().contains("daemon"),
        "mensaje en español que nombra el daemon: {}",
        error.message
    );
}

#[test]
fn speaks_jsonrpc_over_the_socket() {
    let _guard = lock_env();
    let dir = RuntimeDir::new("jsonrpc");
    let socket = dir.socket();
    let runtime = runtime();
    let listener = bind(&runtime, &socket);
    runtime.spawn(async move {
        let (stream, _) = listener.accept().await.expect("conexión");
        answer_initialize(stream).await;
    });

    let client = Client::connect();
    let result = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("initialize responde");

    assert_eq!(result["protocol_version"], json!(1));
}

#[test]
fn delivers_notifications_to_the_event_channel() {
    let _guard = lock_env();
    let dir = RuntimeDir::new("events");
    let socket = dir.socket();
    let runtime = runtime();
    let listener = bind(&runtime, &socket);
    runtime.spawn(async move {
        let (stream, _) = listener.accept().await.expect("conexión");
        answer_initialize(stream).await;
    });

    let client = Client::connect();
    let events = client.events();
    runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("initialize responde");

    let notification = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("el evento llega a tiempo")
            .expect("el canal sigue abierto")
    });

    assert_eq!(notification["method"], json!("job.updated"));
    assert_eq!(notification["params"]["state"], json!("queued"));
    assert!(
        notification.get("id").is_none(),
        "un evento no lleva id de petición: {notification}"
    );
}

#[test]
fn reconnects_after_the_server_goes_away() {
    let _guard = lock_env();
    let dir = RuntimeDir::new("reconnect");
    let socket = dir.socket();
    let runtime = runtime();

    let listener = bind(&runtime, &socket);
    let first = runtime.spawn(async move {
        let (stream, _) = listener.accept().await.expect("conexión");
        // The first daemon answers once and then disappears with its listener.
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        if let Ok(Some(line)) = lines.next_line().await {
            let request: Value = serde_json::from_str(&line).expect("petición JSON");
            let response = json!({
                "jsonrpc": "2.0",
                "id": request["id"].clone(),
                "result": {"protocol_version": 1},
            });
            write
                .write_all(format!("{response}\n").as_bytes())
                .await
                .expect("respuesta");
        }
        drop(listener);
        drop(write);
    });

    let client = Client::connect();
    client_runtime(&runtime, &client);

    runtime
        .block_on(first)
        .expect("la primera conexión termina");

    let error = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("el daemon se fue");
    assert_eq!(error.code, ErrorCode::DaemonUnavailable);

    fs::remove_file(&socket).expect("el socket viejo se retira");
    let listener = bind(&runtime, &socket);
    runtime.spawn(async move {
        let (stream, _) = listener.accept().await.expect("conexión nueva");
        answer_initialize(stream).await;
    });

    let result = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("reconecta en la llamada siguiente");
    assert_eq!(result["protocol_version"], json!(1));
}

/// First call of the reconnect test: proves the socket really worked.
fn client_runtime(runtime: &tokio::runtime::Runtime, client: &Client) {
    let result = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("la primera llamada responde");
    assert_eq!(result["protocol_version"], json!(1));
}
