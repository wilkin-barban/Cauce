//! Mid-flight job writes and removal (`docs/data-model.md` §3, §5;
//! `docs/contracts.md` §3).
//!
//! The scheduler persists the probe, the plan and the temporary path before a
//! conversion starts, streams progress, stores the error before a job becomes
//! `failed` (the schema refuses that row without one) and deletes a terminal job
//! with its events and its reservation.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cauce_core::{
    ConversionPlan, ErrorCode, Job, JobError, JobState, MediaSnapshot, Progress, Timestamp,
    plan_conversion, system_presets,
};
use cauce_jobs::Store;
use uuid::Uuid;

/// A private database directory that cleans itself up.
struct TempDir {
    dir: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "cauce-jobs-updates-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir must be creatable");
        Self { dir }
    }

    fn db_path(&self) -> PathBuf {
        self.dir.join("cauce.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn stamp(seconds: u32) -> Timestamp {
    Timestamp::parse(&format!("2026-11-14T10:00:{seconds:02}Z")).expect("valid UTC stamp")
}

fn empty_progress() -> Progress {
    Progress {
        percent: None,
        processed_ms: 0,
        total_ms: 0,
        fps: None,
        speed: None,
        bitrate_bps: None,
        eta_seconds: None,
        updated_at: stamp(0),
    }
}

fn job(state: JobState, output: &str) -> Job {
    let finished = matches!(state, JobState::Completed | JobState::Failed).then(|| stamp(2));
    Job {
        id: Uuid::now_v7().to_string(),
        input_path: "/media/library/input.mkv".to_owned(),
        output_path: output.to_owned(),
        temp_path: None,
        preset_id: "system-smarttv-compat".to_owned(),
        state,
        priority: 0,
        progress: empty_progress(),
        probe: None,
        plan: None,
        error: None,
        created_at: stamp(0),
        updated_at: stamp(0),
        started_at: None,
        finished_at: finished,
    }
}

/// A 2 s H.264/AAC source, written as the contract JSON `docs/contracts.md` §2.1.
fn snapshot() -> MediaSnapshot {
    let golden = serde_json::json!({
        "path": "/media/library/input.mkv", "container": "mkv", "duration_ms": 2000,
        "size_bytes": 400000, "bitrate_bps": 1600000,
        "video_streams": [{ "index": 0, "codec": "h264", "profile": "High", "level": 41,
            "width": 1280, "height": 720, "fps": 30.0, "pixel_format": "yuv420p",
            "bit_depth": 8, "is_hdr": false, "bitrate_bps": 1400000, "language": null }],
        "audio_streams": [{ "index": 1, "codec": "aac", "channels": 2, "sample_rate": 48000,
            "bitrate_bps": 128000, "language": "spa", "is_default": true }],
        "subtitle_streams": [], "probed_at": "2026-11-14T10:00:00Z",
    });
    serde_json::from_value(golden).expect("the golden snapshot is valid")
}

fn plan() -> ConversionPlan {
    let preset = system_presets()
        .iter()
        .find(|preset| preset.id == "system-smarttv-compat")
        .expect("the system catalog carries the compatibility preset");
    plan_conversion(&snapshot(), preset).expect("a plain H.264/AAC source must plan")
}

fn count(store: &Store, table: &str) -> i64 {
    store
        .connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count must be readable")
}

fn events_of(store: &Store, id: &str) -> i64 {
    store
        .connection()
        .query_row(
            "SELECT count(*) FROM job_events WHERE job_id = ?1",
            [id],
            |row| row.get(0),
        )
        .expect("events must be readable")
}

fn parse_state(name: &str) -> JobState {
    serde_json::from_value(serde_json::json!(name)).expect("estado conocido")
}

fn state_name(state: JobState) -> &'static str {
    state.as_str()
}

