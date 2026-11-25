//! Toolchain location and capability report (task T9 / C-020).
//!
//! `docs/architecture.md` §10: `ffmpeg` and `ffprobe` come from `PATH`, are
//! checked at startup for version, encoders and filters, and every process runs
//! with argv — never a shell. The real-binary tests skip explicitly (never
//! silently) when this machine has no FFmpeg.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use cauce_core::command::{REQUIRED_ENCODERS, REQUIRED_FILTERS};
use cauce_core::{Capabilities, ErrorCode, JobError};
use cauce_ffmpeg::{
    Toolchain, encoders, ffmpeg_version, ffprobe_version, filters, inspect, locate, locate_in,
};

/// `fork` inherits every open write descriptor, so a script one test is still
/// writing can make another test's `execve` fail with `ETXTBSY` — the flake that
/// shows up as "Text file busy". Serialising this file removes the race; every
/// test here takes milliseconds.
static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialized() -> std::sync::MutexGuard<'static, ()> {
    SPAWN_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Temporary directory that removes itself, so the fakes leave nothing behind.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        // Two tests sharing a tag would delete each other's fakes mid-run; the
        // counter makes every directory unique even when that happens.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cauce-t9-{tag}-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the test temp dir must be creatable");
        Self { path }
    }

    fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, body).expect("the fake binary must be writable");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("the fake binary must be executable");
        path
    }

    fn as_str(&self) -> &str {
        self.path
            .to_str()
            .expect("the temp dir must be valid UTF-8")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const BANNER: &str = "ffmpeg version 8.0.1 Copyright (c) 2000-2025 the FFmpeg developers\n";
const PROBE_BANNER: &str = "ffprobe version 8.0.1 Copyright (c) 2007-2025 the FFmpeg developers\n";

/// Fake `ffmpeg` that answers `-version`, `-encoders` and `-filters` but has no
/// `libx265`, so the report must name the missing encoder.
fn fake_ffmpeg_without_x265() -> String {
    format!(
        r#"#!/bin/sh
case "$*" in
  *-version*) echo '{banner}' ;;
  *-encoders*) cat <<'TABLE'
Encoders:
 V..... = Video
 A..... = Audio
 ------
 V....D libx264              H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10
 V....D libsvtav1            SVT-AV1 encoder
 V....D libvpx-vp9           libvpx VP9
 A....D aac                  AAC (Advanced Audio Coding)
 A....D libmp3lame           libmp3lame MP3 (MPEG audio layer 3)
 A....D flac                 FLAC (Free Lossless Audio Codec)
 A....D alac                 ALAC (Apple Lossless Audio Codec)
 A....D pcm_s16le            PCM signed 16-bit little-endian
 A....D libopus              libopus Opus
 A....D ac3                 ATSC A/52A (AC-3)
TABLE
  ;;
  *-filters*) cat <<'TABLE'
Filters:
  T.. = Timeline support
  .S. = Slice threading
 TS zscale            V->V       Apply colorspace and bit depth conversion.
 .. tonemap           V->V       Convert to/from a tonemapping operator.
 .. scale             V->V       Scale the input video size.
 .. format            V->V       Convert the input to a pixel format.
 .. fps               V->V       Force constant framerate.
TABLE
  ;;
esac
"#,
        banner = BANNER.trim_end(),
    )
}

fn fake_ffprobe() -> String {
    format!("#!/bin/sh\necho '{}'\n", PROBE_BANNER.trim_end())
}

fn skip(test: &str) {
    eprintln!("skipping {test}: no ffmpeg/ffprobe in PATH on this machine");
}

fn error_of<T>(result: Result<T, JobError>) -> JobError {
    match result {
        Ok(_) => panic!("the call must fail"),
        Err(error) => error,
    }
}

