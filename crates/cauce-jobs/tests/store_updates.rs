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
    ConversionPlan, CustomAudio, CustomBitrate, CustomContainer, CustomPreset, CustomQuality,
    CustomQualityLevel, CustomResolution, CustomSubtitles, CustomVideo, ErrorCode, Job, JobError,
    JobState, MediaSnapshot, Progress, Timestamp, plan_conversion, system_presets,
};
use cauce_jobs::Store;
use cauce_jobs::store::CustomPresetRead;
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

// The one custom preset lives under the `custom_preset` key of `settings`
// (`docs/data-model.md` §2.5, `docs/contracts.md` §2.3 and §6).

/// A legal combination: MKV carries H.265, Opus and subtitles.
fn custom_preset() -> CustomPreset {
    CustomPreset {
        container: CustomContainer::Mkv,
        video: CustomVideo::H265,
        quality: CustomQuality::Level(CustomQualityLevel::High),
        resolution: CustomResolution::P1080,
        audio: CustomAudio::Opus,
        bitrate: Some(CustomBitrate::K256),
        subtitles: CustomSubtitles::Keep,
    }
}

#[test]
fn custom_preset_round_trips_through_settings() {
    let temp = TempDir::new("custom-round-trip");
    let mut store = Store::open(&temp.db_path()).expect("open");
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Missing,
        "sin guardar, no hay preset personalizado"
    );

    let custom = custom_preset();
    store.set_custom_preset(&custom).expect("guardar");
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Ready(custom.clone()),
        "el preset vuelve idéntico"
    );

    // Saving again replaces it: there is exactly one custom preset.
    let mut replaced = custom.clone();
    replaced.video = CustomVideo::Av1;
    store.set_custom_preset(&replaced).expect("reemplazar");
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Ready(replaced),
        "guardar reemplaza, no acumula"
    );

    // The key shares the table with the settings and breaks neither direction.
    let settings = store.settings_get().expect("los ajustes siguen legibles");
    assert_eq!(settings.max_concurrent_jobs, 1);
    store
        .settings_set(&settings)
        .expect("los ajustes se guardan");
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Ready(custom_preset_with_av1()),
        "escribir los ajustes no borra el preset personalizado"
    );

    // Clearing removes it and clearing again is not an error.
    assert!(store.clear_custom_preset().expect("borrar"), "había uno");
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Missing
    );
    assert!(
        !store.clear_custom_preset().expect("borrar de nuevo"),
        "borrar dos veces no inventa un borrado"
    );
}

fn custom_preset_with_av1() -> CustomPreset {
    let mut replaced = custom_preset();
    replaced.video = CustomVideo::Av1;
    replaced
}

#[test]
fn invalid_custom_preset_is_rejected_on_write() {
    let temp = TempDir::new("custom-invalid");
    let mut store = Store::open(&temp.db_path()).expect("open");

    // MP4 does not carry VP9.
    let mut invalid = custom_preset();
    invalid.container = CustomContainer::Mp4;
    invalid.video = CustomVideo::Vp9;
    let error = store
        .set_custom_preset(&invalid)
        .expect_err("una combinación inválida no se guarda");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Missing,
        "un rechazo no deja fila escrita"
    );

    // A bitrate with no audio track is the same class of error.
    let mut silent = custom_preset();
    silent.audio = CustomAudio::None;
    silent.subtitles = CustomSubtitles::Remove;
    assert!(store.set_custom_preset(&silent).is_err());
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Missing
    );
}

#[test]
fn corrupt_custom_preset_is_ignored_on_read() {
    let temp = TempDir::new("custom-corrupt");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let write_raw = |store: &Store, raw: &str| {
        store
            .connection()
            .execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json",
                rusqlite::params!["custom_preset", raw, "2026-11-14T10:00:00Z"],
            )
            .expect("escribir la fila cruda");
    };

    // Broken JSON: ignored, with a warning that says why.
    write_raw(&store, "{esto no es json");
    match store.custom_preset().expect("leer no falla") {
        CustomPresetRead::Ignored { warning } => {
            assert!(
                !warning.trim().is_empty(),
                "la advertencia explica el motivo"
            );
        }
        other => panic!("se esperaba un preset ignorado, llegó {other:?}"),
    }

    // Valid JSON with an invalid combination: ignored as well.
    write_raw(
        &store,
        r#"{"container":"webm","video":"h264","quality":"high","resolution":"original",
            "audio":"opus","bitrate":192,"subtitles":"keep"}"#,
    );
    match store.custom_preset().expect("leer no falla") {
        CustomPresetRead::Ignored { warning } => {
            assert!(
                warning.contains("webm") || warning.contains("h264"),
                "la advertencia nombra la combinación: {warning}"
            );
        }
        other => panic!("se esperaba un preset ignorado, llegó {other:?}"),
    }

    // The rest of the settings still load: a corrupt value never breaks the app.
    let settings = store.settings_get().expect("los ajustes siguen legibles");
    assert_eq!(settings.max_concurrent_jobs, 1);

    // A valid row replaces the corrupt one and reads back.
    store
        .set_custom_preset(&custom_preset())
        .expect("guardar encima");
    assert_eq!(
        store.custom_preset().expect("read"),
        CustomPresetRead::Ready(custom_preset())
    );
}

