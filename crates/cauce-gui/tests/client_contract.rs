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
use std::time::{Duration, Instant};

use cauce_core::ErrorCode;
use client::{Client, REQUEST_TIMEOUT};
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

#[test]
fn a_timed_out_request_closes_the_connection() {
    // Review finding R3-001: a hung daemon must not leak the socket, the read
    // task and the fd until some later request notices the connection is dead.
    let _guard = lock_env();
    let dir = RuntimeDir::new("timeout-closes");
    let socket = dir.socket();
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind del servidor");

    let server = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Read};

        let (stream, _) = listener.accept().expect("aceptar la conexión");
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("plazo del servidor");
        let mut reader = BufReader::new(stream);
        let mut request = String::new();
        reader.read_line(&mut request).expect("leer la petición");
        assert!(
            request.contains("initialize"),
            "el cliente debe enviar la petición: {request}"
        );

        // No se responde: el cliente tiene que cerrar su extremo al vencer el plazo.
        let mut rest = Vec::new();
        let read = reader.read_to_end(&mut rest).expect("leer hasta el cierre");
        (read, rest)
    });

    let runtime = runtime();
    let client = Client::connect();
    let started = Instant::now();
    let error = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("un daemon que no responde debe vencer el plazo");

    assert_eq!(error.code, ErrorCode::DaemonUnavailable);
    assert!(error.retryable, "un plazo vencido es reintentable");
    assert!(
        started.elapsed() >= REQUEST_TIMEOUT,
        "el plazo real debe respetarse, transcurrió {:?}",
        started.elapsed()
    );

    let (read, rest) = server.join().expect("el servidor termina");
    assert_eq!(
        read, 0,
        "el cliente debe cerrar el socket (EOF), no dejarlo abierto"
    );
    assert!(rest.is_empty(), "no debe haber tráfico después del cierre");
}

#[test]
fn a_timeout_does_not_poison_the_next_request() {
    // Regression R3-REG-001: the kill signal identifies its connection generation,
    // so a kill queued by a timed-out connection must never drop the next one.
    let _guard = lock_env();
    let dir = RuntimeDir::new("timeout-then-reconnect");
    let socket = dir.socket();

    let first = std::os::unix::net::UnixListener::bind(&socket).expect("bind del primer servidor");
    let silent = std::thread::spawn(move || {
        let (stream, _) = first.accept().expect("aceptar");
        // Se queda callado a propósito: la petición vence.
        std::thread::sleep(REQUEST_TIMEOUT + Duration::from_secs(1));
        drop(stream);
    });

    let runtime = runtime();
    let client = Client::connect();
    let timed_out = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("el primer daemon no responde");
    assert_eq!(timed_out.code, ErrorCode::DaemonUnavailable);
    let _ = silent.join();

    // Segundo daemon, misma ruta: la llamada siguiente debe funcionar.
    // El socket del primer servidor queda en disco al cerrarlo; se limpia para
    // que el segundo pueda tomar la misma ruta, como haría systemd al reiniciar.
    let _ = fs::remove_file(&socket);
    let listener = bind(&runtime, &socket);
    let server_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime del servidor");
    let serving = std::thread::spawn(move || {
        server_runtime.block_on(async move {
            let (stream, _) = listener.accept().await.expect("aceptar");
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("leer");
            // El id lo elige el cliente: responder con otro lo deja esperando.
            let request: Value = serde_json::from_str(&line).expect("petición JSON");
            let reply = json!({
                "jsonrpc": "2.0",
                "id": request["id"].clone(),
                "result": { "protocol_version": 1 },
            });
            let mut write = reader.into_inner();
            write
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .expect("responder");
            write.flush().await.expect("descarga");
        })
    });

    let result = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("la petición posterior debe reconectar");
    assert_eq!(result["protocol_version"], json!(1));
    serving.join().expect("el servidor termina");
}

/// One blocking fake daemon: accepts a connection, reads the first request and
/// hands `(request, stream)` to `serve`.
fn fake_daemon(
    socket: &std::path::Path,
    serve: impl FnOnce(Value, std::os::unix::net::UnixStream) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let listener = std::os::unix::net::UnixListener::bind(socket).expect("bind del servidor");
    std::thread::spawn(move || {
        use std::io::BufRead;

        let (stream, _) = listener.accept().expect("aceptar la conexión");
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("plazo del servidor");
        let mut reader = std::io::BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("leer la petición");
        let request: Value = serde_json::from_str(&line).expect("petición JSON");
        serve(request, reader.into_inner());
    })
}

/// `docs/contracts.md` §5: one NDJSON message is at most 1 MiB.
const MAX_LINE: usize = 1024 * 1024;

/// Review finding R1-002: the event channel has a fixed capacity.
const EXPECTED_EVENT_BUFFER: usize = 256;

