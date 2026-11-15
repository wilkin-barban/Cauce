//! Job insertion, recovery and settings contracts.
//!
//! See `docs/data-model.md` §2.2 (jobs), §2.5 (settings), §2.6
//! (output_reservations), §3 (transactions) and §4 (recovery), plus
//! `docs/contracts.md` §2.4 and §3. Fixtures are built with the domain types and
//! every database lives in a throwaway directory under `std::env::temp_dir()`.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cauce_core::{
    DuplicatePolicy, ErrorCode, Job, JobState, Language, Progress, Settings, Theme, Timestamp,
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
            "cauce-jobs-store-{label}-{}-{unique}",
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

/// A job in `state`, created at `created_second`, writing to `output`.
fn job(state: JobState, priority: i32, created_second: u32, output: &str) -> Job {
    let finished = if state == JobState::Completed {
        Some(stamp(created_second + 1))
    } else {
        None
    };
    Job {
        id: Uuid::now_v7().to_string(),
        input_path: format!("/media/library/input-{created_second}.mkv"),
        output_path: output.to_owned(),
        temp_path: None,
        preset_id: "system-smarttv-compat".to_owned(),
        state,
        priority,
        progress: empty_progress(),
        probe: None,
        plan: None,
        error: None,
        created_at: stamp(created_second),
        updated_at: stamp(created_second),
        started_at: None,
        finished_at: finished,
    }
}

fn count(store: &Store, table: &str) -> i64 {
    store
        .connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count must be readable")
}

fn state_str(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Probing => "probing",
        JobState::Running => "running",
        JobState::Paused => "paused",
        JobState::Validating => "validating",
        JobState::Cancelling => "cancelling",
        JobState::Completed => "completed",
        JobState::Cancelled => "cancelled",
        JobState::Failed => "failed",
        JobState::Interrupted => "interrupted",
    }
}

fn valid_settings() -> Settings {
    Settings {
        output_dir: "/media/out".to_owned(),
        max_concurrent_jobs: 2,
        theme: Theme::Dark,
        language: Language::PtBr,
        notifications: true,
        duplicate_policy: DuplicatePolicy::Skip,
        keep_temp_on_failure: true,
    }
}

