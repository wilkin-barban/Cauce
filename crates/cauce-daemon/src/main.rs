//! `cauce-daemon`: resolve paths, recover the queue, bind the socket and serve
//! until SIGINT or SIGTERM. Every decision lives in the library so this file
//! stays a wiring list (`docs/architecture.md` §9).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cauce_core::JobError;
use cauce_daemon::orchestrator::config_for;
use cauce_daemon::socket;
use cauce_daemon::{Daemon, serve, spawn_orchestrator};
use cauce_jobs::Store;

/// Set by the SIGINT/SIGTERM handler. `tokio` has no signal feature here, so the
/// flag is the smallest portable way to stop the accept loop.
static STOPPING: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_signal: libc::c_int) {
    STOPPING.store(true, Ordering::SeqCst);
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cauce-daemon: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    install_signal_handlers();

    // The daemon owns the database as its single writer (`docs/data-model.md` §1).
    socket::create_private_dir(&socket::data_dir()).map_err(describe_io)?;
    let mut store = Store::open(&socket::database_path()).map_err(describe)?;
    let interrupted = store.mark_active_as_interrupted().map_err(describe)?;
    store.release_inactive_reservations().map_err(describe)?;
    let reconciled = cauce_jobs::recovery::reconcile_temp_files(&store).map_err(describe)?;
    let daemon = Arc::new(Daemon::new(store, socket::default_authorized_roots()));

    if interrupted > 0 {
        daemon.events().log(
            "warn",
            "recovery",
            format!("{interrupted} trabajos activos quedaron `interrupted`"),
        );
    }
    if reconciled.removed > 0 || !reconciled.failures.is_empty() {
        let level = if reconciled.failures.is_empty() {
            "info"
        } else {
            "warn"
        };
        daemon.events().log(
            level,
            "recovery",
            format!(
                "reconciliación de temporales: {} eliminado(s), {} fallo(s)",
                reconciled.removed,
                reconciled.failures.len()
            ),
        );
    }

    // The scheduler owns every conversion: closing the GUI never stops a job
    // (`docs/architecture.md` §2, S9).
    let orchestrator =
        spawn_orchestrator(Arc::clone(&daemon), config_for(&daemon).map_err(describe)?);

    // Socket activation first: when `cauce.socket` already listens, systemd
    // passes that descriptor and binding the same path would fail with
    // `AddrInUse`, which `Restart=on-failure` turns into a restart loop.
    let (listener, kind) = socket::listener().await.map_err(describe_io)?;
    let mode = format!("{} en {}", kind.label(), socket::socket_path().display());
    daemon
        .events()
        .log("info", "daemon", format!("modo {mode}"));
    // The ring is only visible through `logs.list`; stderr shows the same line
    // so `journalctl --user -u cauce.service` tells the mode apart on its own.
    eprintln!("cauce-daemon: modo {mode}");

    let serving = Arc::clone(&daemon);
    tokio::select! {
        result = serve(listener, daemon) => result.map_err(describe_io)?,
        () = wait_for_signal() => {}
    }

    // Dropping the task kills every FFmpeg process group (`docs/architecture.md` §18).
    orchestrator.abort();
    serving.events().log("info", "daemon", "detenido por señal");
    // An inherited socket belongs to systemd (`cauce.socket`), so only the
    // self-bound development socket is unlinked here.
    socket::release(&kind);
    Ok(())
}

/// Polls the flag the handler sets; 50 ms is far below any user-visible cost.
async fn wait_for_signal() {
    while !STOPPING.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn install_signal_handlers() {
    let handler = on_signal as *const () as libc::sighandler_t;
    // SAFETY: `signal(2)` only stores the function pointer for SIGINT/SIGTERM;
    // the handler touches one atomic and nothing else.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

/// `JobError` is a domain value, not an `Error`; the binary prints its message.
fn describe(error: JobError) -> String {
    format!("{} ({:?})", error.message, error.code)
}

fn describe_io(error: std::io::Error) -> String {
    error.to_string()
}
