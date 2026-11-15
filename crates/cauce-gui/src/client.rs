//! Asynchronous JSON-RPC 2.0 client for the Cauce daemon
//! (`docs/architecture.md` §13, `docs/contracts.md` §5 and §7).
//!
//! The GUI is a pure client: it never opens SQLite and never runs FFmpeg, so all
//! it needs from the daemon is one NDJSON conversation over
//! `$XDG_RUNTIME_DIR/cauce/daemon.sock`. The connection work lives on a Tokio
//! runtime owned by the client, so nothing here ever blocks the GTK main thread:
//! [`Client::call`] is an ordinary future that the UI awaits through
//! `glib::spawn_future_local`, and [`Client::events`] hands out a receiver for
//! the notifications the daemon pushes.
//!
//! Failure policy: a missing socket, a peer that went away and a request that
//! never answers all become `DAEMON_UNAVAILABLE` (retryable, Spanish message).
//! The next call always tries a fresh connection, so a daemon that comes up late
//! is picked up without restarting the app.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cauce_core::{ErrorCode, JobError};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;

/// How long one request may stay unanswered before the connection is dropped.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Replies travel back to the awaiting caller through this channel.
type Reply = async_channel::Sender<Result<Value, JobError>>;

/// One listener of the daemon's notification stream.
type Subscriber = async_channel::Sender<Value>;

/// A pending request: a line to write plus where its answer must land.
struct Request {
    method: String,
    params: Value,
    reply: Reply,
}

/// Requests awaiting an answer, keyed by the `id` the client chose.
#[derive(Default)]
struct Pending {
    slots: Mutex<HashMap<u64, Reply>>,
    /// Set to false when the reader sees the connection end or time out.
    alive: AtomicBool,
}

impl Pending {
    fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Reply>> {
        self.slots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn register(&self, id: u64, reply: Reply) {
        self.lock().insert(id, reply);
    }

    fn take(&self, id: u64) -> Option<Reply> {
        self.lock().remove(&id)
    }

    /// Fails every waiting request with the same error and closes the socket.
    fn fail_all(&self, error: &JobError) {
        self.alive.store(false, Ordering::SeqCst);
        let waiting: Vec<Reply> = self.lock().drain().map(|(_, reply)| reply).collect();
        for reply in waiting {
            let _ = reply.try_send(Err(error.clone()));
        }
    }
}

/// One live connection: the write half plus the pending map the reader fills.
struct Connection {
    writer: OwnedWriteHalf,
    pending: Arc<Pending>,
}

impl Connection {
    async fn open(
        socket: PathBuf,
        subscribers: Arc<Mutex<Vec<Subscriber>>>,
    ) -> Result<Self, JobError> {
        let stream = UnixStream::connect(&socket)
            .await
            .map_err(|error| daemon_unavailable(&format!(
                "No se pudo conectar con el daemon en `{}`: {error}. Verificá que el servicio esté activo.",
                socket.display()
            )))?;
        let (read, writer) = stream.into_split();
        let pending = Arc::new(Pending::new());
        let reader = Arc::clone(&pending);
        tokio::spawn(async move { read_loop(read, reader, subscribers).await });
        Ok(Self { writer, pending })
    }

    fn is_alive(&self) -> bool {
        self.pending.alive.load(Ordering::SeqCst)
    }

    /// Writes one request line, registers the pending reply and arms the watchdog.
    async fn send(&mut self, id: u64, request: Request) -> Result<(), JobError> {
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": request.method,
            "params": request.params,
        });
        let mut line = serde_json::to_string(&envelope).map_err(|error| {
            JobError::new(
                ErrorCode::InvalidInput,
                format!("La petición no se pudo serializar: {error}"),
                false,
            )
        })?;
        line.push('\n');
        if let Err(error) = self.writer.write_all(line.as_bytes()).await {
            return Err(unavailable_with_io(&error));
        }
        if let Err(error) = self.writer.flush().await {
            return Err(unavailable_with_io(&error));
        }
        self.pending.register(id, request.reply);
        let pending = Arc::clone(&self.pending);
        tokio::spawn(async move {
            tokio::time::sleep(REQUEST_TIMEOUT).await;
            if let Some(reply) = pending.take(id) {
                pending.alive.store(false, Ordering::SeqCst);
                let error = daemon_unavailable(
                    "El daemon no respondió a tiempo. Se descartó la conexión; probá de nuevo.",
                );
                let _ = reply.try_send(Err(error));
            }
        });
        Ok(())
    }
}

/// Reads NDJSON until the daemon closes: answers go to their caller, anything
/// with `method` and no `id` is a notification and is broadcast.
async fn read_loop(
    read: tokio::net::unix::OwnedReadHalf,
    pending: Arc<Pending>,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
) {
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("method").is_some() {
            broadcast(&subscribers, message);
            continue;
        }
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let Some(reply) = pending.take(id) else {
            continue;
        };
        let _ = reply.try_send(parse_response(&message));
    }
    pending.fail_all(&daemon_unavailable(
        "Se perdió la conexión con el daemon. Reconectando en el próximo intento.",
    ));
}

