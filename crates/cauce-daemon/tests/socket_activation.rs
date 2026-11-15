//! Systemd socket activation (`docs/architecture.md` §16).
//!
//! `cauce.socket` already listens on `%t/cauce/daemon.sock` and systemd passes
//! that descriptor; a daemon that calls `bind()` again fails with `AddrInUse`
//! and `Restart=on-failure` turns the unit into a restart loop. These cases pin
//! the two modes: the inherited descriptor (never bound, never unlinked) and the
//! development fallback that still binds and cleans up its own path.
//!
//! Every case mutates process-wide state (the environment and FD 3), so the
//! whole suite runs under one lock instead of in parallel.

use std::os::unix::io::IntoRawFd;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use cauce_daemon::socket::{self, ListenerKind};

/// Serializes the suite: the environment and FD 3 belong to the process.
fn suite_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "cauce-activation-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("carpeta temporal");
    dir
}

/// Sets or clears one variable; `None` removes it.
///
/// SAFETY: `set_var`/`remove_var` are unsafe in edition 2024 because a racing
/// thread could read a torn value. The suite holds [`suite_lock`] for every
/// case, so no other case touches the environment at the same time.
fn set_env(key: &str, value: Option<&str>) {
    unsafe {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

fn current_runtime_dir() -> Option<std::ffi::OsString> {
    std::env::var_os("XDG_RUNTIME_DIR")
}

#[test]
fn parses_listen_env_for_our_pid_only() {
    let _guard = suite_lock();
    let pid = std::process::id();

    set_env("LISTEN_PID", Some(&pid.to_string()));
    set_env("LISTEN_FDS", Some("1"));
    assert_eq!(
        socket::listen_fds_from_env(),
        Some(1),
        "un LISTEN_PID propio con LISTEN_FDS=1 anuncia un descriptor"
    );

    set_env("LISTEN_PID", Some(&(pid + 1).to_string()));
    set_env("LISTEN_FDS", Some("1"));
    assert_eq!(
        socket::listen_fds_from_env(),
        None,
        "otro pid significa que los descriptores no son nuestros"
    );

    set_env("LISTEN_PID", Some(&pid.to_string()));
    set_env("LISTEN_FDS", Some("0"));
    assert_eq!(
        socket::listen_fds_from_env(),
        None,
        "0 descriptores no activa"
    );

    set_env("LISTEN_FDS", Some("dos"));
    assert_eq!(
        socket::listen_fds_from_env(),
        None,
        "un valor no numérico cae al modo bind en vez de entrar en pánico"
    );

    set_env("LISTEN_PID", None);
    set_env("LISTEN_FDS", None);
    assert_eq!(
        socket::listen_fds_from_env(),
        None,
        "sin entorno no hay activación"
    );
}

#[test]
fn inherited_listener_accepts_a_connection_without_binding() {
    let _guard = suite_lock();
    let dir = temp_dir("inherited");
    let path = dir.join("daemon.sock");

    // The test plays systemd: a real listening socket that it hands over as
    // FD 3 (`SD_LISTEN_FDS_START`, right after stdin/stdout/stderr).
    let handed_over = std::os::unix::net::UnixListener::bind(&path).expect("bind en el test");
    // The test takes manual ownership of the descriptor, exactly like systemd
    // does when it hands a socket over to a service.
    // SAFETY: `into_raw_fd` graduates the socket to a raw descriptor that only
    // this test closes, either with the `dup2` dance or never (when it already
    // landed on FD 3).
    let raw = handed_over.into_raw_fd();
    if raw != 3 {
        // SAFETY: `dup2` on two descriptors of this process; the suite lock
        // keeps every other case away from FD 3 while it is in use.
        let duplicated = unsafe { libc::dup2(raw, 3) };
        assert_eq!(duplicated, 3, "el descriptor tiene que quedar en FD 3");
        // SAFETY: the duplicate on FD 3 owns the socket from here on, so the
        // original descriptor is closed exactly once, by this call.
        unsafe { libc::close(raw) };
    }

    // `UnixListener::from_std` registers with the reactor, so the adoption has
    // to happen inside a runtime, exactly like `main` does.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let listener = runtime
        .block_on(async { socket::inherited_listener(std::process::id(), 1) })
        .expect("FD 3 tiene que ser un socket en escucha heredado");

    // A real client, a real connect and a real accept: no stubs.
    let client = std::os::unix::net::UnixStream::connect(&path).expect("conexión real");
    let accepted = runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(5), listener.accept()).await });
    let (stream, _address) = accepted
        .expect("el accept tiene que llegar antes de 5 s")
        .expect("el accept no puede fallar");
    drop(stream);
    drop(client);

    runtime.block_on(async { drop(listener) });

    // systemd owns the socket file: dropping our handle, or releasing an
    // inherited listener, must never unlink the path.
    socket::release(&ListenerKind::Inherited);
    assert!(
        path.exists(),
        "un socket heredado no se borra: el archivo pertenece a systemd"
    );
}

#[test]
fn ignores_a_foreign_pid_or_zero_fds() {
    let _guard = suite_lock();
    let pid = std::process::id();

    // Neither case may touch FD 3: it is not ours to adopt.
    assert!(
        socket::inherited_listener(pid + 1, 1).is_none(),
        "un pid ajeno no autoriza a tomar el descriptor 3"
    );
    assert!(
        socket::inherited_listener(pid, 0).is_none(),
        "cero descriptores no activa el modo heredado"
    );
}

#[test]
fn falls_back_to_bind_without_environment() {
    let _guard = suite_lock();
    let dir = temp_dir("bound");
    let previous_runtime_dir = current_runtime_dir();
    set_env("LISTEN_PID", None);
    set_env("LISTEN_FDS", None);
    set_env(
        "XDG_RUNTIME_DIR",
        Some(dir.to_str().expect("ruta temporal UTF-8")),
    );

    assert_eq!(
        socket::listen_fds_from_env(),
        None,
        "sin LISTEN_PID/LISTEN_FDS no hay socket activation"
    );

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let (listener, kind) = runtime.block_on(socket::listener()).expect("modo bind");

    let path = dir.join("cauce").join("daemon.sock");
    match &kind {
        ListenerKind::Bound(bound) => {
            assert_eq!(bound, &path, "el modo bind usa `$XDG_RUNTIME_DIR/cauce`")
        }
        ListenerKind::Inherited => panic!("sin entorno el daemon tiene que hacer bind"),
    }
    assert_eq!(kind.label(), "bind");
    assert!(path.exists(), "el modo bind crea el socket");

    runtime.block_on(async { drop(listener) });
    socket::release(&kind);
    assert!(
        !path.exists(),
        "el socket que el daemon creó se borra al apagarse"
    );

    set_env(
        "XDG_RUNTIME_DIR",
        previous_runtime_dir.as_deref().and_then(|v| v.to_str()),
    );
}
