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
//!
//! Robustness rules that a local attacker must not be able to bend:
//!
//! - The socket path is never guessed: `XDG_RUNTIME_DIR` must be an absolute path
//!   or every call fails before any I/O (review finding R1-001).
//! - One NDJSON line is capped at the daemon's own 1 MiB (`docs/contracts.md` §5)
//!   and the event channel is bounded, so a peer cannot exhaust GUI memory
//!   (review finding R1-002).
//! - Socket open, write and flush share one deadline (review finding R4-001), and
//!   dropping a connection aborts its reader (review finding R4-002).

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

use crate::i18n::t;

/// How long one request may stay unanswered before the connection is dropped.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// How many notifications one subscriber may have buffered. A peer that pushes
/// faster than the UI drains must not grow memory without limit (finding R1-002).
pub const EVENT_BUFFER: usize = 256;

/// Client-side `<method>` of the notification that says the socket closed.
///
/// It is emitted on the event channel only: the wire protocol of
/// `docs/contracts.md` §5/§7 does not change and this notification is never sent
/// to the daemon, so the GUI learns about a crash before the next poll.
pub const DISCONNECTED_EVENT: &str = "daemon.disconnected";

/// One NDJSON message may not exceed this size, exactly like the daemon
/// (`docs/contracts.md` §5). A longer line closes the connection.
const MAX_LINE: usize = 1024 * 1024;

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
struct Pending {
    /// Abort handle of the connection's read task. A timed-out request aborts it
    /// immediately so a hung daemon cannot leak the socket, the task and the fd
    /// (review finding R3-001).
    reader: Mutex<Option<tokio::task::AbortHandle>>,
    /// Signals `supervise` to drop THIS connection. The payload is the connection
    /// generation, so a kill queued by an old connection can never drop a newer one
    /// (review regression R3-REG-001).
    kill: async_channel::Sender<u64>,
    generation: u64,
    slots: Mutex<HashMap<u64, Reply>>,
    /// Set to false when the reader sees the connection end or time out.
    alive: AtomicBool,
}