#[test]
fn refuses_to_guess_the_socket_path() {
    // Review finding R1-001: without an absolute `XDG_RUNTIME_DIR` the client
    // must never fall back to a shared temp directory, where a local attacker
    // could bind the socket first and impersonate the daemon.
    let _guard = lock_env();
    let previous = std::env::var_os("XDG_RUNTIME_DIR");
    let runtime = runtime();
    let client = Client::connect();

    // SAFETY: the env guard makes this the only mutation in flight.
    unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
    let unset = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("sin XDG_RUNTIME_DIR no hay socket que adivinar");
    assert_eq!(unset.code, ErrorCode::DaemonUnavailable);
    assert!(unset.retryable, "un runtime dir ausente es reintentable");
    assert!(
        unset.message.contains("XDG_RUNTIME_DIR"),
        "el mensaje debe nombrar la variable como arreglo: {}",
        unset.message
    );
    assert!(
        !unset.message.contains("No se pudo conectar"),
        "no debe intentarse ninguna conexión: {}",
        unset.message
    );

    // SAFETY: see above.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", "relative-run-dir") };
    let relative = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("una ruta relativa tampoco sirve");
    assert_eq!(relative.code, ErrorCode::DaemonUnavailable);
    assert!(relative.retryable, "una ruta relativa es reintentable");
    assert!(
        relative.message.contains("XDG_RUNTIME_DIR"),
        "el mensaje debe nombrar la variable como arreglo: {}",
        relative.message
    );
    assert!(
        !relative.message.contains("No se pudo conectar"),
        "no debe intentarse ninguna conexión: {}",
        relative.message
    );

    match previous {
        // SAFETY: see above.
        Some(previous) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", previous) },
        None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
    }
}

#[test]
fn closes_the_connection_on_an_oversized_line() {
    // Review finding R1-002: a line longer than the daemon's own 1 MiB limit
    // closes the connection instead of growing the GUI's memory.
    let _guard = lock_env();
    let dir = RuntimeDir::new("oversized-line");
    let socket = dir.socket();
    let server = fake_daemon(&socket, |_request, stream| {
        use std::io::{Read, Write};

        let mut stream = stream;
        // 2 MiB sin salto de línea: el cliente debe cerrar al pasar el tope.
        let oversized = vec![b'x'; 2 * MAX_LINE];
        let _ = stream.write_all(&oversized);
        let _ = stream.flush();
        let mut rest = Vec::new();
        // El cliente cierra su extremo: la lectura vuelve con 0 bytes.
        let _ = stream.read_to_end(&mut rest);
    });

    let runtime = runtime();
    let client = Client::connect();
    let events = client.events();
    let error = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("una línea sobredimensionada cierra la conexión");

    assert_eq!(error.code, ErrorCode::DaemonUnavailable);
    assert!(error.retryable, "una línea inválida es reintentable");
    assert!(
        error.message.contains("1 MiB"),
        "el mensaje debe nombrar el tope: {}",
        error.message
    );

    let disconnect = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("el evento de desconexión llega a tiempo")
            .expect("el canal sigue abierto")
    });
    assert_eq!(disconnect["method"], json!("daemon.disconnected"));
    server.join().expect("el servidor termina");
}

#[test]
fn event_buffer_is_bounded() {
    // Review finding R1-002: the event channel drops the oldest event instead of
    // growing without limit, so a chatty peer cannot exhaust the GUI's memory.
    assert_eq!(
        client::EVENT_BUFFER,
        EXPECTED_EVENT_BUFFER,
        "el tope publicado coincide con el del canal"
    );
    let _guard = lock_env();
    let dir = RuntimeDir::new("bounded-events");
    let socket = dir.socket();
    let (written, written_rx) = std::sync::mpsc::channel();
    let server = fake_daemon(&socket, move |request, stream| {
        use std::io::Write;

        let mut stream = stream;
        let reply = json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "result": {"protocol_version": 1},
        });
        let mut frames = format!("{reply}\n");
        for index in 0..(EXPECTED_EVENT_BUFFER * 2) {
            let frame = json!({
                "jsonrpc": "2.0",
                "method": "job.progress",
                "params": {"id": "job-1", "index": index},
            });
            frames.push_str(&format!("{frame}\n"));
        }
        stream.write_all(frames.as_bytes()).expect("los eventos");
        stream.flush().expect("descarga");
        let _ = written.send(());
        // Se queda abierto para que el canal siga vivo durante el drenaje.
        std::thread::sleep(Duration::from_secs(2));
    });

    let runtime = runtime();
    let client = Client::connect();
    let events = client.events();
    runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("initialize responde antes de la inundación");
    written_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("el servidor escribió los eventos");
    std::thread::sleep(Duration::from_secs(1));

    let mut count = 0;
    while events.try_recv().is_ok() {
        count += 1;
    }
    assert_eq!(
        count,
        client::EVENT_BUFFER,
        "el canal conserva como máximo el tope y descarta lo viejo: {count}"
    );
    assert!(!events.is_closed(), "el receptor sigue vivo tras el tope");
    let _ = server.join();
}

