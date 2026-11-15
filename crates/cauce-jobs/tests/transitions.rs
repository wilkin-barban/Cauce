//! Transactional state transitions (`docs/contracts.md` §3, §4 and
//! `docs/data-model.md` §3 "Transición de estado").
//!
//! Every test runs against a private database in a throwaway directory: a state
//! change and its `job_events` row must land together, and a rejected transition
//! must leave the database byte-identical.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cauce_core::{
    ErrorCode, Job, JobAction, JobError, JobState, Progress, Timestamp, transition_for,
};
use cauce_jobs::Store;
use uuid::Uuid;

struct TempDir {
    dir: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "cauce-jobs-transitions-{label}-{}-{unique}",
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

fn progress() -> Progress {
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

/// A job in `state`; `failed` carries the error the schema requires.
fn job(state: JobState, created_second: u32) -> Job {
    let id = Uuid::now_v7().to_string();
    let error = (state == JobState::Failed).then(|| {
        JobError::new(
            ErrorCode::ProcessFailed,
            "FFmpeg terminó con el código 1.".to_owned(),
            true,
        )
    });
    Job {
        input_path: format!("/media/library/{id}.mkv"),
        output_path: format!("/media/out/{id}.mp4"),
        temp_path: None,
        preset_id: "system-smarttv-compat".to_owned(),
        state,
        priority: 0,
        progress: progress(),
        probe: None,
        plan: None,
        error,
        created_at: stamp(created_second),
        updated_at: stamp(created_second),
        started_at: None,
        finished_at: None,
        id,
    }
}

fn open(label: &str) -> (TempDir, Store) {
    let temp = TempDir::new(label);
    let store = Store::open(&temp.db_path()).expect("open");
    (temp, store)
}

fn event_count(store: &Store) -> i64 {
    store
        .connection()
        .query_row("SELECT count(*) FROM job_events", [], |row| row.get(0))
        .expect("event count must be readable")
}

/// The last event of a job as `(from_state, to_state, reason)`.
fn last_event(store: &Store) -> (Option<String>, String, String) {
    store
        .connection()
        .query_row(
            "SELECT from_state, to_state, reason FROM job_events \
             ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("the latest event must be readable")
}

fn state_of(store: &Store, id: &str) -> JobState {
    store
        .get_job(id)
        .expect("read must succeed")
        .expect("job must exist")
        .state
}

#[test]
fn transition_updates_state_and_writes_one_event() {
    let (_temp, mut store) = open("one-event");
    let queued = job(JobState::Queued, 1);
    store.insert_job(&queued).expect("insert");
    assert_eq!(event_count(&store), 1, "creation writes the first event");

    let updated = store
        .transition(&queued.id, JobState::Probing, "scheduler")
        .expect("queued -> probing is documented");

    assert_eq!(updated.state, JobState::Probing);
    assert_eq!(state_of(&store, &queued.id), JobState::Probing);
    assert_eq!(event_count(&store), 2, "one event per state change");
    assert_eq!(
        last_event(&store),
        (
            Some("queued".to_owned()),
            "probing".to_owned(),
            "scheduler".to_owned()
        )
    );
    assert_eq!(
        updated.updated_at,
        store
            .get_job(&queued.id)
            .expect("get")
            .expect("job")
            .updated_at,
        "the read-back timestamp is the one that was written"
    );
    assert!(updated.updated_at >= updated.created_at);
}

#[test]
fn invalid_transition_writes_no_event() {
    let (_temp, mut store) = open("invalid");
    let queued = job(JobState::Queued, 1);
    store.insert_job(&queued).expect("insert");
    let events = event_count(&store);

    let error = store
        .transition(&queued.id, JobState::Running, "scheduler")
        .expect_err("queued -> running is not in contracts.md §4");

    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(!error.retryable);
    assert_eq!(
        state_of(&store, &queued.id),
        JobState::Queued,
        "no state change"
    );
    assert_eq!(event_count(&store), events, "no event is written");
    assert_eq!(
        store.get_job(&queued.id).expect("get").expect("job"),
        queued,
        "the row is untouched"
    );
}

#[test]
fn unknown_job_is_rejected() {
    let (_temp, mut store) = open("unknown");
    let events = event_count(&store);

    let error = store
        .transition("missing", JobState::Probing, "scheduler")
        .expect_err("an unknown id cannot transition");

    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(!error.retryable);
    assert_eq!(event_count(&store), events);
}

#[test]
fn pause_and_resume_round_trip() {
    let (_temp, mut store) = open("pause-resume");
    let running = job(JobState::Running, 1);
    store.insert_job(&running).expect("insert");

    let paused = store
        .apply_action(&running.id, JobAction::Pause)
        .expect("pause from running");
    assert_eq!(paused.state, JobState::Paused);
    assert_eq!(
        last_event(&store).2,
        "user",
        "a user action is recorded as `user`"
    );

    let resumed = store
        .apply_action(&running.id, JobAction::Resume)
        .expect("resume from paused");
    assert_eq!(resumed.state, JobState::Running);

    let error = store
        .apply_action(&running.id, JobAction::Resume)
        .expect_err("resume from running is invalid");
    assert_eq!(
        error,
        transition_for(JobAction::Resume, JobState::Running).expect_err("guarded")
    );
}

#[test]
fn cancel_from_queued_goes_to_cancelling_then_cancelled() {
    let (_temp, mut store) = open("cancel");
    let queued = job(JobState::Queued, 1);
    store.insert_job(&queued).expect("insert");

    let cancelling = store
        .apply_action(&queued.id, JobAction::Cancel)
        .expect("cancel from queued");
    assert_eq!(cancelling.state, JobState::Cancelling);

    let cancelled = store
        .transition(&queued.id, JobState::Cancelled, "user")
        .expect("cancelling -> cancelled is documented");
    assert_eq!(cancelled.state, JobState::Cancelled);
    assert!(cancelled.finished_at.is_some());
    assert_eq!(event_count(&store), 3);

    let error = store
        .apply_action(&queued.id, JobAction::Cancel)
        .expect_err("a cancelled job cannot be cancelled again");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert_eq!(state_of(&store, &queued.id), JobState::Cancelled);
}

#[test]
fn retry_from_failed_requeues() {
    let (_temp, mut store) = open("retry");
    let failed = job(JobState::Failed, 1);
    store.insert_job(&failed).expect("insert");

    let requeued = store
        .apply_action(&failed.id, JobAction::Retry)
        .expect("retry from failed");

    assert_eq!(requeued.state, JobState::Queued);
    assert_eq!(
        last_event(&store),
        (
            Some("failed".to_owned()),
            "queued".to_owned(),
            "user".to_owned()
        )
    );
    assert!(
        store.apply_action(&failed.id, JobAction::Retry).is_err(),
        "a queued job cannot be retried"
    );
}

#[test]
fn recover_from_interrupted_requeues() {
    let (_temp, mut store) = open("recover");
    let interrupted = job(JobState::Interrupted, 1);
    store.insert_job(&interrupted).expect("insert");

    let requeued = store
        .apply_action(&interrupted.id, JobAction::Recover)
        .expect("recover from interrupted");

    assert_eq!(requeued.state, JobState::Queued);
    assert_eq!(state_of(&store, &interrupted.id), JobState::Queued);
    assert_eq!(event_count(&store), 2);
}

#[test]
fn sets_started_and_finished_timestamps() {
    let (_temp, mut store) = open("timestamps");
    let queued = job(JobState::Queued, 1);
    store.insert_job(&queued).expect("insert");
    assert_eq!(queued.started_at, None);
    assert_eq!(queued.finished_at, None);

    let probing = store
        .transition(&queued.id, JobState::Probing, "scheduler")
        .expect("queued -> probing");
    assert_eq!(probing.started_at, None, "probing does not start the clock");

    let running = store
        .transition(&queued.id, JobState::Running, "scheduler")
        .expect("probing -> running");
    let started = running.started_at.clone().expect("running sets started_at");
    assert_eq!(started, running.updated_at);

    let validating = store
        .transition(&queued.id, JobState::Validating, "scheduler")
        .expect("running -> validating");
    assert_eq!(validating.started_at, Some(started.clone()));
    assert_eq!(validating.finished_at, None);

    let completed = store
        .transition(&queued.id, JobState::Completed, "scheduler")
        .expect("validating -> completed");
    let finished = completed.finished_at.clone().expect("completed closes it");
    assert_eq!(finished, completed.updated_at);
    assert_eq!(completed.started_at, Some(started));
    assert!(finished >= *completed.started_at.as_ref().expect("started_at"));
}

#[test]
fn active_count_counts_running_paused_and_cancelling() {
    let (_temp, mut store) = open("active-count");
    assert_eq!(store.active_count().expect("empty store"), 0);

    for (index, state) in [
        JobState::Running,
        JobState::Paused,
        JobState::Probing,
        JobState::Validating,
        JobState::Cancelling,
        JobState::Queued,
        JobState::Interrupted,
    ]
    .into_iter()
    .enumerate()
    {
        let created = u32::try_from(index + 1).expect("small index");
        store
            .insert_job(&job(state, created))
            .expect("insert must succeed");
    }

    assert_eq!(
        store.active_count().expect("count"),
        5,
        "probing, running, paused, validating and cancelling hold a slot"
    );
}