fn broadcast(subscribers: &Mutex<Vec<Subscriber>>, message: Value) {
    let mut list = subscribers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    list.retain(|subscriber| !subscriber.is_closed());
    for subscriber in list.iter() {
        let _ = subscriber.try_send(message.clone());
    }
}

/// `{"result": …}` succeeds; `{"error": {"code","message","data.retryable"}}` fails.
fn parse_response(message: &Value) -> Result<Value, JobError> {
    if let Some(error) = message.get("error") {
        let code = serde_json::from_value(error.get("code").cloned().unwrap_or(Value::Null))
            .unwrap_or(ErrorCode::Internal);
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("El daemon devolvió un error sin mensaje.")
            .to_owned();
        let retryable = error
            .get("data")
            .and_then(|data| data.get("retryable"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Err(JobError::new(code, text, retryable));
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

/// `$XDG_RUNTIME_DIR/cauce/daemon.sock`, exactly as the daemon resolves it.
///
/// The daemon's own helper is not reachable from this crate (the GUI may not
/// depend on `cauce-daemon`), so the rule of `docs/contracts.md` §5 is repeated
/// here: the runtime dir, or the temp dir when the variable is unset.
pub fn socket_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    base.join("cauce").join("daemon.sock")
}

fn daemon_unavailable(message: &str) -> JobError {
    JobError::new(ErrorCode::DaemonUnavailable, message, true)
}

fn unavailable_with_io(error: &std::io::Error) -> JobError {
    daemon_unavailable(&format!(
        "No se pudo hablar con el daemon: {error}. Se reintentará en el próximo intento."
    ))
}

/// A cheap handle to the daemon: one runtime, one request queue, many listeners.
pub struct Client {
    requests: async_channel::Sender<Request>,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
    // Kept alive for the whole life of the client: dropping it aborts the
    // supervisor and therefore every connection attempt.
    _runtime: Option<tokio::runtime::Runtime>,
}

impl Client {
    /// Starts the IPC runtime. Never blocks and never fails: a runtime that
    /// cannot be created degrades to "the daemon is unreachable".
    pub fn connect() -> Self {
        let (requests, queue) = async_channel::unbounded::<Request>();
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("cauce-ipc")
            .build();
        match runtime {
            Ok(runtime) => {
                runtime.spawn(supervise(queue, Arc::clone(&subscribers)));
                Self {
                    requests,
                    subscribers,
                    _runtime: Some(runtime),
                }
            }
            Err(_) => {
                // No runtime means no socket conversation at all; closing the
                // queue makes every `call` answer `DAEMON_UNAVAILABLE`.
                queue.close();
                Self {
                    requests,
                    subscribers,
                    _runtime: None,
                }
            }
        }
    }

    /// Sends one JSON-RPC request and awaits its answer.
    ///
    /// Safe to await from the GTK main loop: the socket work happens on the
    /// client runtime and the returned error is always `DAEMON_UNAVAILABLE`
    /// when the daemon is missing, gone or silent.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, JobError> {
        let (reply, answer) = async_channel::bounded(1);
        let request = Request {
            method: method.to_owned(),
            params,
            reply,
        };
        if self.requests.send(request).await.is_err() {
            return Err(daemon_unavailable(
                "El cliente del daemon no está disponible. Reiniciá Cauce.",
            ));
        }
        match answer.recv().await {
            Ok(result) => result,
            Err(_) => Err(daemon_unavailable(
                "El daemon cerró la conexión sin responder. Probá de nuevo.",
            )),
        }
    }

    /// A receiver for the daemon's notifications (`job.updated`, `job.progress`,
    /// `queue.updated`, `log.entry`, `daemon.status`).
    pub fn events(&self) -> async_channel::Receiver<Value> {
        let (sender, receiver) = async_channel::unbounded();
        let mut list = self
            .subscribers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        list.retain(|subscriber| !subscriber.is_closed());
        list.push(sender);
        receiver
    }
}

/// The single owner of the connection: it connects lazily, reuses a live socket
/// and drops a dead one so the next request starts over.
async fn supervise(
    requests: async_channel::Receiver<Request>,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
) {
    let mut connection: Option<Connection> = None;
    let mut next_id = 0_u64;
    while let Ok(request) = requests.recv().await {
        if connection.as_ref().is_some_and(|live| !live.is_alive()) {
            connection = None;
        }
        if connection.is_none() {
            match Connection::open(socket_path(), Arc::clone(&subscribers)).await {
                Ok(fresh) => connection = Some(fresh),
                Err(error) => {
                    let _ = request.reply.try_send(Err(error));
                    continue;
                }
            }
        }
        let Some(live) = connection.as_mut() else {
            continue;
        };
        next_id += 1;
        if let Err(error) = live.send(next_id, request).await {
            live.pending.fail_all(&error);
            connection = None;
        }
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Client")
            .field("socket", &socket_path())
            .finish()
    }
}
