use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;

use cauce_cli::{
    EXIT_RUNTIME_ERROR, EXIT_SUCCESS, EXIT_USAGE_ERROR, build_jobs_add_request, parse_convert_args,
    resolve_audio_language,
};
use cauce_core::{
    AudioMixMode, AudioStream, MediaSnapshot, ResolutionTarget, SubtitleMode, Timestamp,
    ToneMappingOption,
};

fn sample_snapshot() -> MediaSnapshot {
    MediaSnapshot {
        path: "/media/sample.mp4".to_string(),
        container: "mp4".to_string(),
        duration_ms: 60_000,
        size_bytes: 10_000_000,
        bitrate_bps: 1_333_333,
        video_streams: vec![],
        audio_streams: vec![
            AudioStream {
                index: 1,
                codec: "aac".to_string(),
                channels: 6,
                sample_rate: 48000,
                bitrate_bps: Some(384_000),
                language: Some("spa".to_string()),
                is_default: true,
            },
            AudioStream {
                index: 2,
                codec: "aac".to_string(),
                channels: 2,
                sample_rate: 48000,
                bitrate_bps: Some(192_000),
                language: Some("eng".to_string()),
                is_default: false,
            },
            AudioStream {
                index: 3,
                codec: "aac".to_string(),
                channels: 2,
                sample_rate: 48000,
                bitrate_bps: Some(192_000),
                language: Some("fra".to_string()),
                is_default: false,
            },
        ],
        subtitle_streams: vec![],
        probed_at: Timestamp::parse("2026-11-24T12:00:00Z").unwrap(),
    }
}

