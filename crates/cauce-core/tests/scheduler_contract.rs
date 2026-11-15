//! The pure scheduling decision (`docs/contracts.md` §4 rules and §2.4 `priority`).
//!
//! `next_startable` is the only scheduling policy of v0.1: it never mutates a
//! job, never blocks and knows nothing about hardware.

use cauce_core::{Job, JobState, Progress, Timestamp, next_startable};

fn stamp(seconds: u32) -> Timestamp {
    Timestamp::parse(&format!("2026-11-14T10:00:{seconds:02}Z")).expect("valid UTC stamp")
}

fn progress(seconds: u32) -> Progress {
    Progress {
        percent: None,
        processed_ms: 0,
        total_ms: 0,
        fps: None,
        speed: None,
        bitrate_bps: None,
        eta_seconds: None,
        updated_at: stamp(seconds),
    }
}

/// A job in `state`, created at `created_second`, with a fixed `id`.
fn job(id: &str, state: JobState, priority: i32, created_second: u32) -> Job {
    Job {
        id: id.to_owned(),
        input_path: format!("/media/library/{id}.mkv"),
        output_path: format!("/media/out/{id}.mp4"),
        temp_path: None,
        preset_id: "system-smarttv-compat".to_owned(),
        state,
        priority,
        progress: progress(created_second),
        probe: None,
        plan: None,
        error: None,
        created_at: stamp(created_second),
        updated_at: stamp(created_second),
        started_at: None,
        finished_at: None,
    }
}

fn id_of(job: Option<&Job>) -> Option<&str> {
    job.map(|job| job.id.as_str())
}

#[test]
fn picks_the_highest_priority_queued_job() {
    let jobs = vec![
        job("a", JobState::Queued, 0, 1),
        job("b", JobState::Queued, 10, 2),
        job("c", JobState::Queued, -5, 3),
    ];

    assert_eq!(id_of(next_startable(&jobs, 0, 1, false)), Some("b"));
}

#[test]
fn breaks_ties_by_created_at_then_id() {
    let by_age = vec![
        job("later", JobState::Queued, 0, 5),
        job("earlier", JobState::Queued, 0, 2),
    ];
    assert_eq!(id_of(next_startable(&by_age, 0, 1, false)), Some("earlier"));

    let by_id = vec![
        job("b", JobState::Queued, 0, 2),
        job("a", JobState::Queued, 0, 2),
    ];
    assert_eq!(id_of(next_startable(&by_id, 0, 1, false)), Some("a"));
}

#[test]
fn starts_nothing_when_the_queue_is_paused() {
    let jobs = vec![job("a", JobState::Queued, 0, 1)];

    assert_eq!(id_of(next_startable(&jobs, 0, 4, true)), None);
}

#[test]
fn starts_nothing_at_the_concurrency_limit() {
    let jobs = vec![job("a", JobState::Queued, 0, 1)];

    assert_eq!(id_of(next_startable(&jobs, 2, 2, false)), None);
    assert_eq!(
        id_of(next_startable(&jobs, 3, 2, false)),
        None,
        "over the limit must never start work"
    );
    assert_eq!(
        id_of(next_startable(&jobs, 1, 2, false)),
        Some("a"),
        "one free slot is enough to start"
    );
}

#[test]
fn ignores_non_queued_jobs() {
    let jobs = vec![
        job("running", JobState::Running, 100, 1),
        job("paused", JobState::Paused, 90, 2),
        job("validating", JobState::Validating, 80, 3),
        job("interrupted", JobState::Interrupted, 70, 4),
        job("queued", JobState::Queued, -100, 5),
    ];

    assert_eq!(id_of(next_startable(&jobs, 0, 4, false)), Some("queued"));

    let active_only = vec![
        job("running", JobState::Running, 100, 1),
        job("probing", JobState::Probing, 90, 2),
        job("cancelling", JobState::Cancelling, 80, 3),
    ];
    assert_eq!(id_of(next_startable(&active_only, 0, 8, false)), None);
}

#[test]
fn starts_nothing_on_an_empty_queue() {
    let jobs: Vec<Job> = Vec::new();

    assert_eq!(id_of(next_startable(&jobs, 0, 1, false)), None);
    assert_eq!(id_of(next_startable(&jobs, 0, 8, true)), None);
}