#[test]
fn inserts_a_job_with_reservation_and_event() {
    let temp = TempDir::new("insert");
    let path = temp.db_path();
    let mut store = Store::open(&path).expect("open");
    let queued = job(JobState::Queued, 0, 1, "/media/out/movie.mp4");

    store.insert_job(&queued).expect("insert must succeed");

    assert_eq!(count(&store, "jobs"), 1);
    assert_eq!(count(&store, "output_reservations"), 1);
    assert_eq!(count(&store, "job_events"), 1);

    let conn = store.connection();
    let (reserved_path, reserved_job): (String, String) = conn
        .query_row("SELECT path, job_id FROM output_reservations", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("the reservation must be readable");
    assert_eq!(reserved_path, queued.output_path);
    assert_eq!(reserved_job, queued.id);

    let (from_state, to_state, reason): (Option<String>, String, String) = conn
        .query_row(
            "SELECT from_state, to_state, reason FROM job_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("the creation event must be readable");
    assert_eq!(from_state, None, "creation has no previous state");
    assert_eq!(to_state, "queued");
    assert_eq!(reason, "user");

    assert_eq!(store.get_job(&queued.id).expect("get"), Some(queued));
    assert_eq!(store.get_job("missing").expect("get"), None);
}

#[test]
fn rejects_a_second_active_job_with_the_same_output() {
    let temp = TempDir::new("collision");
    let path = temp.db_path();
    let mut store = Store::open(&path).expect("open");
    let output = "/media/out/movie.mp4";

    store
        .insert_job(&job(JobState::Queued, 0, 1, output))
        .expect("first insert wins");
    let second = job(JobState::Queued, 0, 2, output);
    let error = store
        .insert_job(&second)
        .expect_err("the same output must collide");

    assert_eq!(error.code, ErrorCode::OutputCollision);
    assert!(error.retryable);
    assert_eq!(count(&store, "jobs"), 1, "no second job row");
    assert_eq!(count(&store, "output_reservations"), 1, "one reservation");
    assert_eq!(count(&store, "job_events"), 1, "no second event");
    assert_eq!(store.get_job(&second.id).expect("get"), None);
}

#[test]
fn marks_active_jobs_interrupted_on_recovery() {
    let temp = TempDir::new("recovery");
    let path = temp.db_path();
    let mut store = Store::open(&path).expect("open");

    let active = [
        (JobState::Probing, "/media/out/probing.mp4"),
        (JobState::Running, "/media/out/running.mp4"),
        (JobState::Paused, "/media/out/paused.mp4"),
        (JobState::Validating, "/media/out/validating.mp4"),
    ];
    let mut ids = Vec::new();
    for (second, (state, output)) in active.iter().enumerate() {
        let job = job(*state, 0, second as u32 + 1, output);
        ids.push(job.id.clone());
        store.insert_job(&job).expect("insert");
    }
    let queued = job(JobState::Queued, 0, 9, "/media/out/queued.mp4");
    store.insert_job(&queued).expect("insert");

    let interrupted = store
        .mark_active_as_interrupted()
        .expect("recovery must succeed");
    assert_eq!(interrupted, 4, "only the four active jobs are interrupted");

    for id in &ids {
        let job = store.get_job(id).expect("get").expect("job must exist");
        assert_eq!(
            job.state,
            JobState::Interrupted,
            "`{id}` must be interrupted"
        );
    }
    assert_eq!(
        store
            .get_job(&queued.id)
            .expect("get")
            .expect("job must exist")
            .state,
        JobState::Queued,
        "recovery keeps queued jobs for the scheduler"
    );

    assert_eq!(
        count(&store, "job_events"),
        5 + 4,
        "one recovery event each"
    );
    let recovery: Vec<(String, String, String)> = {
        let conn = store.connection();
        let mut statement = conn
            .prepare(
                "SELECT job_id, from_state, reason FROM job_events \
                 WHERE to_state = 'interrupted' ORDER BY id",
            )
            .expect("events must be queryable");
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("events must be queryable");
        rows.collect::<Result<Vec<_>, _>>()
            .expect("rows must be readable")
    };
    assert_eq!(recovery.len(), 4);
    for (index, (job_id, from_state, reason)) in recovery.iter().enumerate() {
        assert_eq!(job_id, &ids[index]);
        assert_eq!(from_state, &state_str(active[index].0));
        assert_eq!(reason, "recovery");
    }
}

#[test]
fn releases_reservations_of_inactive_jobs() {
    let temp = TempDir::new("release");
    let path = temp.db_path();
    let mut store = Store::open(&path).expect("open");

    let completed = job(JobState::Completed, 0, 1, "/media/out/done.mp4");
    let running = job(JobState::Running, 0, 2, "/media/out/busy.mp4");
    store.insert_job(&completed).expect("insert");
    store.insert_job(&running).expect("insert");
    assert_eq!(count(&store, "output_reservations"), 2);

    let released = store
        .release_inactive_reservations()
        .expect("release must succeed");
    assert_eq!(released, 1, "only the terminal job gives its output back");

    let remaining: String = store
        .connection()
        .query_row("SELECT job_id FROM output_reservations", [], |row| {
            row.get(0)
        })
        .expect("one reservation must remain");
    assert_eq!(remaining, running.id);
}

#[test]
fn rejects_invalid_settings_before_writing() {
    let temp = TempDir::new("settings");
    let path = temp.db_path();
    let mut store = Store::open(&path).expect("open");
    let previous = valid_settings();

    store
        .settings_set(&previous)
        .expect("valid settings must persist");
    assert_eq!(store.settings_get().expect("get"), previous);

    let invalid = Settings {
        max_concurrent_jobs: 9,
        ..previous.clone()
    };
    let error = store
        .settings_set(&invalid)
        .expect_err("concurrency 9 is outside 1..=8");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(!error.retryable);
    assert_eq!(
        store.settings_get().expect("get"),
        previous,
        "a rejected value must not reach the database"
    );
}

#[test]
fn lists_jobs_by_priority_then_age() {
    let temp = TempDir::new("list");
    let path = temp.db_path();
    let mut store = Store::open(&path).expect("open");

    let old_low = job(JobState::Queued, 0, 1, "/media/out/old-low.mp4");
    let newest_high = job(JobState::Queued, 5, 3, "/media/out/newest-high.mp4");
    let oldest_high = job(JobState::Queued, 5, 2, "/media/out/oldest-high.mp4");
    let lowest = job(JobState::Paused, -1, 4, "/media/out/lowest.mp4");
    for queued in [&old_low, &newest_high, &oldest_high, &lowest] {
        store.insert_job(queued).expect("insert");
    }

    let all: Vec<String> = store
        .list_jobs(None)
        .expect("list must succeed")
        .into_iter()
        .map(|job| job.id)
        .collect();
    assert_eq!(
        all,
        vec![
            oldest_high.id.clone(),
            newest_high.id.clone(),
            old_low.id.clone(),
            lowest.id.clone(),
        ],
        "priority DESC, then created_at ASC"
    );

    let queued: Vec<String> = store
        .list_jobs(Some(&[JobState::Queued]))
        .expect("filtered list must succeed")
        .into_iter()
        .map(|job| job.id)
        .collect();
    assert_eq!(
        queued,
        vec![
            oldest_high.id.clone(),
            newest_high.id.clone(),
            old_low.id.clone()
        ]
    );

    assert!(
        store
            .list_jobs(Some(&[JobState::Cancelled]))
            .expect("empty filter must succeed")
            .is_empty()
    );
}