#[test]
fn emits_a_disconnect_notification_when_the_daemon_closes() {
    // Review findings R3-002 and R4-003: the reader announces the disconnect so
    // the banner appears at once instead of waiting for the next `daemon.status`.
    let _guard = lock_env();
    let dir = RuntimeDir::new("disconnect-event");
    let socket = dir.socket();
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind del servidor");
    let server = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};

        let (stream, _) = listener.accept().expect("aceptar la conexión");
        let mut reader = BufReader::new(stream);
        let mut first = String::new();
        reader.read_line(&mut first).expect("leer la petición");
        let request: Value = serde_json::from_str(&first).expect("petición JSON");
        let reply = json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "result": {"protocol_version": 1},
        });
        reader
            .get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .expect("respuesta");
        reader.get_mut().flush().expect("descarga");
        // Segunda petición: se cierra sin responder, como un daemon que cae.
        let mut second = String::new();
        reader
            .read_line(&mut second)
            .expect("leer la segunda petición");
    });

    let runtime = runtime();
    let client = Client::connect();
    let events = client.events();
    runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("initialize responde");
    let pending = runtime
        .block_on(client.call("jobs.list", json!({})))
        .expect_err("el daemon se cerró con la petición pendiente");
    assert_eq!(pending.code, ErrorCode::DaemonUnavailable);
    assert!(pending.retryable, "una desconexión es reintentable");

    let notification = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("la desconexión llega a tiempo")
            .expect("el canal sigue abierto")
    });
    assert_eq!(notification["jsonrpc"], json!("2.0"));
    assert_eq!(notification["method"], json!("daemon.disconnected"));
    assert_eq!(notification["params"], json!({}));
    assert!(
        notification.get("id").is_none(),
        "un evento no lleva id de petición: {notification}"
    );
    server.join().expect("el servidor termina");
}

#[test]
fn a_stalled_open_times_out() {
    // Review finding R4-001: opening the socket, writing and flushing share one
    // deadline, so a listener that accepts and never reads cannot hang a call.
    let _guard = lock_env();
    let dir = RuntimeDir::new("stalled-write");
    let socket = dir.socket();
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind del servidor");
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("aceptar la conexión");
        // Nunca lee: el búfer del socket se llena y la escritura se bloquea.
        std::thread::sleep(REQUEST_TIMEOUT + Duration::from_secs(5));
        drop(stream);
    });

    let runtime = runtime();
    let client = Client::connect();
    let started = Instant::now();
    // 8 MiB de parámetros no caben en el búfer del socket.
    let inputs: Vec<String> = (0..128).map(|_| "x".repeat(64 * 1024)).collect();
    let error = runtime
        .block_on(client.call("jobs.add", json!({"inputs": inputs})))
        .expect_err("una escritura estancada debe vencer el plazo");
    let elapsed = started.elapsed();

    assert_eq!(error.code, ErrorCode::DaemonUnavailable);
    assert!(error.retryable, "un plazo vencido es reintentable");
    assert!(
        elapsed < REQUEST_TIMEOUT + Duration::from_secs(2),
        "la llamada debe rendirse en el plazo, tardó {elapsed:?}"
    );
    assert!(
        error.message.contains("daemon"),
        "mensaje en español que nombra el daemon: {}",
        error.message
    );
    server.join().expect("el servidor termina");
}

#[test]
fn a_discarded_connection_stops_reading() {
    // Review finding R4-002: a discarded connection aborts its reader, so the
    // socket really closes and no late notification survives the drop.
    let _guard = lock_env();
    let dir = RuntimeDir::new("discarded-connection");
    let socket = dir.socket();
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind del servidor");
    let server = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Read, Write};

        let (stream, _) = listener.accept().expect("aceptar la conexión");
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("plazo del servidor");
        let mut reader = BufReader::new(stream);
        let mut request = String::new();
        reader.read_line(&mut request).expect("leer la petición");
        // Nunca responde: el plazo del cliente descarta la conexión.
        let mut rest = Vec::new();
        let read = reader.read_to_end(&mut rest).expect("leer hasta el EOF");
        // Notificación tardía: ningún lector debe quedar para recibirla.
        let late = json!({
            "jsonrpc": "2.0",
            "method": "job.updated",
            "params": {"id": "job-1"},
        });
        let _ = reader.get_mut().write_all(format!("{late}\n").as_bytes());
        let _ = reader.get_mut().flush();
        read
    });

    let runtime = runtime();
    let client = Client::connect();
    let events = client.events();
    let error = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect_err("sin respuesta vence el plazo");
    assert_eq!(error.code, ErrorCode::DaemonUnavailable);

    assert_eq!(
        server.join().expect("el servidor termina"),
        0,
        "el servidor debe observar el EOF del cliente"
    );
    let late = runtime
        .block_on(async { tokio::time::timeout(Duration::from_millis(500), events.recv()).await });
    assert!(
        late.is_err(),
        "ninguna notificación tardía sobrevive al descarte: {late:?}"
    );
}
