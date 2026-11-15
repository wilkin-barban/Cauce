//! Process control for conversions (task T10 / C-021).
//!
//! `docs/architecture.md` §7 requires `SIGSTOP`/`SIGCONT` on pause/resume and
//! `SIGTERM`→`SIGKILL` on cancel; §18 requires no orphaned processes. Every
//! signal goes to the child's own process group, so a FFmpeg grandchild dies
//! with its parent. The fakes are hermetic shell scripts in a temp dir.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cauce_core::{ErrorCode, JobError, Progress};
use cauce_ffmpeg::runner::{ConversionChild, spawn};

/// Temporary directory that removes itself, so the fakes leave nothing behind.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};

        // Tests run in parallel threads inside one process: a per-process path is
        // shared state, and one test executing a script another test is still
        // writing yields ETXTBSY ("Text file busy"). Every test gets its own dir.
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "cauce-t10-{tag}-{}-{unique}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the test temp dir must be creatable");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn script(&self, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        // Write aside and rename atomically, so exec never sees a partial file.
        let path = self.path.join("fake-ffmpeg");
        let staged = self.path.join("fake-ffmpeg.staged");
        fs::write(&staged, body).expect("the fake binary must be writable");
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
            .expect("the fake binary must be executable");
        fs::rename(&staged, &path).expect("the fake binary must be published atomically");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A fake FFmpeg that records its pid and, optionally, a `sleep` grandchild's.
struct Fake {
    script: PathBuf,
    pid: PathBuf,
    grandchild: Option<PathBuf>,
}

fn fake(dir: &TempDir, with_grandchild: bool) -> Fake {
    let pid = dir.path().join("pid");
    let grandchild = dir.path().join("grandchild.pid");
    let body = if with_grandchild {
        format!(
            "#!/bin/bash\nsleep 30 &\necho $! > {}\necho $$ > {}\nwait\n",
            grandchild.display(),
            pid.display()
        )
    } else {
        format!("#!/bin/bash\necho $$ > {}\nexec sleep 30\n", pid.display())
    };
    Fake {
        script: dir.script(&body),
        pid,
        grandchild: with_grandchild.then_some(grandchild),
    }
}

fn start(script: &Path) -> ConversionChild {
    spawn_sh(script, &[], 0, Box::new(|_| {}), Box::new(|_| {}))
        .unwrap_or_else(|error| panic!("spawning {} failed: {error:?}", script.display()))
}

/// Runs a fake through `/bin/sh` instead of exec'ing the script directly.
///
/// Linux answers `ETXTBSY` ("Text file busy") when a file that was just written
/// is executed while another thread still holds a descriptor to it, which makes
/// a script-based test flaky under parallel load. `/bin/sh` is a stable binary,
/// and everything under test (signals, process groups, throttling) behaves the
/// same. The `#!/bin/bash` header of the fakes is simply ignored.
fn spawn_sh(
    script: &Path,
    args: &[String],
    total_ms: u64,
    on_progress: Box<dyn FnMut(Progress) + Send>,
    on_stderr: Box<dyn FnMut(&str) + Send>,
) -> Result<ConversionChild, JobError> {
    let mut argv = vec![script.display().to_string()];
    argv.extend(args.iter().cloned());
    spawn(
        Path::new("/bin/sh"),
        &argv,
        total_ms,
        on_progress,
        on_stderr,
    )
}

/// Polls `probe` until it returns something, for at most five seconds.
fn wait_for<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Number of lines the counter file holds right now; `0` while it is missing.
fn counter_lines(file: &Path) -> usize {
    fs::read_to_string(file).map_or(0, |body| body.lines().count())
}

fn wait_for_pid(file: &Path) -> u32 {
    wait_for("the fake binary to write its pid", || {
        fs::read_to_string(file).ok()?.trim().parse().ok()
    })
}

fn exists(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

fn wait_until_gone(pid: u32) {
    wait_for(&format!("/proc/{pid} to disappear"), || {
        (!exists(pid)).then_some(())
    });
}

/// Single-letter process state from `/proc/<pid>/stat`.
fn state_of(pid: u32) -> Option<char> {
    let raw = fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("stat")).ok()?;
    // The comm field may contain spaces and parentheses, so read past the last `)`.
    raw[raw.rfind(')')? + 1..]
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

fn wait_for_state(pid: u32, wanted: impl Fn(Option<char>) -> bool, what: &str) {
    wait_for(what, || wanted(state_of(pid)).then_some(()));
}

#[test]
fn pause_suspends_the_process_group() {
    let dir = TempDir::new("pause");
    let fake = fake(&dir, false);

    let mut child = start(&fake.script);
    let pid = wait_for_pid(&fake.pid);
    assert_eq!(pid, child.pid(), "`exec` must keep the pid the group uses");
    wait_for_state(pid, |state| state == Some('S'), "state `S`");

    child.pause().expect("pause must stop the process group");
    wait_for_state(pid, |state| state == Some('T'), "state `T`");

    child
        .resume()
        .expect("resume must continue the process group");
    wait_for_state(pid, |state| state != Some('T'), "leaving `T`");

    child.cancel(Duration::from_secs(2)).expect("cancel");
    wait_until_gone(pid);
}

#[test]
fn resume_continues_the_process_group() {
    let dir = TempDir::new("resume");
    let fake = fake(&dir, false);

    let mut child = start(&fake.script);
    let pid = wait_for_pid(&fake.pid);
    wait_for_state(pid, |state| state == Some('S'), "state `S`");

    child.pause().expect("pause");
    wait_for_state(pid, |state| state == Some('T'), "state `T`");
    // Pausing twice is idempotent: the second call is not an error.
    child.pause().expect("a second pause must be a no-op");

    child.resume().expect("resume");
    wait_for_state(pid, |state| state != Some('T'), "leaving `T`");
    // Resuming a child that is not paused is a no-op, not an error.
    child.resume().expect("a second resume must be a no-op");

    child.cancel(Duration::from_secs(2)).expect("cancel");
    wait_until_gone(pid);
}

#[test]
fn pause_freezes_and_resume_continues_a_live_child() {
    let dir = TempDir::new("live-pause");
    let counter = dir.path().join("counter");
    // One line every 20 ms for ~4 s: the fake outlives every assertion below, so
    // `cancel` — not exhaustion — is what ends it.
    let script = dir.script(&format!(
        "#!/bin/bash\nfor i in $(seq 1 200); do\n  echo $i >> {}\n  sleep 0.02\ndone\n",
        counter.display()
    ));

    let mut child = start(&script);
    let pid = child.pid();
    wait_for("the fake to append five counter lines", || {
        (counter_lines(&counter) >= 5).then_some(())
    });

    child.pause().expect("pause must stop the process group");
    let frozen = counter_lines(&counter);
    thread::sleep(Duration::from_millis(250));
    let after_pause = counter_lines(&counter);
    // One write may still be in flight when `SIGSTOP` lands: the assertion is
    // that the counter does not keep growing, not that it froze at the same byte.
    assert!(
        after_pause <= frozen + 1,
        "the counter kept growing while paused: {frozen} -> {after_pause} lines"
    );

    child
        .resume()
        .expect("resume must continue the process group");
    let resumed = wait_for("the counter to grow again after resume", || {
        (counter_lines(&counter) > after_pause).then(|| counter_lines(&counter))
    });
    assert!(
        resumed > after_pause,
        "resume must restart the writes: stuck at {after_pause} lines"
    );

    child
        .cancel(Duration::from_secs(2))
        .expect("cancel must terminate the group");
    wait_until_gone(pid);
    assert!(
        counter_lines(&counter) >= resumed,
        "a cancelled child appends nothing more"
    );
}

#[test]
fn reports_exit_even_when_stderr_closes_early() {
    let dir = TempDir::new("stderr-closed");
    // stderr closes while the process is still alive, so the reader thread ends
    // first and no stream-end marker can announce the exit.
    let script = dir.script("#!/bin/sh\nexec 2>&-\nsleep 0.2\nexit 0\n");

    let mut child = start(&script);
    let status = wait_for("`try_exit` to report the closed-stderr exit", || {
        child.try_exit().expect("`try_exit` must not fail")
    });
    assert!(status.success(), "the fake exits 0, got {status}");

    let again = child.try_exit().expect("a second `try_exit` must not fail");
    assert_eq!(
        again,
        Some(status),
        "a second `try_exit` repeats the cached status instead of erroring"
    );
    assert_eq!(status.code(), Some(0), "the fake exits 0, got {status}");
}

#[test]
fn try_exit_is_non_blocking_for_a_running_child() {
    let dir = TempDir::new("try-exit");
    let fake = fake(&dir, false);

    let mut child = start(&fake.script);
    let pid = wait_for_pid(&fake.pid);
    wait_for_state(pid, |state| state == Some('S'), "state `S`");

    let started = Instant::now();
    for _ in 0..50 {
        assert_eq!(
            child.try_exit().expect("`try_exit` must not fail"),
            None,
            "a child that is still running has no exit status"
        );
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "50 calls took {:?}: `try_exit` must never wait for a live child",
        started.elapsed()
    );

    child.cancel(Duration::from_secs(2)).expect("cancel");
    wait_until_gone(pid);
}

#[test]
fn cancel_terminates_child_and_grandchild() {
    let dir = TempDir::new("cancel");
    let fake = fake(&dir, true);
    let grandchild_file = fake.grandchild.clone().expect("a grandchild pid file");

    let mut child = start(&fake.script);
    let pid = wait_for_pid(&fake.pid);
    let grandchild = wait_for_pid(&grandchild_file);
    assert!(
        exists(pid) && exists(grandchild),
        "both must be alive first"
    );

    child
        .cancel(Duration::from_secs(2))
        .expect("cancel must terminate the group");
    wait_until_gone(pid);
    wait_until_gone(grandchild);

    // Cancelling again is a no-op, not an error.
    child.cancel(Duration::from_secs(2)).expect("second cancel");
}

#[test]
fn cancel_escalates_to_sigkill_when_sigterm_is_ignored() {
    let dir = TempDir::new("sigkill");
    let pid_file = dir.path().join("pid");
    let script = dir.script(&format!(
        "#!/bin/bash\ntrap '' TERM\necho $$ > {}\nwhile true; do sleep 1; done\n",
        pid_file.display()
    ));

    let mut child = start(&script);
    let pid = wait_for_pid(&pid_file);
    wait_for_state(pid, |state| state.is_some(), "a live process");

    let started = Instant::now();
    child
        .cancel(Duration::from_millis(200))
        .expect("cancel must escalate to SIGKILL");
    assert!(
        started.elapsed() >= Duration::from_millis(200),
        "cancel must honour the grace period before SIGKILL"
    );
    wait_until_gone(pid);
}

#[test]
fn dropping_the_child_leaves_no_orphan() {
    let dir = TempDir::new("drop");
    let fake = fake(&dir, true);
    let grandchild_file = fake.grandchild.clone().expect("a grandchild pid file");

    let child = start(&fake.script);
    let pid = wait_for_pid(&fake.pid);
    let grandchild = wait_for_pid(&grandchild_file);

    drop(child);

    wait_until_gone(pid);
    wait_until_gone(grandchild);
}

#[test]
fn throttles_progress_to_two_per_second() {
    let dir = TempDir::new("throttle");
    let script = dir.script(
        "#!/bin/bash\n\
         for i in $(seq 1 200); do\n\
           echo \"frame=$i\"\n\
           echo \"out_time_us=$((i * 1000))\"\n\
           echo \"progress=continue\"\n\
         done\n\
         echo \"out_time_us=200000\"\n\
         echo \"progress=end\"\n\
         sleep 1\n",
    );

    let samples = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&samples);
    let on_progress = Box::new(move |progress: Progress| {
        recorded.lock().expect("the sample list").push(progress);
    });

    let mut child = spawn_sh(&script, &[], 400_000, on_progress, Box::new(|_| {}))
        .expect("the fake binary must spawn");
    assert!(child.wait().expect("the fake binary exits 0").success());

    let samples = samples.lock().expect("the sample list");
    assert!(
        (1..=3).contains(&samples.len()),
        "2 Hz over ~1 s plus the final block is at most 3 samples, got {}",
        samples.len()
    );
    let last = samples.last().expect("the final block must be delivered");
    assert_eq!(
        last.processed_ms, 200,
        "the last sample must be the `progress=end` block, delivered even inside the throttle window"
    );
    assert_eq!(last.total_ms, 400_000);
}

#[test]
fn forwards_stderr_lines_truncated_to_four_kibibytes() {
    let dir = TempDir::new("stderr");
    let script = dir.script(
        "#!/bin/bash\n\
         head -c 5000 /dev/zero | tr '\\0' 'x' >&2\n\
         echo >&2\n\
         echo 'short line' >&2\n",
    );

    let lines = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&lines);
    let on_stderr = Box::new(move |line: &str| {
        collected
            .lock()
            .expect("the line list")
            .push(line.to_owned());
    });

    let mut child =
        spawn_sh(&script, &[], 0, Box::new(|_| {}), on_stderr).expect("the fake binary must spawn");
    child.wait().expect("the fake binary exits 0");

    let lines = lines.lock().expect("the line list");
    assert_eq!(
        lines.len(),
        2,
        "one callback per stderr line, got {lines:?}"
    );
    assert_eq!(lines[0].len(), 4096, "each line is truncated to 4096 bytes");
    assert_eq!(lines[1], "short line");
}

