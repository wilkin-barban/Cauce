//! Transport security of the daemon socket (`docs/contracts.md` §5 and §9).
//!
//! Every test drives a real in-process server over a real Unix socket: no
//! binary is spawned and each case owns its own `XDG_RUNTIME_DIR` and database
//! under `std::env::temp_dir()`, so the cases never share a socket or a file.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cauce_daemon::protocol::MAX_MESSAGE_BYTES;
use cauce_daemon::{Daemon, serve, socket};
use cauce_jobs::Store;
use serde_json::{Value, json};
use tokio::net::{UnixListener, UnixStream as TokioStream};

/// Unique directory per test: process id plus nanoseconds.
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("cauce-ipc-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("no se pudo crear el directorio temporal");
    dir
}

/// An in-process daemon listening on its own socket inside `dir`.
struct Server {
    socket_path: PathBuf,
    _dir: PathBuf,
    _runtime: tokio::runtime::Runtime,
}

impl Server {
    fn start(tag: &str) -> Self {
        let dir = unique_dir(tag);
        let socket_path = dir.join("runtime/cauce/daemon.sock");
        let store = Store::open(&dir.join("cauce.db")).expect("no se pudo abrir la base");
        let daemon = Arc::new(Daemon::new(store, vec![dir.clone()]));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("no se pudo crear el runtime");

        let path = socket_path.clone();
        runtime.block_on(async move {
            let listener = socket::bind(&path)
                .await
                .expect("no se pudo abrir el socket");
            tokio::spawn(serve(listener, daemon));
        });

        Self {
            socket_path,
            _dir: dir,
            _runtime: runtime,
        }
    }
}

/// Minimal blocking JSON-RPC client: one line out, one line in.
struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    fn connect(path: &Path) -> Self {
        let stream = UnixStream::connect(path).expect("no se pudo conectar al socket");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("no se pudo fijar el timeout de lectura");
        let reader = BufReader::new(stream.try_clone().expect("no se pudo clonar el socket"));
        Self {
            reader,
            writer: stream,
        }
    }

    /// Writes raw bytes without a trailing newline, so a case can exceed the
    /// message limit before the frame ends.
    fn send_raw(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    fn send_line(&mut self, line: &str) {
        self.send_raw(line.as_bytes());
        self.send_raw(b"\n");
    }

    /// Reads one message; `None` means the daemon closed the connection.
    fn read_message(&mut self) -> Option<Value> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(serde_json::from_str(line.trim_end()).expect("respuesta JSON-RPC")),
        }
    }

    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.send_line(&request.to_string());
        self.read_message().expect("el daemon cerró la conexión")
    }
}

fn error_code(response: &Value) -> String {
    response["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn socket_is_created_with_0600() {
    let dir = unique_dir("socket-mode");
    let path = dir.join("runtime/cauce/daemon.sock");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let listener = runtime
        .block_on(socket::bind(&path))
        .expect("no se pudo abrir el socket");

    let mode = std::fs::metadata(&path)
        .expect("falta el socket")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "el socket debe quedar con modo 0600");
    drop(listener);
}

#[test]
fn directory_is_created_with_0700() {
    let dir = unique_dir("dir-mode");
    let path = dir.join("runtime/cauce/daemon.sock");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let listener = runtime
        .block_on(socket::bind(&path))
        .expect("no se pudo abrir el socket");

    let socket_dir = path.parent().expect("el socket tiene carpeta");
    let mode = std::fs::metadata(socket_dir)
        .expect("falta la carpeta del socket")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o700,
        "la carpeta del socket debe quedar con modo 0700"
    );
    drop(listener);
}

#[test]
fn oversized_message_closes_the_connection() {
    let server = Server::start("oversize");
    let mut client = Client::connect(&server.socket_path);

    client.send_raw(&vec![b'x'; MAX_MESSAGE_BYTES + 1]);

    let response = client.read_message().expect("falta el error final");
    assert_eq!(error_code(&response), "INVALID_INPUT");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("MiB"),
        "el mensaje debe nombrar el límite: {response}"
    );
    assert!(
        client.read_message().is_none(),
        "el daemon debe cerrar la conexión tras el error"
    );
}

#[test]
fn invalid_json_closes_the_connection() {
    let server = Server::start("invalid-json");
    let mut client = Client::connect(&server.socket_path);

    client.send_line("esto no es JSON-RPC");

    let response = client.read_message().expect("falta el error final");
    assert_eq!(error_code(&response), "INVALID_INPUT");
    assert!(response["id"].is_null(), "el error sin id debe ir con null");
    assert!(
        client.read_message().is_none(),
        "el daemon debe cerrar la conexión tras el error"
    );
}

#[test]
fn peer_uid_matches_the_test_process() {
    let dir = unique_dir("peer-cred");
    let path = dir.join("daemon.sock");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let (peer, expected) = runtime.block_on(async {
        let listener = UnixListener::bind(&path).expect("no se pudo abrir el socket de prueba");
        let client = TokioStream::connect(&path)
            .await
            .expect("no se pudo conectar");
        let (server_side, _address): (TokioStream, _) =
            listener.accept().await.expect("no se pudo aceptar");
        let uid = socket::peer_uid(&server_side).expect("no se pudo leer SO_PEERCRED");
        drop(client);
        (uid, socket::current_uid())
    });

    assert_eq!(peer, expected, "SO_PEERCRED debe informar la UID propia");
    assert_eq!(expected, unsafe { libc::geteuid() });
}

#[test]
fn rejects_a_relative_path() {
    let server = Server::start("relative");
    let mut client = Client::connect(&server.socket_path);

    let response = client.call(1, "media.inspect", json!({"paths": ["media/relativa.mp4"]}));

    assert_eq!(error_code(&response), "INVALID_INPUT");
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("rutas absolutas"),
        "el error debe explicar que la ruta no es absoluta: {response}"
    );
    assert!(
        response.get("result").is_none(),
        "una ruta relativa nunca produce resultado"
    );
}

#[test]
fn denies_a_path_outside_the_authorized_roots() {
    let server = Server::start("path-denied");
    let mut client = Client::connect(&server.socket_path);

    // `/etc/hostname` always exists and is never inside a test root.
    let response = client.call(1, "media.inspect", json!({"paths": ["/etc/hostname"]}));

    assert_eq!(error_code(&response), "PATH_DENIED");
    assert_eq!(response["error"]["data"]["retryable"], json!(false));
}
