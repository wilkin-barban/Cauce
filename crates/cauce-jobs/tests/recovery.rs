//! Recovery contracts: reconciling orphan temporary files after an abrupt stop.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cauce_core::{Job, JobState, Progress, Timestamp, temp_belongs_to};
use cauce_jobs::Store;
use cauce_jobs::recovery::{Reconciliation, reconcile_temp_files};
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
            "cauce-jobs-recovery-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir must be creatable");
        Self { dir }
    }

    fn db_path(&self) -> PathBuf {
        self.dir.join("cauce.db")
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn stamp() -> Timestamp {
    Timestamp::parse("2026-11-14T10:00:00Z").expect("valid UTC stamp")
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
        updated_at: stamp(),
    }
}

fn test_job(state: JobState, temp_path: Option<String>) -> Job {
    let id = Uuid::now_v7().to_string();
    Job {
        id: id.clone(),
        input_path: format!("/media/library/input-{id}.mkv"),
        output_path: format!("/media/out/output-{id}.mp4"),
        temp_path,
        preset_id: "system-smarttv-compat".to_owned(),
        state,
        priority: 0,
        progress: empty_progress(),
        probe: None,
        plan: None,
        error: None,
        created_at: stamp(),
        updated_at: stamp(),
        started_at: None,
        finished_at: None,
    }
}

#[test]
fn temp_belongs_to_pure_guard() {
    assert!(temp_belongs_to("/var/tmp/.cauce-job1.part.mp4", "job1"));
    assert!(temp_belongs_to(".cauce-job1.part.mkv", "job1"));
    assert!(!temp_belongs_to("/var/tmp/.cauce-job1.part.", "job1"));
    assert!(!temp_belongs_to("/var/tmp/.cauce-job2.part.mp4", "job1"));
    assert!(!temp_belongs_to("/var/tmp/movie.mp4", "job1"));
    assert!(!temp_belongs_to("", "job1"));
    assert!(!temp_belongs_to("/var/tmp/.cauce-job1.part.mp4", ""));
}

#[test]
fn reconcile_removes_the_temp_file_of_an_interrupted_job() {
    let temp = TempDir::new("interrupted");
    let mut store = Store::open(&temp.db_path()).expect("open store");

    let mut job = test_job(JobState::Interrupted, None);
    let temp_file = temp.file_path(&format!(".cauce-{}.part.mp4", job.id));
    fs::write(&temp_file, b"partial transcode bytes").expect("write temp file");
    job.temp_path = Some(temp_file.to_str().unwrap().to_owned());
    store.insert_job(&job).expect("insert job");

    assert!(temp_file.exists(), "temp file must exist before reconcile");

    let report = reconcile_temp_files(&store).expect("reconcile must succeed");
    assert_eq!(
        report,
        Reconciliation {
            removed: 1,
            missing: 0,
            failures: Vec::new(),
        }
    );
    assert!(!temp_file.exists(), "orphan temp file must be removed");
}

#[test]
fn reconcile_keeps_the_temp_file_of_a_running_job() {
    let temp = TempDir::new("running");
    let mut store = Store::open(&temp.db_path()).expect("open store");

    let mut job = test_job(JobState::Running, None);
    let temp_file = temp.file_path(&format!(".cauce-{}.part.mp4", job.id));
    fs::write(&temp_file, b"active transcode bytes").expect("write temp file");
    job.temp_path = Some(temp_file.to_str().unwrap().to_owned());
    store.insert_job(&job).expect("insert job");

    assert!(temp_file.exists());

    let report = reconcile_temp_files(&store).expect("reconcile must succeed");
    assert_eq!(
        report,
        Reconciliation {
            removed: 0,
            missing: 0,
            failures: Vec::new(),
        }
    );
    assert!(
        temp_file.exists(),
        "active job's temp file must never be touched"
    );
}

#[test]
fn reconcile_ignores_a_path_that_is_not_a_cauce_temp_file() {
    let temp = TempDir::new("foreign");
    let mut store = Store::open(&temp.db_path()).expect("open store");

    let mut job = test_job(JobState::Interrupted, None);
    let user_file = temp.file_path("my_important_video.mp4");
    fs::write(&user_file, b"original video data").expect("write user file");
    job.temp_path = Some(user_file.to_str().unwrap().to_owned());
    store.insert_job(&job).expect("insert job");

    assert!(user_file.exists());

    let report = reconcile_temp_files(&store).expect("reconcile must succeed");
    assert_eq!(
        report,
        Reconciliation {
            removed: 0,
            missing: 0,
            failures: Vec::new(),
        }
    );
    assert!(
        user_file.exists(),
        "a file not matching .cauce-<job_id>.part.<ext> must never be deleted"
    );
}

#[test]
fn reconcile_reports_a_missing_file_without_failing() {
    let temp = TempDir::new("missing");
    let mut store = Store::open(&temp.db_path()).expect("open store");

    let mut job = test_job(JobState::Interrupted, None);
    let temp_file = temp.file_path(&format!(".cauce-{}.part.mp4", job.id));
    job.temp_path = Some(temp_file.to_str().unwrap().to_owned());
    store.insert_job(&job).expect("insert job");

    let report = reconcile_temp_files(&store).expect("reconcile must succeed");
    assert_eq!(
        report,
        Reconciliation {
            removed: 0,
            missing: 1,
            failures: Vec::new(),
        }
    );
}