#[test]
fn reports_a_non_zero_exit() {
    let dir = TempDir::new("exit");
    let script = dir.script("#!/bin/bash\necho \"progress=continue\"\nexit 3\n");

    let mut child = start(&script);
    let error = child.wait().expect_err("exit status 3 must fail the job");
    assert_eq!(error.code, ErrorCode::ProcessFailed);
    assert!(error.retryable, "a crashed FFmpeg is retryable");
    assert!(
        error.message.contains('3'),
        "the message must report the exit status, got `{}`",
        error.message
    );
}

#[test]
fn missing_binary_is_ffmpeg_missing() {
    let missing = std::env::temp_dir().join("cauce-t10-no-such-ffmpeg");
    let error = spawn(&missing, &[], 0, Box::new(|_| {}), Box::new(|_| {}))
        .expect_err("a nonexistent binary cannot be spawned");
    assert_eq!(error.code, ErrorCode::FfmpegMissing);
    assert!(!error.retryable);
}

#[test]
fn runs_a_real_ffmpeg_conversion() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/cauce-fixtures/run/sample-h264-aac.mp4");
    let available = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available || !fixture.exists() {
        eprintln!("skipping `runs_a_real_ffmpeg_conversion`: no ffmpeg or no {fixture:?}");
        return;
    }

    let input = fixture.to_string_lossy();
    let args: Vec<String> = [
        "-progress",
        "pipe:1",
        "-nostats",
        "-i",
        input.as_ref(),
        "-c",
        "copy",
        "-f",
        "null",
        "-",
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect();

    let samples = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&samples);
    let on_progress = Box::new(move |progress: Progress| {
        recorded.lock().expect("the sample list").push(progress);
    });
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&stderr);
    let on_stderr = Box::new(move |line: &str| {
        collected.lock().expect("the lines").push(line.to_owned());
    });

    // Twice the fixture duration, so real samples stay strictly inside 0..100.
    let mut child = spawn(Path::new("ffmpeg"), &args, 4_000, on_progress, on_stderr)
        .expect("the system ffmpeg must spawn");
    let status = child
        .wait()
        .unwrap_or_else(|error| panic!("ffmpeg failed: {error:?} stderr: {:?}", stderr.lock()));
    assert!(status.success());

    let samples = samples.lock().expect("the sample list");
    let percents: Vec<Option<f64>> = samples.iter().map(|sample| sample.percent).collect();
    assert!(
        !samples.is_empty(),
        "ffmpeg must emit at least one progress block"
    );
    assert!(samples.iter().all(|sample| sample.total_ms == 4_000));
    assert!(
        percents
            .iter()
            .flatten()
            .any(|percent| *percent > 0.0 && *percent < 100.0),
        "at least one sample must have a percent between 0 and 100, got {percents:?}"
    );
    assert!(percents.iter().flatten().all(|p| (0.0..=100.0).contains(p)));
}
