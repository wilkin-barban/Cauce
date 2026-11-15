//! Smoke test of the real GUI binary under Xvfb (`docs/ui-mvp.md` §12 and §14).
//!
//! `CAUCE_SMOKE=1` makes the shell print `window-ready` after its first map and
//! quit on its own, so the test proves that a window really opened instead of
//! trusting that the process started. A missing `xvfb-run`/`Xvfb` fails the test
//! with what to install instead of skipping it silently (review finding R3-003).

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(30);

fn in_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(tool))
                .any(|candidate| candidate.is_file())
        })
        .unwrap_or(false)
}

/// Fails loudly instead of skipping: a green run must mean a window really
/// opened, so a missing display tool is a broken environment, not a pass.
fn require_display_tooling() {
    let missing: Vec<&str> = ["xvfb-run", "Xvfb"]
        .into_iter()
        .filter(|tool| !in_path(tool))
        .collect();
    assert!(
        missing.is_empty(),
        "faltan {missing:?} en PATH: instalá Xvfb (`apt install xvfb`) para correr el smoke de la GUI sin display real"
    );
}

/// A fresh `XDG_RUNTIME_DIR` so the test never talks to the host's real daemon.
fn empty_runtime_dir(tag: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("gui-smoke")
        .join(format!("{tag}-{}", std::process::id()));
    fs::create_dir_all(dir.join("cauce")).expect("carpeta temporal");
    dir
}

/// Runs the binary through `xvfb-run` and returns its exit code and stdout.
fn run_shell(tag: &str) -> (i32, String) {
    let runtime_dir = empty_runtime_dir(tag);
    let mut child: Child = Command::new("xvfb-run")
        .arg("-a")
        .arg(env!("CARGO_BIN_EXE_cauce-gui"))
        .env("CAUCE_SMOKE", "1")
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        // Each run gets a dead bus address: GApplication must register locally
        // instead of forwarding activation to the smoke of the other test.
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", runtime_dir.join("no-session-bus").display()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(
            fs::File::create(runtime_dir.join("stderr.log")).expect("log del smoke"),
        ))
        .spawn()
        .expect("el binario de la GUI debe arrancar bajo xvfb-run");

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait().expect("estado del proceso") {
            Some(_) => break,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("la ventana no se abrió y no terminó en {TIMEOUT:?}");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }

    let output = child.wait_with_output().expect("salida del proceso");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = fs::read_to_string(runtime_dir.join("stderr.log")).unwrap_or_default();
    if !stderr.trim().is_empty() {
        eprintln!("stderr del shell:\n{stderr}");
    }
    (code, stdout)
}

#[test]
fn window_opens_under_xvfb() {
    require_display_tooling();
    let (code, stdout) = run_shell("open");
    assert_eq!(code, 0, "el shell debe salir con 0; stdout: {stdout}");
    assert!(
        stdout.contains("window-ready"),
        "stdout debe declarar la primera map: {stdout:?}"
    );
}

#[test]
fn window_opens_without_a_daemon() {
    require_display_tooling();
    let (code, stdout) = run_shell("no-daemon");
    assert_eq!(
        code, 0,
        "sin daemon la ventana igual abre (el banner no es un crash): {stdout}"
    );
    assert!(
        stdout.contains("window-ready"),
        "sin daemon también hay primera map: {stdout:?}"
    );
}