#[test]
fn parses_every_flag_into_the_request() {
    let args = [
        "cauce-cli",
        "convert",
        "video.mkv",
        "--preset",
        "system-smarttv-compat",
        "--output-dir",
        "/tmp/salida",
        "--audio-track",
        "1",
        "--audio-track",
        "2",
        "--mix",
        "dual",
        "--subtitles",
        "burn",
        "--resolution",
        "1080p",
        "--crf",
        "22",
        "--tone-mapping",
        "mobius",
        "--json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();

    let parsed = parse_convert_args(&args).expect("must parse all flags");
    assert_eq!(parsed.inputs, vec![PathBuf::from("video.mkv")]);
    assert_eq!(parsed.preset, "system-smarttv-compat");
    assert_eq!(parsed.output_dir, PathBuf::from("/tmp/salida"));
    assert_eq!(parsed.audio_tracks, vec![1, 2]);
    assert_eq!(parsed.mix, Some(AudioMixMode::DualTrackUniversal));
    assert_eq!(parsed.subtitles, Some((None, SubtitleMode::BurnIn)));
    assert_eq!(parsed.resolution, Some(ResolutionTarget::MaxHeight(1080)));
    assert_eq!(parsed.crf, Some(22));
    assert_eq!(parsed.tone_mapping, Some(ToneMappingOption::Mobius));
    assert!(parsed.json);

    let request = build_jobs_add_request(
        &parsed.inputs,
        &parsed.output_dir,
        &parsed.preset,
        parsed.build_track_selection().unwrap().as_ref(),
        parsed.build_custom_options().as_ref(),
    );

    assert_eq!(request["method"], "jobs.add");
    let params = &request["params"];
    assert_eq!(params["inputs"], serde_json::json!(["video.mkv"]));
    assert_eq!(params["output_dir"], "/tmp/salida");
    assert_eq!(params["preset_id"], "system-smarttv-compat");

    let track_sel = params
        .get("track_selection")
        .expect("track_selection must be present");
    assert_eq!(track_sel["audio_track_indices"], serde_json::json!([1, 2]));
    assert_eq!(track_sel["audio_mix_mode"], "dual_track_universal");
    assert_eq!(track_sel["subtitle_mode"], "burn_in");
    assert!(track_sel["subtitle_track_index"].is_null());

    let custom_opt = params
        .get("custom_options")
        .expect("custom_options must be present");
    assert_eq!(
        custom_opt["resolution"],
        serde_json::json!({"max_height": 1080})
    );
    assert_eq!(custom_opt["crf"], 22);
    assert_eq!(custom_opt["tone_mapping"], "mobius");
}

#[test]
fn omits_the_selection_when_no_flag_asks_for_it() {
    let args = [
        "cauce-cli",
        "convert",
        "video.mkv",
        "--output-dir",
        "/tmp/salida",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();

    let parsed = parse_convert_args(&args).expect("must parse minimal flags");
    assert_eq!(parsed.preset, "system-smarttv-compat");
    assert!(parsed.build_track_selection().unwrap().is_none());
    assert!(parsed.build_custom_options().is_none());

    let request = build_jobs_add_request(
        &parsed.inputs,
        &parsed.output_dir,
        &parsed.preset,
        None,
        None,
    );

    let params = &request["params"];
    assert!(
        params.get("track_selection").is_none(),
        "track_selection must be omitted"
    );
    assert!(
        params.get("custom_options").is_none(),
        "custom_options must be omitted"
    );
    assert_eq!(params["inputs"], serde_json::json!(["video.mkv"]));
    assert_eq!(params["output_dir"], "/tmp/salida");
    assert_eq!(params["preset_id"], "system-smarttv-compat");
}

#[test]
fn resolves_an_audio_language_against_the_snapshot() {
    let snapshot = sample_snapshot();

    // Direct and alias matches for Spanish
    assert_eq!(resolve_audio_language(&snapshot, "es").unwrap(), 1);
    assert_eq!(resolve_audio_language(&snapshot, "spa").unwrap(), 1);
    assert_eq!(resolve_audio_language(&snapshot, "es-419").unwrap(), 1);
    assert_eq!(resolve_audio_language(&snapshot, "ES").unwrap(), 1);
    assert_eq!(resolve_audio_language(&snapshot, "SPA").unwrap(), 1);

    // English
    assert_eq!(resolve_audio_language(&snapshot, "en").unwrap(), 2);
    assert_eq!(resolve_audio_language(&snapshot, "eng").unwrap(), 2);
    assert_eq!(resolve_audio_language(&snapshot, "en-US").unwrap(), 2);

    // French
    assert_eq!(resolve_audio_language(&snapshot, "fr").unwrap(), 3);
    assert_eq!(resolve_audio_language(&snapshot, "fra").unwrap(), 3);
}

#[test]
fn rejects_an_unknown_language() {
    let snapshot = sample_snapshot();
    let result = resolve_audio_language(&snapshot, "ja");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("ja"),
        "error must mention the requested language: {err}"
    );
    assert!(
        err.contains("spa"),
        "error must mention available languages: {err}"
    );

    // Also test through CLI execution against mock daemon returning snapshot
    let temp_dir = std::env::temp_dir().join(format!(
        "cauce-cli-test-unknown-lang-{}",
        std::process::id()
    ));
    let socket_dir = temp_dir.join("cauce");
    let _ = std::fs::create_dir_all(&socket_dir);
    let socket_path = socket_dir.join("daemon.sock");

    let input_file = temp_dir.join("video.mp4");
    std::fs::write(&input_file, b"content").unwrap();
    let out_dir = temp_dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let snapshot_val = serde_json::to_value(&snapshot).unwrap();
    let server = run_mock_daemon(
        socket_path,
        vec![
            (
                "initialize",
                serde_json::json!({"server_version": "0.1.1", "protocol_version": 1}),
            ),
            (
                "presets.list",
                serde_json::json!([{"id": "system-smarttv-compat"}]),
            ),
            ("media.inspect", serde_json::json!([snapshot_val])),
        ],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_cauce-cli"))
        .args([
            "convert",
            input_file.to_str().unwrap(),
            "--output-dir",
            out_dir.to_str().unwrap(),
            "--audio-lang",
            "ja",
        ])
        .env("XDG_RUNTIME_DIR", &temp_dir)
        .output()
        .expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(EXIT_USAGE_ERROR),
        "exit code must be 2 for unknown language"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ja"),
        "stderr should mention requested language: {stderr}"
    );

    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn rejects_an_out_of_range_crf() {
    // 52 is out of range 0..=51
    let args = [
        "cauce-cli",
        "convert",
        "video.mkv",
        "--output-dir",
        "/tmp/salida",
        "--crf",
        "52",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();

    let err = parse_convert_args(&args).unwrap_err();
    assert_eq!(err.exit_code(), EXIT_USAGE_ERROR);
    let msg = err.to_string();
    assert!(
        msg.contains("52") && msg.contains("51"),
        "message should explain CRF range: {msg}"
    );

    // Also test via CLI binary execution
    let output = Command::new(env!("CARGO_BIN_EXE_cauce-cli"))
        .args([
            "convert",
            "video.mkv",
            "--output-dir",
            "/tmp",
            "--crf",
            "52",
        ])
        .output()
        .expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(EXIT_USAGE_ERROR),
        "exit code must be 2 for usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("CRF") || stderr.contains("crf"),
        "stderr should mention CRF error: {stderr}"
    );
}

#[test]
fn a_missing_socket_fails_with_an_actionable_message() {
    let temp_dir = std::env::temp_dir().join(format!(
        "cauce-cli-test-missing-sock-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&temp_dir);

    // Provide an existing file and existing output dir so argument parsing passes
    let input_file = temp_dir.join("input.mp4");
    std::fs::write(&input_file, b"fake video content").unwrap();
    let out_dir = temp_dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cauce-cli"))
        .args([
            "convert",
            input_file.to_str().unwrap(),
            "--output-dir",
            out_dir.to_str().unwrap(),
        ])
        .env("XDG_RUNTIME_DIR", &temp_dir)
        .output()
        .expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(EXIT_RUNTIME_ERROR),
        "exit code must be 1 for runtime error (missing socket)"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("socket") || stderr.contains("daemon"),
        "stderr should mention daemon or socket: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

fn run_mock_daemon(
    socket_path: PathBuf,
    responses: Vec<(&'static str, serde_json::Value)>,
) -> std::thread::JoinHandle<()> {
    // Bind before returning: when the listener was created inside the thread the
    // client could connect first, the connection error turned a usage error into
    // a runtime failure, and the exit-code assertion failed at random.
    let listener = UnixListener::bind(&socket_path).expect("bind mock socket");
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        for (expected_method, result_value) in responses {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let req: serde_json::Value =
                serde_json::from_str(&line).expect("valid json-rpc request");
            let req_id = &req["id"];
            assert_eq!(req["method"], expected_method);
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "result": result_value,
            });
            let _ = stream.write_all(format!("{resp}\n").as_bytes());
            let _ = stream.flush();
        }
    })
}

#[test]
fn rejects_an_unknown_preset() {
    let temp_dir = std::env::temp_dir().join(format!(
        "cauce-cli-test-unknown-preset-{}",
        std::process::id()
    ));
    let socket_dir = temp_dir.join("cauce");
    let _ = std::fs::create_dir_all(&socket_dir);
    let socket_path = socket_dir.join("daemon.sock");

    let input_file = temp_dir.join("video.mp4");
    std::fs::write(&input_file, b"content").unwrap();
    let out_dir = temp_dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let server = run_mock_daemon(
        socket_path,
        vec![
            (
                "initialize",
                serde_json::json!({"server_version": "0.1.1", "protocol_version": 1}),
            ),
            (
                "presets.list",
                serde_json::json!([{"id": "system-smarttv-compat"}]),
            ),
        ],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_cauce-cli"))
        .args([
            "convert",
            input_file.to_str().unwrap(),
            "--output-dir",
            out_dir.to_str().unwrap(),
            "--preset",
            "non-existent-preset",
        ])
        .env("XDG_RUNTIME_DIR", &temp_dir)
        .output()
        .expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(EXIT_USAGE_ERROR),
        "exit code must be 2 for unknown preset"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("non-existent-preset"),
        "stderr should mention the missing preset: {stderr}"
    );

    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn runs_convert_and_prints_json() {
    let temp_dir = std::env::temp_dir().join(format!(
        "cauce-cli-test-convert-json-{}",
        std::process::id()
    ));
    let socket_dir = temp_dir.join("cauce");
    let _ = std::fs::create_dir_all(&socket_dir);
    let socket_path = socket_dir.join("daemon.sock");

    let input_file = temp_dir.join("video.mp4");
    std::fs::write(&input_file, b"content").unwrap();
    let out_dir = temp_dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let snapshot_val = serde_json::to_value(sample_snapshot()).unwrap();
    let sample_job = serde_json::json!([{
        "id": "job-123",
        "input_path": input_file.to_str().unwrap(),
        "output_path": out_dir.join("video.mp4").to_str().unwrap(),
        "preset_id": "system-smarttv-compat",
        "state": "queued",
        "priority": 0,
        "progress": {"processed_ms": 0, "total_ms": 60000},
        "created_at": "2026-11-24T12:00:00Z",
        "updated_at": "2026-11-24T12:00:00Z"
    }]);

    let server = run_mock_daemon(
        socket_path,
        vec![
            (
                "initialize",
                serde_json::json!({"server_version": "0.1.1", "protocol_version": 1}),
            ),
            (
                "presets.list",
                serde_json::json!([{"id": "system-smarttv-compat"}]),
            ),
            ("media.inspect", serde_json::json!([snapshot_val])),
            ("jobs.add", sample_job.clone()),
        ],
    );

    let output = Command::new(env!("CARGO_BIN_EXE_cauce-cli"))
        .args([
            "convert",
            input_file.to_str().unwrap(),
            "--output-dir",
            out_dir.to_str().unwrap(),
            "--json",
        ])
        .env("XDG_RUNTIME_DIR", &temp_dir)
        .output()
        .expect("binary should execute");

    assert_eq!(output.status.code(), Some(EXIT_SUCCESS));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed_json: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be valid JSON");
    assert_eq!(parsed_json[0]["id"], "job-123");

    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&temp_dir);
}
