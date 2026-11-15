//! `Job`, `JobState` and `Progress` contract (`docs/contracts.md` §2.4, §2.5, §4).

mod common;

use cauce_core::{ErrorCode, Job, JobState, Progress, Timestamp};

const PROGRESS_JSON: &str = r#"{
  "percent": 42.5,
  "processed_ms": 5100,
  "total_ms": 12000,
  "fps": 29.97,
  "speed": 1.02,
  "bitrate_bps": 712000,
  "eta_seconds": 7,
  "updated_at": "2026-11-14T10:00:05Z"
}"#;

const JOB_JSON: &str = r#"{
  "id": "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b",
  "input_path": "/home/wilkin/videos/holiday.mp4",
  "output_path": "/home/wilkin/videos/holiday-smarttv.mp4",
  "temp_path": null,
  "preset_id": "system-smarttv-compat",
  "state": "queued",
  "priority": 0,
  "progress": {
    "percent": null,
    "processed_ms": 0,
    "total_ms": 12000,
    "fps": null,
    "speed": null,
    "bitrate_bps": null,
    "eta_seconds": null,
    "updated_at": "2026-11-14T10:00:00Z"
  },
  "probe": null,
  "plan": null,
  "error": null,
  "created_at": "2026-11-14T10:00:00Z",
  "updated_at": "2026-11-14T10:00:00Z",
  "started_at": null,
  "finished_at": null
}"#;

#[test]
fn progress_round_trips_through_golden_json() {
    let progress = Progress {
        percent: Some(42.5),
        processed_ms: 5_100,
        total_ms: 12_000,
        fps: Some(29.97),
        speed: Some(1.02),
        bitrate_bps: Some(712_000),
        eta_seconds: Some(7),
        updated_at: Timestamp::parse("2026-11-14T10:00:05Z").expect("valid timestamp"),
    };

    common::round_trip(&progress, PROGRESS_JSON);
}

#[test]
fn job_round_trips_through_golden_json() {
    let job = Job {
        id: String::from("0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"),
        input_path: String::from("/home/wilkin/videos/holiday.mp4"),
        output_path: String::from("/home/wilkin/videos/holiday-smarttv.mp4"),
        temp_path: None,
        preset_id: String::from("system-smarttv-compat"),
        state: JobState::Queued,
        priority: 0,
        progress: Progress {
            percent: None,
            processed_ms: 0,
            total_ms: 12_000,
            fps: None,
            speed: None,
            bitrate_bps: None,
            eta_seconds: None,
            updated_at: Timestamp::parse("2026-11-14T10:00:00Z").expect("valid timestamp"),
        },
        probe: None,
        plan: None,
        error: None,
        created_at: Timestamp::parse("2026-11-14T10:00:00Z").expect("valid timestamp"),
        updated_at: Timestamp::parse("2026-11-14T10:00:00Z").expect("valid timestamp"),
        started_at: None,
        finished_at: None,
    };

    common::round_trip(&job, JOB_JSON);
    job.validate().expect("a queued job is a valid job");
}

#[test]
fn progress_rejects_negative_percent() {
    let raw = PROGRESS_JSON.replace("42.5", "-1.0");
    let error =
        serde_json::from_str::<Progress>(&raw).expect_err("percent below 0 must be rejected");
    assert!(
        error.to_string().contains("percent"),
        "the error must name the offending field, got: {error}"
    );
}

#[test]
fn job_rejects_unknown_state() {
    let raw = JOB_JSON.replace("\"queued\"", "\"exploding\"");
    serde_json::from_str::<Job>(&raw).expect_err("an unknown job state must be rejected");
}

#[test]
fn job_rejects_output_equal_to_input() {
    let raw = JOB_JSON.replace(
        "/home/wilkin/videos/holiday-smarttv.mp4",
        "/home/wilkin/videos/holiday.mp4",
    );
    let job: Job = serde_json::from_str(&raw).expect("the job shape is still valid");
    let error = job
        .validate()
        .expect_err("output_path must never equal input_path");
    assert_eq!(error.code, ErrorCode::InvalidInput);
}
