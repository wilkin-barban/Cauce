//! Runs `ffprobe` and hands its JSON to the pure parser
//! (`docs/architecture.md` §10, `docs/contracts.md` §6 `media.inspect`).
//!
//! FFprobe is spawned with an argv vector, never with a shell, and the call is
//! bounded in time so a wedged binary cannot freeze the daemon. Translating the
//! JSON stays in `cauce_core::probe`: this module only executes.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cauce_core::{ErrorCode, JobError, MediaSnapshot, Timestamp, parse_ffprobe};

use crate::toolchain::Toolchain;

/// A probe reads a container header; a minute means something is wrong.
const TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Probes `path` with the toolchain's `ffprobe` and returns a domain snapshot.
///
/// `probed_at` is supplied by the caller so the snapshot is reproducible and
/// the parser stays clock-free. A non-zero exit status means FFprobe could not
/// read the file: that is `PROBE_FAILED`, never a panic.
pub fn probe_file(
    toolchain: &Toolchain,
    path: &Path,
    probed_at: Timestamp,
) -> Result<MediaSnapshot, JobError> {
    let path_text = path.to_str().ok_or_else(|| {
        JobError::new(
            ErrorCode::InvalidInput,
            "La ruta del archivo no es UTF-8 válido; el daemon no la puede analizar.",
            false,
        )
    })?;

    let mut command = Command::new(&toolchain.ffprobe);
    command
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let (stdout, success) = capture(command)?;
    if !success {
        return Err(JobError::new(
            ErrorCode::ProbeFailed,
            format!(
                "FFprobe no pudo analizar `{path_text}`: el archivo no existe, no es multimedia \
                 o está dañado."
            ),
            false,
        ));
    }

    parse_ffprobe(path_text, probed_at, &stdout)
}

/// Spawns `command`, waits at most [`TIMEOUT`] and returns `(stdout, success)`.
fn capture(mut command: Command) -> Result<(String, bool), JobError> {
    let mut child = command.spawn().map_err(|error| {
        JobError::new(
            ErrorCode::FfmpegMissing,
            format!(
                "No se pudo ejecutar `ffprobe`: {error}. \
                 Instalá FFmpeg con `sudo apt install ffmpeg`."
            ),
            false,
        )
    })?;

    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = stdout.read_to_string(&mut buffer);
            let _ = sender.send(buffer);
        })
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(JobError::new(
                    ErrorCode::Internal,
                    format!("No se pudo esperar a `ffprobe`: {error}."),
                    true,
                ));
            }
        }
        if started.elapsed() >= TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(JobError::new(
                ErrorCode::ProbeFailed,
                format!(
                    "FFprobe no respondió en {} segundos y fue terminado.",
                    TIMEOUT.as_secs()
                ),
                false,
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    if let Some(reader) = reader {
        let _ = reader.join();
    }
    let stdout = receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_default();
    Ok((stdout, status.success()))
}