/// Walks `id` through `states`, which must be a legal path from its current one.
fn drive(store: &mut Store, id: &str, states: &[JobState]) {
    for state in states {
        store
            .transition(id, *state, "tests")
            .unwrap_or_else(|error| panic!("transición a `{state:?}`: {}", error.message));
    }
}

#[test]
fn persists_probe_plan_and_temp() {
    let temp = TempDir::new("probe-plan");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let queued = job(JobState::Queued, "/media/out/input.mp4");
    store.insert_job(&queued).expect("insert");

    let probe = snapshot();
    let planned = plan();
    store
        .set_probe_and_plan(&queued.id, &probe, &planned)
        .expect("persist probe and plan");
    store
        .set_temp_path(&queued.id, Some("/media/out/.cauce-input.part.mp4"))
        .expect("persist the temporary");

    let stored = store
        .get_job(&queued.id)
        .expect("get")
        .expect("the job exists");
    assert_eq!(stored.probe, Some(probe), "probe_json round-trips");
    assert_eq!(stored.plan, Some(planned), "plan_json round-trips");
    assert_eq!(
        stored.temp_path.as_deref(),
        Some("/media/out/.cauce-input.part.mp4")
    );
    assert_eq!(
        stored.state,
        JobState::Queued,
        "persisting does not move it"
    );
    assert_eq!(events_of(&store, &queued.id), 1, "no event is written");

    store
        .set_temp_path(&queued.id, None)
        .expect("clear the temporary");
    let cleared = store.get_job(&queued.id).expect("get").expect("job");
    assert_eq!(cleared.temp_path, None, "the temporary is cleared");

    let error = store
        .set_probe_and_plan("no-existe", &snapshot(), &plan())
        .expect_err("an unknown id must be rejected");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
}

#[test]
fn saves_progress_without_touching_state() {
    let temp = TempDir::new("progress");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let queued = job(JobState::Queued, "/media/out/progress.mp4");
    store.insert_job(&queued).expect("insert");
    let before = store.get_job(&queued.id).expect("get").expect("job");

    let progress = Progress {
        percent: Some(62.5),
        processed_ms: 1_250,
        total_ms: 2_000,
        fps: Some(30.0),
        speed: Some(1.4),
        bitrate_bps: Some(900_000),
        eta_seconds: Some(1),
        updated_at: stamp(9),
    };
    store
        .save_progress(&queued.id, &progress)
        .expect("save the progress sample");

    let after = store.get_job(&queued.id).expect("get").expect("job");
    assert_eq!(after.progress, progress, "progress_json carries the sample");
    assert_eq!(after.state, JobState::Queued, "the state is untouched");
    assert_eq!(after.error, None, "no error is invented");
    assert_eq!(after.plan, None, "the plan is untouched");
    assert_ne!(
        after.updated_at, before.updated_at,
        "updated_at follows the sample"
    );
    assert_eq!(
        events_of(&store, &queued.id),
        1,
        "a progress sample is not an event (docs/data-model.md §3)"
    );

    let error = store
        .save_progress("no-existe", &progress)
        .expect_err("an unknown id must be rejected");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
}

#[test]
fn set_error_then_transition_to_failed_is_allowed() {
    let temp = TempDir::new("error-first");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let queued = job(JobState::Queued, "/media/out/error.mp4");
    store.insert_job(&queued).expect("insert");
    drive(&mut store, &queued.id, &[JobState::Probing]);

    let failure = JobError::new(
        ErrorCode::ProcessFailed,
        "FFmpeg terminó con error: exit status: 1",
        true,
    );
    store
        .set_error(&queued.id, &failure)
        .expect("persist the error before the state change");
    let failed = store
        .transition(&queued.id, JobState::Failed, "scheduler")
        .expect("a failed job with an error satisfies the schema CHECK");

    assert_eq!(failed.state, JobState::Failed);
    assert_eq!(failed.error, Some(failure));
    assert!(failed.finished_at.is_some(), "a terminal job is stamped");
    let stored = store.get_job(&queued.id).expect("get").expect("job");
    assert_eq!(stored.error, failed.error, "the error is persisted");
    assert_eq!(
        events_of(&store, &queued.id),
        3,
        "creation, probing, failed"
    );
}

