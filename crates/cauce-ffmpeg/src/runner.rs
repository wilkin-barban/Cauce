//! FFmpeg process runner: real progress, stderr forwarding and process control.
//!
//! `docs/architecture.md` §6 step 8 runs FFmpeg with `-progress pipe:1`;
//! §7 pauses with `SIGSTOP`, resumes with `SIGCONT` and cancels with
//! `SIGTERM`→`SIGKILL`; §18 forbids orphaned processes. Every child gets its own
//! process group and every signal goes to `-pid`, so FFmpeg's own children die
//! with it. There is no shell anywhere: the argv vector is passed as-is.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cauce_core::{ErrorCode, JobError, Progress};

use crate::progress::{ProgressParser, for_each_line, utc_now};

/// At least this much wall clock between two progress callbacks
/// (`docs/contracts.md` §7: at most 2 Hz).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
/// Polling step while `cancel` waits out its grace period.
const CANCEL_POLL: Duration = Duration::from_millis(10);

/// A running FFmpeg conversion, owned by whoever scheduled the job.
///
/// Dropping it kills the whole process group and reaps the child, so a job that
/// is dropped mid-flight never leaves FFmpeg (or `sleep`, or a muxer helper)
/// running behind the daemon's back.
#[derive(Debug)]
pub struct ConversionChild {
    pid: u32,
    child: Option<Child>,
    status: Option<ExitStatus>,
    threads: Vec<JoinHandle<()>>,
    paused: bool,
    cancelled: bool,
}

/// Runs `ffmpeg` with `args` in its own process group.
///
/// `total_ms` comes from the FFprobe snapshot of the input and feeds `percent`
/// and the ETA. Progress callbacks are throttled to 2 Hz; the closing
/// `progress=end` block is always delivered.
pub fn spawn(
    ffmpeg: &Path,
    args: &[String],
    total_ms: u64,
    on_progress: Box<dyn FnMut(Progress) + Send>,
    on_stderr: Box<dyn FnMut(&str) + Send>,
) -> Result<ConversionChild, JobError> {
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(ffmpeg);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A group of its own: `process_group(0)` makes the child the leader, so
        // `kill(-pid, …)` reaches FFmpeg and everything it spawned.
        .process_group(0);

    let mut child = command.spawn().map_err(|error| {
        JobError::new(
            ErrorCode::FfmpegMissing,
            format!(
                "No se pudo ejecutar `{}`: {error}. Instalá FFmpeg con `sudo apt install ffmpeg`.",
                ffmpeg.display()
            ),
            false,
        )
    })?;

    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal(pid, "no se pudo capturar la salida de progreso"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| internal(pid, "no se pudo capturar la salida de errores"))?;

    let threads = vec![
        thread::spawn(move || pump_progress(stdout, total_ms, on_progress)),
        thread::spawn(move || pump_stderr(stderr, on_stderr)),
    ];

    Ok(ConversionChild {
        pid,
        child: Some(child),
        status: None,
        threads,
        paused: false,
        cancelled: false,
    })
}

impl ConversionChild {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// `SIGSTOP` to the process group. Pausing twice is idempotent.
    pub fn pause(&mut self) -> Result<(), JobError> {
        if self.paused {
            return Ok(());
        }
        self.ensure_running()?;
        signal_group(self.pid, libc::SIGSTOP)?;
        self.paused = true;
        Ok(())
    }

    /// `SIGCONT` to the process group. Resuming a child that is not paused is a
    /// no-op, not an error.
    pub fn resume(&mut self) -> Result<(), JobError> {
        if !self.paused {
            return Ok(());
        }
        self.ensure_running()?;
        signal_group(self.pid, libc::SIGCONT)?;
        self.paused = false;
        Ok(())
    }

    /// `SIGTERM` to the group, then `SIGKILL` if the grace period expires.
    ///
    /// Returns once the child is reaped. Cancelling twice is a no-op.
    pub fn cancel(&mut self, grace: Duration) -> Result<(), JobError> {
        if self.cancelled {
            return Ok(());
        }
        self.ensure_running()?;

        // A stopped group ignores nothing, but `SIGCONT` first makes sure a
        // paused FFmpeg can act on the `SIGTERM` it is about to receive.
        if self.paused {
            let _ = signal_group(self.pid, libc::SIGCONT);
            self.paused = false;
        }
        signal_group(self.pid, libc::SIGTERM)?;

        if self.poll_exit(grace)?.is_none() {
            signal_group(self.pid, libc::SIGKILL)?;
        }
        self.reap()?;
        self.cancelled = true;
        Ok(())
    }

