//! systemd user units that ship with Cauce (`docs/architecture.md` §16,
//! `docs/contracts.md` §5, `odd/tasks/cauce-0.1.md` T14).
//!
//! The units are plain text, so every assertion here is a `lines()` scan of
//! `packaging/cauce.socket` and `packaging/cauce.service`. The socket path and
//! its mode are cross-checked against the constants the daemon itself uses, so
//! the units can never drift away from the binary they start.

use std::path::{Path, PathBuf};
use std::process::Command;

use cauce_daemon::socket;

/// `/home/wilkin/proyectos/Cauce` when the tests run from the crate directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("cauce-daemon debe vivir en <raíz>/crates/cauce-daemon")
        .to_path_buf()
}

fn unit_path(name: &str) -> PathBuf {
    workspace_root().join("packaging").join(name)
}

fn read_unit(name: &str) -> String {
    let path = unit_path(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", path.display()))
}

/// Value of `key` inside `[section]`, ignoring surrounding spaces.
fn directive(unit: &str, section: &str, key: &str) -> Option<String> {
    let mut current = String::new();
    for line in unit.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current = name.trim().to_string();
            continue;
        }
        if current != section {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            if name.trim() == key {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn assert_directive(unit: &str, section: &str, key: &str, expected: &str) {
    let actual = directive(unit, section, key);
    assert_eq!(
        actual.as_deref(),
        Some(expected),
        "[{section}] {key} debe ser {expected}"
    );
}

#[test]
fn socket_unit_listens_on_the_runtime_dir() {
    let socket_unit = read_unit("cauce.socket");

    assert_directive(
        &socket_unit,
        "Socket",
        "ListenStream",
        "%t/cauce/daemon.sock",
    );
    assert_directive(&socket_unit, "Socket", "SocketMode", "0600");
    assert_directive(&socket_unit, "Socket", "DirectoryMode", "0700");
    assert_directive(&socket_unit, "Install", "WantedBy", "sockets.target");

    let description = directive(&socket_unit, "Unit", "Description");
    assert!(
        description.as_deref().is_some_and(|text| !text.is_empty()),
        "la unidad de socket necesita una [Unit] Description"
    );
    let documentation = directive(&socket_unit, "Unit", "Documentation");
    assert!(
        documentation
            .as_deref()
            .is_some_and(|value| value.contains("io.github.wilkin.Cauce")),
        "Documentation debe apuntar al id de AppStream: {documentation:?}"
    );
}

#[test]
fn service_unit_runs_the_installed_binary() {
    let service = read_unit("cauce.service");

    assert_directive(&service, "Service", "Type", "exec");
    assert_directive(&service, "Service", "ExecStart", "/usr/bin/cauce-daemon");
    assert_directive(&service, "Service", "Restart", "on-failure");
    assert_directive(&service, "Unit", "Requires", "cauce.socket");
    assert_directive(&service, "Unit", "After", "cauce.socket");

    let description = directive(&service, "Unit", "Description");
    assert!(
        description.as_deref().is_some_and(|text| !text.is_empty()),
        "la unidad de servicio necesita una [Unit] Description"
    );
}

#[test]
fn units_are_activated_by_the_user_manager() {
    let service = read_unit("cauce.service");

    assert_directive(&service, "Install", "WantedBy", "default.target");
    assert_eq!(
        directive(&service, "Service", "User"),
        None,
        "una unidad de usuario no declara [Service] User="
    );
    assert_directive(&service, "Service", "NoNewPrivileges", "yes");
}

#[test]
fn units_pass_systemd_analyze_verify() {
    if Command::new("systemd-analyze")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("systemd-analyze no está instalado: se omite la verificación de unidades");
        return;
    }

    let socket_unit = unit_path("cauce.socket");
    let service_unit = unit_path("cauce.service");
    // `--user`: both files are user units. `--no-pager` keeps the output plain
    // and the explicit paths make the check independent of `$XDG_RUNTIME_DIR`.
    let output = Command::new("systemd-analyze")
        .args(["--user", "--no-pager", "verify"])
        .arg(&service_unit)
        .arg(&socket_unit)
        .output()
        .expect("systemd-analyze no se pudo ejecutar");

    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!(
        "systemd-analyze --user --no-pager verify {} {} (exit {:?})\n{stderr}",
        service_unit.display(),
        socket_unit.display(),
        output.status.code()
    );

    // `verify` reports every diagnostic, so success is not only exit 0. Two
    // families of lines are environmental and do not describe a defect:
    //   * a dependency that is not loaded in the current manager;
    //   * `ExecStart` pointing at `/usr/bin/cauce-daemon`, which only exists
    //     after the `.deb` installs it (`docs/architecture.md` §16).
    let status = output.status.code();
    let known_diagnostic = stderr.lines().all(|line| {
        let line = line.trim();
        line.is_empty()
            || line.contains("is not loaded")
            || (line.contains("Command /usr/bin/cauce-daemon is not executable")
                && line.contains("No such file or directory"))
    });
    assert!(
        status == Some(0) || (status == Some(1) && known_diagnostic),
        "systemd-analyze verify falló con exit {status:?}: {stderr}"
    );
}

#[test]
fn socket_path_matches_the_daemon() {
    let socket_unit = read_unit("cauce.socket");
    let listen = directive(&socket_unit, "Socket", "ListenStream")
        .expect("[Socket] ListenStream debe existir");

    // `%t` is the user runtime dir, so the daemon's runtime dir name plus its
    // socket file name must be exactly what the unit declares.
    let daemon_path = socket::socket_path();
    let daemon_name = daemon_path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("el daemon necesita un nombre de socket");
    let daemon_dir = daemon_path
        .parent()
        .expect("el socket vive en una carpeta")
        .file_name()
        .and_then(|name| name.to_str())
        .expect("la carpeta del socket necesita un nombre");

    assert_eq!(
        listen,
        format!("%t/{daemon_dir}/{daemon_name}"),
        "ListenStream debe coincidir con {} (daemon.sock)",
        daemon_path.display()
    );

    let socket_mode = directive(&socket_unit, "Socket", "SocketMode").expect("SocketMode");
    assert_eq!(
        socket_mode,
        format!("{:04o}", socket::SOCKET_MODE),
        "SocketMode debe coincidir con el modo que aplica el daemon"
    );
    let directory_mode = directive(&socket_unit, "Socket", "DirectoryMode").expect("DirectoryMode");
    assert_eq!(
        directory_mode,
        format!("{:04o}", socket::DIR_MODE),
        "DirectoryMode debe coincidir con el modo que aplica el daemon"
    );
}
