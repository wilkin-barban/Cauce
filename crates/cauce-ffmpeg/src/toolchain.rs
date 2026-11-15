//! System FFmpeg locator and capability report
//! (`docs/architecture.md` §10, `docs/contracts.md` §6 `system.capabilities`).
//!
//! FFmpeg is a package dependency, never a bundled binary (S2): this module only
//! looks for it in `PATH`, never downloads anything. Every process is spawned
//! with an argv vector — no shell, no command string — and dies after a hard
//! timeout so a wedged binary cannot freeze the daemon.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cauce_core::command::{REQUIRED_ENCODERS, REQUIRED_FILTERS};
use cauce_core::{Capabilities, ErrorCode, JobError};

const FFMPEG: &str = "ffmpeg";
const FFPROBE: &str = "ffprobe";
/// Hard wall-clock limit for a single FFmpeg/FFprobe/`df` invocation.
const TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
/// Ubuntu 26.04 baseline (`docs/architecture.md` §10).
const BASELINE_MAJOR: u32 = 8;
/// The only fix a user can apply, so every "missing" message carries it.
const INSTALL_HINT: &str = "Instalá FFmpeg con `sudo apt install ffmpeg`.";

/// Absolute paths of the two binaries Cauce is allowed to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

/// Locates the toolchain in the process `PATH`.
pub fn locate() -> Result<Toolchain, JobError> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    locate_in(&path.to_string_lossy())
}

/// Same rules as [`locate`], but on an explicit `PATH` string so it is testable.
///
/// A candidate must be an executable regular file, and both binaries are
/// required: FFprobe alone cannot convert, FFmpeg alone cannot validate.
pub fn locate_in(search_path: &str) -> Result<Toolchain, JobError> {
    let ffmpeg = find_in(search_path, FFMPEG);
    let ffprobe = find_in(search_path, FFPROBE);
    match (ffmpeg, ffprobe) {
        (Some(ffmpeg), Some(ffprobe)) => Ok(Toolchain { ffmpeg, ffprobe }),
        (None, _) => Err(missing(format!(
            "No se encontró `ffmpeg` en {search_path}. {INSTALL_HINT}"
        ))),
        (Some(_), None) => Err(missing(format!(
            "No se encontró `ffprobe`, que FFmpeg instala junto al codificador, \
             en {search_path}. {INSTALL_HINT}"
        ))),
    }
}

fn find_in(search_path: &str, binary: &str) -> Option<PathBuf> {
    std::env::split_paths(search_path)
        .filter(|directory| !directory.as_os_str().is_empty())
        .map(|directory| directory.join(binary))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

/// Version token of `ffmpeg -version`, for example `8.0.1`.
pub fn ffmpeg_version(toolchain: &Toolchain) -> Result<String, JobError> {
    version_of(&toolchain.ffmpeg, FFMPEG)
}

/// Version token of `ffprobe -version`, for example `8.0.1`.
pub fn ffprobe_version(toolchain: &Toolchain) -> Result<String, JobError> {
    version_of(&toolchain.ffprobe, FFPROBE)
}

fn version_of(binary: &Path, name: &str) -> Result<String, JobError> {
    let banner = run_text(binary, &["-hide_banner", "-version"])?;
    parse_version(&banner).ok_or_else(|| {
        missing(format!(
            "El binario `{name}` en {} no informa una versión de FFmpeg reconocible. {INSTALL_HINT}",
            binary.display()
        ))
    })
}

fn parse_version(banner: &str) -> Option<String> {
    let mut words = banner.lines().next()?.split_whitespace();
    while let Some(word) = words.next() {
        if word == "version" || word.ends_with("version") {
            return leading_version(words.next()?);
        }
    }
    None
}

/// `8.0.1-3ubuntu2+esm5` → `8.0.1`; garbage → `None`.
fn leading_version(token: &str) -> Option<String> {
    let end = token
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(token.len());
    let candidate = &token[..end];
    let mut parts = candidate.split('.');
    let major = parts.next()?;
    if major.is_empty() || parts.next().is_none() {
        return None;
    }
    Some(candidate.to_string())
}

fn major_of(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}

/// Sorted, deduplicated encoder names from `ffmpeg -encoders`.
pub fn encoders(toolchain: &Toolchain) -> Result<Vec<String>, JobError> {
    let table = run_text(&toolchain.ffmpeg, &["-hide_banner", "-encoders"])?;
    Ok(table_names(&table))
}

/// Sorted, deduplicated filter names from `ffmpeg -filters`.
pub fn filters(toolchain: &Toolchain) -> Result<Vec<String>, JobError> {
    let table = run_text(&toolchain.ffmpeg, &["-hide_banner", "-filters"])?;
    Ok(table_names(&table))
}

/// Reads the second column of the first table in an FFmpeg listing.
///
/// The headers of both listings are prose or `flag = meaning` lines, so the
/// first line that is neither starts the table; the `------` rule of the encoder
/// listing has no second column and is ignored by the same rule.
fn table_names(text: &str) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    let mut in_table = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !in_table {
            if trimmed.is_empty() || trimmed.contains('=') || trimmed.ends_with(':') {
                continue;
            }
            in_table = true;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(name) = trimmed.split_whitespace().nth(1) {
            names.insert(name.to_string());
        }
    }
    names.into_iter().collect()
}