#[test]
fn locates_the_system_toolchain() {
    let _serial = serialized();
    let toolchain = match locate() {
        Ok(toolchain) => toolchain,
        Err(_) => return skip("locates_the_system_toolchain"),
    };

    for binary in [&toolchain.ffmpeg, &toolchain.ffprobe] {
        assert!(
            binary.is_absolute(),
            "PATH lookup must yield an absolute path, got {}",
            binary.display()
        );
        assert!(binary.exists(), "{} must exist", binary.display());
        let mode = fs::metadata(binary)
            .expect("the binary must be stat-able")
            .permissions()
            .mode();
        assert!(
            mode & 0o111 != 0,
            "{} must be executable, mode {mode:o}",
            binary.display()
        );
    }
}

#[test]
fn reads_real_versions() {
    let _serial = serialized();
    let toolchain = match locate() {
        Ok(toolchain) => toolchain,
        Err(_) => return skip("reads_real_versions"),
    };

    for version in [
        ffmpeg_version(&toolchain).expect("ffmpeg must report its version"),
        ffprobe_version(&toolchain).expect("ffprobe must report its version"),
    ] {
        assert!(
            version.starts_with(|c: char| c.is_ascii_digit()),
            "a version token starts with a digit, got `{version}`"
        );
        assert!(
            version.contains('.'),
            "a version token contains a `.`, got `{version}`"
        );
        assert!(
            version.len() < 16,
            "the version must be a token, not the whole banner, got `{version}`"
        );
    }
}

#[test]
fn reports_every_mvp_encoder_present() {
    let _serial = serialized();
    let toolchain = match locate() {
        Ok(toolchain) => toolchain,
        Err(_) => return skip("reports_every_mvp_encoder_present"),
    };

    let available = encoders(&toolchain).expect("ffmpeg must list its encoders");
    for encoder in REQUIRED_ENCODERS {
        assert!(
            available.iter().any(|found| found == encoder),
            "the MVP needs `{encoder}`; the local FFmpeg exposes {} encoders and none of them is it",
            available.len()
        );
    }
}

#[test]
fn reports_every_required_filter_present() {
    let _serial = serialized();
    let toolchain = match locate() {
        Ok(toolchain) => toolchain,
        Err(_) => return skip("reports_every_required_filter_present"),
    };

    let available = filters(&toolchain).expect("ffmpeg must list its filters");
    for filter in REQUIRED_FILTERS {
        assert!(
            available.iter().any(|found| found == filter),
            "the MVP needs the `{filter}` filter; {} filters listed without it",
            available.len()
        );
    }
}

#[test]
fn inspects_the_real_toolchain() {
    let _serial = serialized();
    let toolchain = match locate() {
        Ok(toolchain) => toolchain,
        Err(_) => return skip("inspects_the_real_toolchain"),
    };

    let capabilities: Capabilities = inspect(&toolchain, &std::env::temp_dir())
        .expect("the local toolchain must be inspectable");

    assert!(capabilities.cpu_cores > 0, "cpu_cores must be measured");
    assert!(
        capabilities.disk_free_bytes > 0,
        "disk_free_bytes must be measured"
    );
    assert!(
        capabilities.ffmpeg_missing.is_empty(),
        "this machine is expected to satisfy the MVP, missing: {:?}",
        capabilities.ffmpeg_missing
    );
    assert!(
        capabilities.warnings.is_empty(),
        "a complete toolchain warns about nothing, got: {:?}",
        capabilities.warnings
    );
    assert!(
        capabilities.daemon_version.is_empty(),
        "the daemon fills `daemon_version` in T13, not the locator"
    );
    assert!(!capabilities.encoders.is_empty() && !capabilities.filters.is_empty());
}

#[test]
fn fails_when_ffmpeg_is_missing() {
    let _serial = serialized();
    let error = error_of(locate_in("/nonexistent-cauce"));

    assert_eq!(error.code, ErrorCode::FfmpegMissing);
    assert!(!error.retryable, "a missing package is not retryable");
    assert!(
        error.message.contains("sudo apt install ffmpeg"),
        "the message must name the exact fix, got: {}",
        error.message
    );
}