// The bulk clear of `jobs.clear_finished` (`docs/contracts.md` §6): one
// transaction that deletes the finished jobs with their events and reservations.

/// Drives `id` to `states`, storing the error a `failed` row requires first.
fn drive_to(store: &mut Store, id: &str, states: &[JobState]) {
    for state in states {
        if *state == JobState::Failed {
            let failure = JobError::new(
                ErrorCode::ProcessFailed,
                "FFmpeg terminó con error: exit status: 1",
                true,
            );
            store
                .set_error(id, &failure)
                .expect("the schema needs the error before `failed`");
        }
        store
            .transition(id, *state, "tests")
            .unwrap_or_else(|error| panic!("transición a `{state:?}`: {}", error.message));
    }
}

/// Every state of `docs/contracts.md` §4 with a legal path from `queued`.
fn state_paths() -> [(&'static str, &'static [JobState]); 10] {
    [
        ("queued", &[]),
        ("probing", &[JobState::Probing]),
        ("running", &[JobState::Probing, JobState::Running]),
        (
            "paused",
            &[JobState::Probing, JobState::Running, JobState::Paused],
        ),
        (
            "validating",
            &[JobState::Probing, JobState::Running, JobState::Validating],
        ),
        (
            "cancelling",
            &[JobState::Probing, JobState::Running, JobState::Cancelling],
        ),
        (
            "completed",
            &[
                JobState::Probing,
                JobState::Running,
                JobState::Validating,
                JobState::Completed,
            ],
        ),
        (
            "cancelled",
            &[
                JobState::Probing,
                JobState::Running,
                JobState::Cancelling,
                JobState::Cancelled,
            ],
        ),
        ("failed", &[JobState::Probing, JobState::Failed]),
        (
            "interrupted",
            &[JobState::Probing, JobState::Running, JobState::Interrupted],
        ),
    ]
}

#[test]
fn clear_finished_removes_only_terminal_jobs() {
    let temp = TempDir::new("clear-finished");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let mut ids = Vec::new();
    for (name, path) in state_paths() {
        let output = format!("/media/out/{name}.mp4");
        let queued = job(JobState::Queued, &output);
        store.insert_job(&queued).expect("insert");
        drive_to(&mut store, &queued.id, path);
        ids.push((name, queued.id));
    }
    assert_eq!(count(&store, "jobs"), 10, "un trabajo por estado");

    let removed = store.clear_finished().expect("clear the finished jobs");

    assert_eq!(removed, 4, "solo los cuatro estados terminados");
    let remaining: Vec<String> = ids
        .iter()
        .filter(|(_, id)| store.get_job(id).expect("get").is_some())
        .map(|(name, _)| (*name).to_owned())
        .collect();
    assert_eq!(
        remaining,
        vec![
            "queued",
            "probing",
            "running",
            "paused",
            "validating",
            "cancelling"
        ],
        "un trabajo activo o en espera nunca se borra"
    );
    assert_eq!(count(&store, "jobs"), 6);
    assert_eq!(
        count(&store, "output_reservations"),
        6,
        "las reservas de los que quedan siguen ahí"
    );
    for (name, id) in &ids {
        let trailing = events_of(&store, id);
        if matches!(*name, "completed" | "cancelled" | "failed" | "interrupted") {
            assert_eq!(trailing, 0, "`{name}` se llevó sus eventos");
        } else {
            assert!(trailing > 0, "`{name}` conserva su historia");
        }
    }

    assert_eq!(
        store.clear_finished().expect("clear again"),
        0,
        "sin terminados la limpieza borra cero"
    );
    assert_eq!(count(&store, "jobs"), 6, "y no toca a nadie");
}

#[test]
fn clear_finished_is_atomic() {
    let temp = TempDir::new("clear-atomic");
    let mut store = Store::open(&temp.db_path()).expect("open");
    let finished = job(JobState::Queued, "/media/out/atomic-finished.mp4");
    store.insert_job(&finished).expect("insert");
    drive_to(
        &mut store,
        &finished.id,
        &[
            JobState::Probing,
            JobState::Running,
            JobState::Validating,
            JobState::Completed,
        ],
    );
    let waiting = job(JobState::Queued, "/media/out/atomic-waiting.mp4");
    store.insert_job(&waiting).expect("insert");
    let before = (
        count(&store, "jobs"),
        count(&store, "job_events"),
        count(&store, "output_reservations"),
    );

    // A deletion that fails halfway must leave the three tables as they were.
    store
        .connection()
        .execute_batch(
            "CREATE TRIGGER refuse_clear BEFORE DELETE ON jobs \
             BEGIN SELECT RAISE(ABORT, 'la limpieza no puede terminar'); END;",
        )
        .expect("crear el disparador que falla");

    let error = store
        .clear_finished()
        .expect_err("el borrado falla dentro de la transacción");
    assert_eq!(error.code, ErrorCode::Internal, "{error:?}");

    let after = (
        count(&store, "jobs"),
        count(&store, "job_events"),
        count(&store, "output_reservations"),
    );
    assert_eq!(after, before, "la transacción no deja nada a medias");
    assert!(
        store.get_job(&finished.id).expect("get").is_some(),
        "el trabajo terminado sigue en la tabla"
    );
}