impl Pending {
    fn new(kill: async_channel::Sender<u64>, generation: u64) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            alive: AtomicBool::new(true),
            reader: Mutex::new(None),
            kill,
            generation,
        }
    }

    /// Drops the live connection from the supervisor side, closing both halves.
    fn close_connection(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.stop_reader();
        self.signal_closed();
    }

    /// Asks `supervise` to drop THIS connection without touching the read task.
    ///
    /// Used when the reader ends by itself: closing the write half with it keeps
    /// no half-open socket behind, so a peer that is still writing sees the EOF
    /// instead of blocking on a socket nobody reads.
    fn signal_closed(&self) {
        let _ = self.kill.try_send(self.generation);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Reply>> {
        self.slots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_reader(&self, handle: tokio::task::AbortHandle) {
        let mut reader = self
            .reader
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *reader = Some(handle);
    }

    /// Aborts the read task and drops its half of the socket.
    fn stop_reader(&self) {
        let handle = {
            let mut reader = self
                .reader
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            reader.take()
        };
        if let Some(handle) = handle {
            handle.abort();
        }
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
    generation: u64,
}

impl Drop for Connection {
    /// A discarded connection must stop reading immediately (review finding
    /// R4-002): aborting the read task releases its half of the socket and marks
    /// the pending map dead, so no late reply or notification survives the drop.
    fn drop(&mut self) {
        self.pending.stop_reader();
        self.pending
            .fail_all(&daemon_unavailable(t("client.discarded")));
    }
}

impl Connection {
    /// Opens the socket within `deadline`: a daemon that is not accepting must not
    /// block the call forever (review finding R4-001).
    async fn open(
        socket: PathBuf,
        subscribers: Arc<Mutex<Vec<Subscriber>>>,
        kill: async_channel::Sender<u64>,
        generation: u64,
        deadline: tokio::time::Instant,
    ) -> Result<Self, JobError> {
        let stream = match tokio::time::timeout_at(deadline, UnixStream::connect(&socket)).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => {
                return Err(daemon_unavailable(
                    &t("client.connect_failed")
                        .replace("{socket}", &socket.display().to_string())
                        .replace("{error}", &error.to_string()),
                ));
            }
            Err(_) => return Err(stalled_open()),
        };
        let (read, writer) = stream.into_split();
        let pending = Arc::new(Pending::new(kill, generation));
        let reader = Arc::clone(&pending);
        let task = tokio::spawn(async move { read_loop(read, reader, subscribers).await });
        pending.set_reader(task.abort_handle());
        Ok(Self {
            writer,
            pending,
            generation,
        })
    }

    fn is_alive(&self) -> bool {
        self.pending.alive.load(Ordering::SeqCst)
    }

    /// Writes one request line, registers the pending reply and arms the watchdog.
    ///
    /// Write and flush share the same `deadline` as the socket open: a peer that
    /// accepts and never reads fills the buffer and would otherwise hang here
    /// (review finding R4-001).
    async fn send(
        &mut self,
        id: u64,
        request: Request,
        deadline: tokio::time::Instant,
    ) -> Result<(), JobError> {
        let envelope = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": request.method,
            "params": request.params,
        });
        let mut line = serde_json::to_string(&envelope).map_err(|error| {
            JobError::new(
                ErrorCode::InvalidInput,
                t("client.serialize_failed").replace("{error}", &error.to_string()),
                false,
            )
        })?;
        line.push('\n');
        let write = async {
            self.writer.write_all(line.as_bytes()).await?;
            self.writer.flush().await
        };
        match tokio::time::timeout_at(deadline, write).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.pending.close_connection();
                return Err(unavailable_with_io(&error));
            }
            Err(_) => {
                self.pending.close_connection();
                return Err(stalled_open());
            }
        }
        self.pending.register(id, request.reply);
        let pending = Arc::clone(&self.pending);
        tokio::spawn(async move {
            tokio::time::sleep(REQUEST_TIMEOUT).await;
            if let Some(reply) = pending.take(id) {
                // A hung daemon must not keep the socket, the read task and the fd
                // alive until the next request (review finding R3-001).
                pending.close_connection();
                let error = daemon_unavailable(t("client.request_timeout"));
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
    let mut lines = Lines::new(read);
    let failure = loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
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
            Ok(None) => break daemon_unavailable(t("client.lost")),
            Err(LineError::TooLong) => break daemon_unavailable(t("client.oversized_line")),
            Err(LineError::Io) => break daemon_unavailable(t("client.lost")),
        }
    };
    // Client-side only: this event is never written to the daemon and does not
    // change the wire protocol of `docs/contracts.md` §5/§7. It is emitted once
    // per connection, before the pending replies fail, so the banner appears as
    // soon as the daemon crashes (review findings R3-002 and R4-003).
    broadcast(
        &subscribers,
        json!({"jsonrpc": "2.0", "method": DISCONNECTED_EVENT, "params": {}}),
    );
    // The write half must go away with the reader: a half-open socket would let a
    // peer keep writing into a buffer nobody drains.
    pending.signal_closed();
    pending.fail_all(&failure);
}

/// Why a read ended the connection.
enum LineError {
    /// The line grew past [`MAX_LINE`]: the peer is not following the protocol.
    TooLong,
    /// The socket failed; treated exactly like a clean EOF by the caller.
    Io,
}

/// A bounded NDJSON line reader.
///
/// `BufRead::lines` buffers an arbitrarily long line before serialising it, so a
/// peer that never sends `\n` would grow the GUI's memory without limit. This
/// reader rejects a line as soon as it passes [`MAX_LINE`] (review finding
/// R1-002) and still keeps the read side's buffering, so no data is lost between
/// iterations.
struct Lines {
    read: BufReader<tokio::net::unix::OwnedReadHalf>,
    partial: Vec<u8>,
}

impl Lines {
    fn new(read: tokio::net::unix::OwnedReadHalf) -> Self {
        Self {
            read: BufReader::new(read),
            partial: Vec::new(),
        }
    }

    /// Next line without its `\n`, `Ok(None)` at EOF.
    async fn next_line(&mut self) -> Result<Option<String>, LineError> {
        self.partial.clear();
        loop {
            let mut consumed = 0;
            let mut complete = false;
            let mut too_long = false;
            let mut eof = false;
            {
                let available = self.read.fill_buf().await.map_err(|_| LineError::Io)?;
                if available.is_empty() {
                    eof = true;
                } else if let Some(end) = available.iter().position(|byte| *byte == b'\n') {
                    if self.partial.len() + end > MAX_LINE {
                        too_long = true;
                    } else {
                        self.partial.extend_from_slice(&available[..end]);
                        complete = true;
                    }
                    consumed = end + 1;
                } else {
                    if self.partial.len() + available.len() > MAX_LINE {
                        too_long = true;
                    } else {
                        self.partial.extend_from_slice(available);
                    }
                    consumed = available.len();
                }
            }
            self.read.consume(consumed);
            if too_long {
                return Err(LineError::TooLong);
            }
            if eof {
                return Ok(if self.partial.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(&self.partial).into_owned())
                });
            }
            if complete {
                return Ok(Some(String::from_utf8_lossy(&self.partial).into_owned()));
            }
        }
    }
}

