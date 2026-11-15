//! The Cauce headless daemon: one Unix socket, JSON-RPC 2.0 over NDJSON, the
//! queue's single SQLite writer and one event broadcaster
//! (`docs/architecture.md` §9, `docs/contracts.md` §5-§7).
//!
//! The daemon is the only source of truth: the GUI and the CLI speak this
//! protocol and never open the database nor run FFmpeg. A connection is accepted
//! only from the daemon's own uid (`SO_PEERCRED`), every message is capped at
//! 1 MiB and a message that is not valid JSON-RPC closes the connection after a
//! final error, so a hostile client can never take the daemon down.

pub mod dispatch;
pub mod events;
pub mod ids;
pub mod protocol;
pub mod socket;

use std::io;
use std::sync::Arc;

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast::error::RecvError;

pub use dispatch::Daemon;
pub use events::{Event, Events, LogEntry};

/// Accepts connections until the listener or the caller goes away.
///
/// `daemon.status` is the first event of the process, so a client that connects
/// after start still finds the daemon alive through `initialize` and
/// `system.capabilities`.
pub async fn serve(listener: UnixListener, daemon: Arc<Daemon>) -> io::Result<()> {
    daemon
        .events()
        .daemon_status("running", env!("CARGO_PKG_VERSION"));

    loop {
        let (stream, _address) = match listener.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                daemon
                    .events()
                    .log("warn", "ipc", format!("accept falló: {error}"));
                continue;
            }
        };
        let connection = Arc::clone(&daemon);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, connection.clone()).await {
                connection.events().log(
                    "warn",
                    "ipc",
                    format!("conexión cerrada con error: {error}"),
                );
            }
        });
    }
}

/// Serves one connection: peer check, framed reads and event forwarding.
async fn handle_connection(stream: UnixStream, daemon: Arc<Daemon>) -> io::Result<()> {
    let peer = socket::peer_uid(&stream)?;
    let own = socket::current_uid();
    if peer != own {
        // No response at all: an unauthorized peer learns nothing.
        daemon.events().log(
            "warn",
            "ipc",
            format!("conexión rechazada: uid {peer} distinto de {own}"),
        );
        return Ok(());
    }
    daemon.events().log("info", "ipc", "conexión aceptada");

    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut subscriber = daemon.events().subscribe();
    let mut buffer = Vec::new();

    loop {
        tokio::select! {
            frame = socket::read_frame(&mut reader, &mut buffer) => {
                let Some(frame) = frame? else {
                    return Ok(());
                };
                let line = match frame {
                    socket::Frame::Line(bytes) => match String::from_utf8(bytes) {
                        Ok(text) => text,
                        // Not UTF-8: the same transport violation as bad JSON.
                        Err(_) => {
                            let error = transport_error("El mensaje no es texto UTF-8 válido.");
                            write_value(&mut write_half, &protocol::failure(None, &error)).await?;
                            return Ok(());
                        }
                    },
                    socket::Frame::TooLarge => {
                        let error = transport_error(&format!(
                            "El mensaje supera el límite de {} MiB: el daemon cierra la conexión.",
                            protocol::MAX_MESSAGE_BYTES / (1024 * 1024)
                        ));
                        write_value(&mut write_half, &protocol::failure(None, &error)).await?;
                        return Ok(());
                    }
                };
                if !respond(&line, &daemon, &mut write_half).await? {
                    return Ok(());
                }
            }
            event = subscriber.recv() => {
                match event {
                    Ok(event) => write_value(&mut write_half, &event.to_json()).await?,
                    // A slow client skips events instead of growing memory.
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return Ok(()),
                }
            }
        }
    }
}

/// Handles one line. Returns `false` when the connection must be closed.
async fn respond(
    line: &str,
    daemon: &Arc<Daemon>,
    writer: &mut OwnedWriteHalf,
) -> io::Result<bool> {
    let request = match protocol::parse_request(line) {
        Ok(request) => request,
        Err(error) => {
            write_value(writer, &protocol::failure(None, &error)).await?;
            return Ok(false);
        }
    };

    let method = request.method.clone();
    let params = request.params.clone();
    let owner = Arc::clone(daemon);
    let outcome =
        tokio::task::spawn_blocking(move || owner.dispatch(&method, params.as_ref())).await;

    let response = match outcome {
        Ok(Ok(result)) => protocol::success(request.id.as_ref(), result),
        Ok(Err(error)) => protocol::failure(request.id.as_ref(), &error),
        Err(error) => {
            let error = protocol::internal(format!(
                "El despacho de `{}` no pudo completarse: {error}",
                request.method
            ));
            protocol::failure(request.id.as_ref(), &error)
        }
    };
    write_value(writer, &response).await?;
    Ok(true)
}

async fn write_value(writer: &mut OwnedWriteHalf, value: &serde_json::Value) -> io::Result<()> {
    let mut encoded = serde_json::to_vec(value).map_err(io::Error::other)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    writer.flush().await
}

fn transport_error(message: &str) -> cauce_core::JobError {
    protocol::invalid(message)
}