#[test]
fn fails_when_only_ffprobe_is_present() {
    let _serial = serialized();
    let dir = TempDir::new("only-ffprobe");
    let ffprobe = dir.script("ffprobe", &fake_ffprobe());
    assert!(ffprobe.exists());

    let error = error_of(locate_in(dir.as_str()));

    assert_eq!(error.code, ErrorCode::FfmpegMissing);
    assert!(!error.retryable);
    assert!(
        error.message.contains("ffmpeg"),
        "the message must name the absent binary, got: {}",
        error.message
    );
}

#[test]
fn reports_missing_encoders_from_a_fake_binary() {
    let _serial = serialized();
    let dir = TempDir::new("fake-encoders");
    dir.script("ffmpeg", &fake_ffmpeg_without_x265());
    dir.script("ffprobe", &fake_ffprobe());

    let toolchain = locate_in(dir.as_str()).expect("both fake binaries are present and executable");
    let capabilities = inspect(&toolchain, Path::new(dir.as_str()))
        .expect("an incomplete toolchain is still a report");

    assert_eq!(
        capabilities.ffmpeg_missing,
        vec!["libx265".to_string()],
        "only `libx265` is absent from the fake encoder table"
    );
    assert_eq!(
        capabilities.warnings.len(),
        1,
        "one missing encoder means one warning, got: {:?}",
        capabilities.warnings
    );
    assert!(
        capabilities.warnings[0].contains("libx265"),
        "the warning must name the missing encoder, got: {}",
        capabilities.warnings[0]
    );
}

#[test]
fn rejects_a_binary_that_prints_garbage() {
    let _serial = serialized();
    let dir = TempDir::new("garbage");
    let ffmpeg = dir.script("ffmpeg", "#!/bin/sh\necho 'not a banner'\n");
    let ffprobe = dir.script("ffprobe", &fake_ffprobe());

    let toolchain = Toolchain { ffmpeg, ffprobe };
    let error = error_of(ffmpeg_version(&toolchain));

    assert_eq!(
        error.code,
        ErrorCode::FfmpegMissing,
        "an unusable binary is reported as missing, got: {}",
        error.message
    );
    assert!(!error.retryable);
}

#[test]
fn zscale_is_optional_and_not_reported_as_missing() {
    let _serial = serialized();
    let dir = TempDir::new("no-zscale");
    let fake_ffmpeg = format!(
        r#"#!/bin/sh
case "$*" in
  *-version*) echo '{banner}' ;;
  *-encoders*) cat <<'TABLE'
Encoders:
 V..... = Video
 A..... = Audio
 ------
 V....D libx264              H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10
 V....D libx265              H.265 / HEVC
 V....D libsvtav1            SVT-AV1 encoder
 V....D libvpx-vp9           libvpx VP9
 A....D aac                  AAC (Advanced Audio Coding)
 A....D libmp3lame           libmp3lame MP3 (MPEG audio layer 3)
 A....D flac                 FLAC (Free Lossless Audio Codec)
 A....D alac                 ALAC (Apple Lossless Audio Codec)
 A....D pcm_s16le            PCM signed 16-bit little-endian
 A....D libopus              libopus Opus
 A....D ac3                 ATSC A/52A (AC-3)
TABLE
  ;;
  *-filters*) cat <<'TABLE'
Filters:
  T.. = Timeline support
  .S. = Slice threading
 .. tonemap           V->V       Convert to/from a tonemapping operator.
 .. scale             V->V       Scale the input video size.
 .. format            V->V       Convert the input to a pixel format.
 .. fps               V->V       Force constant framerate.
TABLE
  ;;
esac
"#,
        banner = BANNER.trim_end(),
    );
    dir.script("ffmpeg", &fake_ffmpeg);
    dir.script("ffprobe", &fake_ffprobe());

    let toolchain = locate_in(dir.as_str()).expect("both fake binaries are present and executable");
    let capabilities = inspect(&toolchain, Path::new(dir.as_str())).expect("inspect must succeed");

    assert!(
        !capabilities.ffmpeg_missing.contains(&"zscale".to_string()),
        "zscale is optional and must not be reported as missing: {:?}",
        capabilities.ffmpeg_missing
    );
}