#[test]
fn transition_to_failed_without_error_is_rejected() {
    let temp = TempDir::new("error-missing");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let queued = job(JobState::Queued, "/media/out/missing-error.mp4");
    store.insert_job(&queued).expect("insert");
    drive(&mut store, &queued.id, &[JobState::Probing]);

    let error = store
        .transition(&queued.id, JobState::Failed, "scheduler")
        .expect_err("the schema refuses a failed job without an error");
    assert_eq!(error.code, ErrorCode::Internal, "{error:?}");

    let stored = store.get_job(&queued.id).expect("get").expect("job");
    assert_eq!(stored.state, JobState::Probing, "the state is unchanged");
    assert_eq!(stored.error, None, "no error was written");
    assert_eq!(
        events_of(&store, &queued.id),
        2,
        "a rejected transition writes no event"
    );
}

#[test]
fn remove_deletes_job_events_and_reservation() {
    let temp = TempDir::new("remove");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let queued = job(JobState::Queued, "/media/out/remove.mp4");
    store.insert_job(&queued).expect("insert");
    drive(
        &mut store,
        &queued.id,
        &[
            JobState::Probing,
            JobState::Running,
            JobState::Validating,
            JobState::Completed,
        ],
    );
    assert_eq!(events_of(&store, &queued.id), 5, "the whole trail exists");
    assert_eq!(count(&store, "output_reservations"), 1);

    let removed = store.remove_job(&queued.id).expect("remove");

    assert!(removed, "a terminal job is removable");
    assert_eq!(count(&store, "jobs"), 0);
    assert_eq!(count(&store, "job_events"), 0, "its events are gone");
    assert_eq!(
        count(&store, "output_reservations"),
        0,
        "and its reservation"
    );
    assert_eq!(store.get_job(&queued.id).expect("get"), None);
}

#[test]
fn remove_refuses_an_active_job() {
    let temp = TempDir::new("remove-active");
    let mut store = Store::open(&temp.db_path()).expect("open");

    // One job per slot-occupying state: none of them may be deleted.
    let active = [
        (JobState::Probing, &["probing"][..]),
        (JobState::Running, &["probing", "running"][..]),
        (JobState::Paused, &["probing", "running", "paused"][..]),
        (
            JobState::Validating,
            &["probing", "running", "validating"][..],
        ),
        (
            JobState::Cancelling,
            &["probing", "running", "cancelling"][..],
        ),
    ];
    for (expected, path) in active {
        let output = format!("/media/out/{}.mp4", state_name(expected));
        let queued = job(JobState::Queued, &output);
        store.insert_job(&queued).expect("insert");
        drive(
            &mut store,
            &queued.id,
            &path
                .iter()
                .map(|state| parse_state(state))
                .collect::<Vec<_>>(),
        );
        let error = store
            .remove_job(&queued.id)
            .expect_err("an active job must not be removed");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
        assert!(
            error.message.contains("activo"),
            "the message explains the refusal: {}",
            error.message
        );
        let stored = store.get_job(&queued.id).expect("get").expect("job");
        assert_eq!(stored.state, expected, "the job keeps its state");
        assert_eq!(stored.plan, None, "nothing else was written");
    }
    assert_eq!(count(&store, "jobs"), 5, "nothing was deleted");
    // One creation event plus the transitions each job needed: 2 + 3 + 4 + 4 + 4.
    assert_eq!(count(&store, "job_events"), 17);
}

#[test]
fn remove_of_a_missing_job_returns_false() {
    let temp = TempDir::new("remove-missing");
    let mut store = Store::open(&temp.db_path()).expect("open");

    assert!(
        !store
            .remove_job("no-existe")
            .expect("ask for a missing job"),
        "a missing job is not an error"
    );
    assert_eq!(count(&store, "jobs"), 0);
}
