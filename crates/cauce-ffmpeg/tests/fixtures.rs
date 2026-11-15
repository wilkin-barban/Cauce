//! Reproducible multimedia fixtures (`docs/backlog.md` C-017).
//!
//! Fixtures are generated with the system FFmpeg from synthetic sources, so the
//! repository never stores media binaries and every checkout can rebuild them.
//! When FFmpeg is unavailable the test reports that it skipped instead of
//! pretending the fixtures were verified.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn generator() -> PathBuf {
    repo_root().join("tests/fixtures/generate.sh")
}

fn manifest() -> Value {
    let path = repo_root().join("tests/fixtures/manifest.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&raw).expect("fixture manifest must be valid JSON")
}

fn manifest_fixtures() -> Vec<(String, String, String)> {
    manifest()["fixtures"]
        .as_array()
        .expect("manifest must declare a fixtures array")
        .iter()
        .map(|entry| {
            (
                entry["name"].as_str().expect("fixture name").to_owned(),
                entry["kind"].as_str().expect("fixture kind").to_owned(),
                entry["container"]
                    .as_str()
                    .expect("fixture container")
                    .to_owned(),
            )
        })
        .collect()
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn generate_into(output_dir: &Path) {
    let status = Command::new("bash")
        .arg(generator())
        .arg(output_dir)
        .status()
        .unwrap_or_else(|error| panic!("cannot run {}: {error}", generator().display()));

    assert!(
        status.success(),
        "fixture generator failed with {status}; run {} manually to see why",
        generator().display()
    );
}

fn probe_format(path: &Path) -> Value {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_entries",
            "format=format_name,duration",
            "-i",
        ])
        .arg(path)
        .output()
        .expect("ffprobe must run");

    assert!(
        output.status.success(),
        "ffprobe could not open {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice::<Value>(&output.stdout).expect("ffprobe must emit JSON")
}

#[test]
fn generator_script_is_versioned() {
    let path = generator();
    assert!(
        path.is_file(),
        "the fixture generator must live at {}. Fixtures are generated, never committed",
        path.display()
    );
}

#[test]
fn generates_the_declared_fixtures_within_bounds() {
    if !ffmpeg_available() {
        eprintln!("skipping fixture generation: ffmpeg is not available on this machine");
        return;
    }

    let output_dir = repo_root().join("target/cauce-fixtures/run");
    let _ = std::fs::remove_dir_all(&output_dir);
    generate_into(&output_dir);

    let max_size = manifest()["max_size_bytes"]
        .as_u64()
        .expect("max_size_bytes");
    let max_duration_ms = manifest()["max_duration_ms"]
        .as_u64()
        .expect("max_duration_ms");

    for (name, kind, container) in manifest_fixtures() {
        let path = output_dir.join(&name);
        assert!(path.is_file(), "{name} was not generated");

        let size = std::fs::metadata(&path).expect("fixture metadata").len();
        assert!(
            size > 0 && size <= max_size,
            "{name} must be non-empty and at most {max_size} bytes, got {size}"
        );

        let format = probe_format(&path);
        let format_name = format["format"]["format_name"]
            .as_str()
            .expect("format_name");
        assert!(
            format_name.contains(&container),
            "{name} ({kind}) must be a {container} file, ffprobe reported {format_name}"
        );

        let duration_ms = format["format"]["duration"]
            .as_str()
            .expect("duration")
            .parse::<f64>()
            .expect("numeric duration")
            * 1000.0;
        assert!(
            duration_ms > 0.0 && duration_ms <= max_duration_ms as f64,
            "{name} must last at most {max_duration_ms} ms, got {duration_ms}"
        );
    }
}

#[test]
fn generation_is_reproducible() {
    if !ffmpeg_available() {
        eprintln!("skipping reproducibility check: ffmpeg is not available on this machine");
        return;
    }

    let first = repo_root().join("target/cauce-fixtures/repro-a");
    let second = repo_root().join("target/cauce-fixtures/repro-b");
    for dir in [&first, &second] {
        let _ = std::fs::remove_dir_all(dir);
        generate_into(dir);
    }

    for (name, _, _) in manifest_fixtures() {
        let a = std::fs::read(first.join(&name)).expect("first run output");
        let b = std::fs::read(second.join(&name)).expect("second run output");
        assert_eq!(
            a, b,
            "{name} must be byte-identical across runs: the fixtures are generated, not sampled"
        );
    }
}