fn broadcast(subscribers: &Mutex<Vec<Subscriber>>, message: Value) {
    let mut list = subscribers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    list.retain(|subscriber| !subscriber.is_closed());
    for subscriber in list.iter() {
        offer(subscriber, &message);
    }
}

/// Bounded delivery: `force_send` replaces the oldest buffered event when the
/// channel is full, so the backlog never grows and the newest events still win
/// (review finding R1-002).
fn offer(subscriber: &Subscriber, message: &Value) {
    let _ = subscriber.force_send(message.clone());
}

/// `{"result": …}` succeeds; `{"error": {"code","message","data.retryable"}}` fails.
fn parse_response(message: &Value) -> Result<Value, JobError> {
    if let Some(error) = message.get("error") {
        let code = serde_json::from_value(error.get("code").cloned().unwrap_or(Value::Null))
            .unwrap_or(ErrorCode::Internal);
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(t("client.error_without_message"))
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
/// here — without the daemon's own fallback: it runs as a service with a valid
/// runtime dir, while guessing one for the client would let a local attacker
/// bind the socket first and impersonate the daemon (review finding R1-001).
pub fn socket_path() -> Result<PathBuf, JobError> {
    let Some(base) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    else {
        return Err(missing_runtime_dir());
    };
    Ok(base.join("cauce").join("daemon.sock"))
}

/// The refusal that replaces the old temp-dir fallback.
fn missing_runtime_dir() -> JobError {
    daemon_unavailable(t("client.no_runtime_dir"))
}

/// One deadline covers socket open, write and flush (review finding R4-001).
fn stalled_open() -> JobError {
    daemon_unavailable(t("client.open_timeout"))
}

fn daemon_unavailable(message: &str) -> JobError {
    JobError::new(ErrorCode::DaemonUnavailable, message, true)
}

fn unavailable_with_io(error: &std::io::Error) -> JobError {
    daemon_unavailable(&t("client.talk_failed").replace("{error}", &error.to_string()))
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
            return Err(daemon_unavailable(t("client.worker_unavailable")));
        }
        match answer.recv().await {
            Ok(result) => result,
            Err(_) => Err(daemon_unavailable(t("client.closed_without_answer"))),
        }
    }

    /// A receiver for the daemon's notifications (`job.updated`, `job.progress`,
    /// `queue.updated`, `log.entry`, `daemon.status`) and for the client-side
    /// [`DISCONNECTED_EVENT`].
    ///
    /// The buffer is bounded: past [`EVENT_BUFFER`] the oldest event is dropped so
    /// a chatty peer cannot exhaust memory (review finding R1-002).
    pub fn events(&self) -> async_channel::Receiver<Value> {
        let (sender, receiver) = async_channel::bounded(EVENT_BUFFER);
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
    let (kill_tx, kill_rx) = async_channel::unbounded::<u64>();
    let mut generation = 0_u64;
    loop {
        let request = tokio::select! {
            // A timed-out or broken connection is dropped here, not on the next call.
            // The generation guard keeps a stale kill from dropping a newer connection.
            killed = kill_rx.recv() => {
                if let Ok(generation) = killed
                    && connection
                        .as_ref()
                        .is_some_and(|live| live.generation == generation)
                {
                    connection = None;
                }
                continue;
            }
            request = requests.recv() => match request {
                Ok(request) => request,
                Err(_) => break,
            },
        };
        if connection.as_ref().is_some_and(|live| !live.is_alive()) {
            connection = None;
        }
        // One deadline covers socket open, write and flush (review finding R4-001).
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        if connection.is_none() {
            let socket = match socket_path() {
                Ok(socket) => socket,
                Err(error) => {
                    let _ = request.reply.try_send(Err(error));
                    continue;
                }
            };
            generation = generation.wrapping_add(1);
            match Connection::open(
                socket,
                Arc::clone(&subscribers),
                kill_tx.clone(),
                generation,
                deadline,
            )
            .await
            {
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
        if let Err(error) = live.send(next_id, request, deadline).await {
            live.pending.fail_all(&error);
            connection = None;
        }
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let socket = socket_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "<XDG_RUNTIME_DIR ausente o relativo>".to_owned());
        formatter
            .debug_struct("Client")
            .field("socket", &socket)
            .finish()
    }
}