/// Full capability report. `daemon_version` stays empty: the daemon sets it.
///
/// A toolchain that exists but lacks a required encoder or filter still yields
/// `Ok`: the diagnostic page needs the list to offer a concrete action.
pub fn inspect(toolchain: &Toolchain, disk_path: &Path) -> Result<Capabilities, JobError> {
    let ffmpeg_version = ffmpeg_version(toolchain)?;
    let ffprobe_version = ffprobe_version(toolchain)?;
    let encoders = encoders(toolchain)?;
    let filters = filters(toolchain)?;

    let mut ffmpeg_missing = Vec::new();
    let mut warnings = Vec::new();
    for encoder in REQUIRED_ENCODERS {
        if !encoders.iter().any(|found| found == encoder) {
            ffmpeg_missing.push(encoder.to_string());
            warnings.push(format!(
                "Esta compilación de FFmpeg no trae el codificador `{encoder}`, \
                 que exige el MVP. {INSTALL_HINT}"
            ));
        }
    }
    for filter in REQUIRED_FILTERS {
        if !filters.iter().any(|found| found == filter) {
            ffmpeg_missing.push(filter.to_string());
            warnings.push(format!(
                "Esta compilación de FFmpeg no trae el filtro `{filter}`, \
                 que exige el MVP. {INSTALL_HINT}"
            ));
        }
    }
    if let Some(major) = major_of(&ffmpeg_version) {
        if major < BASELINE_MAJOR {
            warnings.push(format!(
                "FFmpeg {ffmpeg_version} es anterior a la línea base {BASELINE_MAJOR}.0; \
                 actualizalo con `sudo apt install --only-upgrade ffmpeg`."
            ));
        }
    }

    Ok(Capabilities {
        ffmpeg_version,
        ffprobe_version,
        encoders,
        filters,
        cpu_cores: cpu_cores(),
        disk_free_bytes: disk_free(disk_path)?,
        daemon_version: String::new(),
        ffmpeg_missing,
        warnings,
    })
}

fn cpu_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(1)
}

/// Free bytes on the filesystem that holds `path`, straight from `df`.
fn disk_free(path: &Path) -> Result<u64, JobError> {
    let mut command = Command::new("df");
    command.args(["-B1", "--output=avail"]).arg(path);
    let table = capture(command, "df")?;
    table
        .lines()
        .skip(1)
        .find_map(|line| line.trim().parse::<u64>().ok())
        .ok_or_else(|| {
            JobError::new(
                ErrorCode::Internal,
                format!(
                    "`df` no informó espacio libre para {}; revisá el punto de montaje.",
                    path.display()
                ),
                true,
            )
        })
}

fn run_text(binary: &Path, args: &[&str]) -> Result<String, JobError> {
    let label = binary.display().to_string();
    let mut command = Command::new(binary);
    command.args(args);
    capture(command, &label)
}

/// Spawns `command`, waits at most [`TIMEOUT`] and returns its stdout.
///
/// The bounded wait loop is deliberate: `std` has no `wait_timeout`, and adding
/// a dependency for it would be a bigger contract than T9 needs.
fn capture(mut command: Command, label: &str) -> Result<String, JobError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|error| {
        missing(format!(
            "No se pudo ejecutar `{label}`: {error}. {INSTALL_HINT}"
        ))
    })?;

    let stdout = child.stdout.take();
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = stdout.map(|mut stdout| {
        std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = stdout.read_to_string(&mut buffer);
            let _ = sender.send(buffer);
        })
    });

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(JobError::new(
                    ErrorCode::Internal,
                    format!("No se pudo esperar a `{label}`: {error}"),
                    true,
                ));
            }
        }
        if started.elapsed() >= TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(JobError::new(
                ErrorCode::Internal,
                format!(
                    "`{label}` no respondió en {} segundos y fue terminado.",
                    TIMEOUT.as_secs()
                ),
                true,
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    if let Some(reader) = reader {
        let _ = reader.join();
    }
    // The child is gone, so the pipe closes and the reader returns the buffer.
    Ok(receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_default())
}

fn missing(message: String) -> JobError {
    JobError::new(ErrorCode::FfmpegMissing, message, false)
}