    /// Waits for the child, joins the reader threads and turns a non-zero exit
    /// into `PROCESS_FAILED` with the status in the message.
    pub fn wait(&mut self) -> Result<ExitStatus, JobError> {
        let status = self.reap()?;
        self.join_threads()?;
        if status.success() {
            Ok(status)
        } else {
            Err(JobError::new(
                ErrorCode::ProcessFailed,
                format!("FFmpeg terminó con error: {status}"),
                true,
            ))
        }
    }

    /// Rejects control calls on a child that already exited.
    fn ensure_running(&mut self) -> Result<(), JobError> {
        if self.status.is_some() {
            return Err(not_running(self.pid));
        }
        match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => {
                    self.status = Some(status);
                    Err(not_running(self.pid))
                }
                Ok(None) => Ok(()),
                Err(error) => Err(internal(
                    self.pid,
                    &format!("no se pudo consultar: {error}"),
                )),
            },
            None => Err(not_running(self.pid)),
        }
    }

    /// Polls for the exit status for at most `timeout`.
    fn poll_exit(&mut self, timeout: Duration) -> Result<Option<ExitStatus>, JobError> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        self.status = Some(status);
                        return Ok(Some(status));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        return Err(internal(
                            self.pid,
                            &format!("no se pudo consultar el proceso: {error}"),
                        ));
                    }
                },
                None => return Ok(self.status),
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(CANCEL_POLL);
        }
    }

    /// Blocks until the child is reaped and caches the status.
    fn reap(&mut self) -> Result<ExitStatus, JobError> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let Some(child) = self.child.as_mut() else {
            return Err(internal(self.pid, "el proceso ya no está disponible"));
        };
        let status = child
            .wait()
            .map_err(|error| internal(self.pid, &format!("no se pudo esperar: {error}")))?;
        self.status = Some(status);
        Ok(status)
    }

    /// Joins both reader threads. A panicked thread is an internal error, never
    /// an `unwrap` on a poisoned lock.
    fn join_threads(&mut self) -> Result<(), JobError> {
        let mut panicked = false;
        for thread in self.threads.drain(..) {
            if thread.join().is_err() {
                panicked = true;
            }
        }
        if panicked {
            return Err(internal(
                self.pid,
                "un hilo de lectura de FFmpeg terminó de forma inesperada",
            ));
        }
        Ok(())
    }
}

impl Drop for ConversionChild {
    fn drop(&mut self) {
        if self.status.is_none() {
            // `SIGKILL` also reaches a stopped group, so a paused child dies too.
            let _ = signal_group(self.pid, libc::SIGKILL);
            if let Some(child) = self.child.as_mut() {
                let _ = child.wait();
            }
        }
        // Reader threads see EOF once the group is gone; ignoring their outcome
        // here keeps `drop` from panicking on a dead daemon.
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        self.cancelled = true;
    }
}

/// Signals the whole process group of `pid`.
fn signal_group(pid: u32, signal: libc::c_int) -> Result<(), JobError> {
    let Ok(group) = libc::pid_t::try_from(pid) else {
        return Err(internal(pid, "identificador de proceso fuera de rango"));
    };
    // SAFETY: `kill(2)` only reads its arguments; a negative pid addresses the
    // process group this runner created, never the caller's own group.
    let result = unsafe { libc::kill(-group, signal) };
    if result == 0 {
        return Ok(());
    }
    Err(internal(
        pid,
        &format!(
            "no se pudo enviar la señal {signal} al grupo: {}",
            std::io::Error::last_os_error()
        ),
    ))
}

fn not_running(pid: u32) -> JobError {
    JobError::new(
        ErrorCode::ProcessFailed,
        format!("El proceso {pid} ya no está en ejecución."),
        false,
    )
}

fn internal(pid: u32, detail: &str) -> JobError {
    JobError::new(
        ErrorCode::Internal,
        format!("Fallo interno controlando el proceso {pid}: {detail}"),
        false,
    )
}

/// Feeds stdout lines to the parser and fans samples out at 2 Hz.
fn pump_progress(
    stream: impl Read,
    total_ms: u64,
    mut on_progress: Box<dyn FnMut(Progress) + Send>,
) {
    let mut parser = ProgressParser::new(total_ms);
    let mut last_emit: Option<Instant> = None;

    for_each_line(stream, |line| {
        let is_final = line.trim() == "progress=end";
        let Some(progress) = parser.feed_line(line, utc_now()) else {
            return;
        };
        let due = last_emit.is_none_or(|last| last.elapsed() >= PROGRESS_INTERVAL);
        if is_final || due {
            last_emit = Some(Instant::now());
            on_progress(progress);
        }
    });
}

/// Forwards stderr line by line; nothing is accumulated beyond the current line.
fn pump_stderr(stream: impl Read, mut on_stderr: Box<dyn FnMut(&str) + Send>) {
    for_each_line(stream, |line| on_stderr(line));
}
