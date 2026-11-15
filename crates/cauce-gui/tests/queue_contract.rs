//! Contract of the Cola page core (`docs/ui-mvp.md` §5, §9, §12;
//! `docs/contracts.md` §2.4, §2.5, §4, §6 and §7).
//!
//! The page decides in a plain struct with no GTK type, so the phase, the actions
//! each state offers, the progress merge and the footer counters are all tested
//! without a display. The widgets are only a rendering of this state.

use cauce_core::{Job, JobState, Progress, Timestamp};
use cauce_gui::i18n::t;
use cauce_gui::pages::queue::{JobAction, QueuePhase, QueueState, progress_label};

fn ts(text: &str) -> Timestamp {
    Timestamp::parse(text).expect("timestamp UTC válido")
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
        updated_at: ts("2026-11-14T10:00:00Z"),
    }
}

/// A literal job: `cauce-core` values, no fixtures and no daemon.
fn job(id: &str, state: JobState) -> Job {
    Job {
        id: id.to_owned(),
        input_path: format!("/home/wilkin/Vídeos/{id}.mp4"),
        output_path: format!("/home/wilkin/Vídeos/salida/{id}.mp4"),
        temp_path: None,
        preset_id: "system-web-mp4".to_owned(),
        state,
        priority: 0,
        progress: empty_progress(),
        probe: None,
        plan: None,
        error: None,
        created_at: ts("2026-11-14T10:00:00Z"),
        updated_at: ts("2026-11-14T10:00:00Z"),
        started_at: None,
        finished_at: None,
    }
}

fn state_with(jobs: Vec<Job>) -> QueueState {
    let mut state = QueueState::new();
    state.apply_jobs(jobs);
    state
}

#[test]
fn empty_state_is_distinct_from_error() {
    let mut state = QueueState::new();
    assert_eq!(state.phase(), QueuePhase::Loading);

    state.apply_jobs(Vec::new());
    assert_eq!(state.phase(), QueuePhase::Empty);
    assert!(state.last_error().is_none());

    state.set_error("No se pudo conectar con el daemon.".to_owned());
    assert_eq!(state.phase(), QueuePhase::Error);
    assert_eq!(
        state.last_error(),
        Some("No se pudo conectar con el daemon.")
    );

    // The next successful `jobs.list` recovers the page.
    state.apply_jobs(Vec::new());
    assert_eq!(state.phase(), QueuePhase::Empty);
    assert!(state.last_error().is_none());
}

#[test]
fn allowed_actions_follow_the_state_machine() {
    let cases = [
        (JobState::Queued, vec![JobAction::Cancel, JobAction::Remove]),
        (JobState::Probing, vec![JobAction::Cancel]),
        (JobState::Running, vec![JobAction::Pause, JobAction::Cancel]),
        (JobState::Paused, vec![JobAction::Resume, JobAction::Cancel]),
        (JobState::Validating, vec![]),
        (JobState::Cancelling, vec![]),
        (JobState::Completed, vec![JobAction::Remove]),
        (JobState::Cancelled, vec![JobAction::Remove]),
        (JobState::Failed, vec![JobAction::Retry]),
        (JobState::Interrupted, vec![JobAction::Recover]),
    ];

    for (job_state, expected) in cases {
        let state = state_with(vec![job("a", job_state)]);
        assert_eq!(
            state.allowed_actions("a"),
            expected,
            "acciones de `{}`",
            job_state.as_str()
        );
    }
}

#[test]
fn cancel_is_offered_only_for_cancellable_states() {
    let cancellable = [
        JobState::Queued,
        JobState::Probing,
        JobState::Running,
        JobState::Paused,
    ];
    for job_state in all_states() {
        let state = state_with(vec![job("a", job_state)]);
        let offered = state.allowed_actions("a").contains(&JobAction::Cancel);
        assert_eq!(
            offered,
            cancellable.contains(&job_state),
            "cancelar en `{}`",
            job_state.as_str()
        );
    }
}

#[test]
fn retry_is_offered_only_for_failed() {
    for job_state in all_states() {
        let state = state_with(vec![job("a", job_state)]);
        let offered = state.allowed_actions("a").contains(&JobAction::Retry);
        assert_eq!(
            offered,
            job_state == JobState::Failed,
            "reintentar en `{}`",
            job_state.as_str()
        );
    }
}

#[test]
fn remove_is_offered_only_for_terminal_and_queued() {
    let removable = [JobState::Queued, JobState::Completed, JobState::Cancelled];
    for job_state in all_states() {
        let state = state_with(vec![job("a", job_state)]);
        let offered = state.allowed_actions("a").contains(&JobAction::Remove);
        assert_eq!(
            offered,
            removable.contains(&job_state),
            "eliminar en `{}`",
            job_state.as_str()
        );
    }
}

#[test]
fn recover_is_offered_only_for_interrupted() {
    for job_state in all_states() {
        let state = state_with(vec![job("a", job_state)]);
        let offered = state.allowed_actions("a").contains(&JobAction::Recover);
        assert_eq!(
            offered,
            job_state == JobState::Interrupted,
            "reencolar en `{}`",
            job_state.as_str()
        );
    }
    assert_eq!(
        JobAction::Recover.method(),
        "jobs.recover",
        "la acción llama al método del contrato"
    );
    assert!(
        !JobAction::Recover.needs_confirmation(),
        "reencolar no destruye nada"
    );
}

#[test]
fn queue_start_label_depends_on_the_paused_flag() {
    let mut state = QueueState::new();
    assert!(
        !state.queue_paused(),
        "una cola nueva está activa: no se pausa sola"
    );
    assert_eq!(
        state.toggle(),
        "queue.pause",
        "cola activa: el control pausa"
    );
    assert_eq!(t(state.toggle()), "Pausar cola", "la etiqueta existe");

    state.set_queue_paused(true);
    assert_eq!(
        state.toggle(),
        "queue.start",
        "cola pausada: el control inicia, nunca reanuda ni duplica"
    );
    assert_eq!(t(state.toggle()), "Iniciar cola", "la etiqueta existe");

    state.set_queue_paused(false);
    assert_eq!(state.toggle(), "queue.pause", "y vuelve a pausar");
}

#[test]
fn progress_update_only_touches_the_matching_job() {
    let mut state = state_with(vec![
        job("a", JobState::Running),
        job("b", JobState::Running),
    ]);
    let running = Progress {
        percent: Some(42.0),
        processed_ms: 4_200,
        total_ms: 10_000,
        fps: Some(30.0),
        speed: Some(1.25),
        bitrate_bps: Some(2_000_000),
        eta_seconds: Some(6),
        updated_at: ts("2026-11-14T10:00:05Z"),
    };
    state.apply_progress("b", running.clone());

    let jobs = state.jobs();
    assert_eq!(jobs.len(), 2);
    let by_id = |id: &str| {
        jobs.iter()
            .find(|job| job.id == id)
            .expect("el trabajo sigue listado")
    };
    assert_eq!(by_id("a").progress, empty_progress());
    assert_eq!(by_id("b").progress, running);
}

#[test]
fn indeterminate_progress_renders_without_percent() {
    let label = progress_label(None, Some(30.0), Some(1.5), Some(125));
    assert!(
        !label.contains("0 %"),
        "el porcentaje desconocido no se imprime como cero: {label}"
    );
    assert!(
        label.to_lowercase().contains("desconocid"),
        "la etiqueta nombra el porcentaje desconocido: {label}"
    );
    assert!(label.contains("30.0 FPS"), "FPS presentes: {label}");
    assert!(label.contains("1.50×"), "velocidad presente: {label}");
    assert!(label.contains("2:05"), "ETA presente: {label}");

    let known = progress_label(Some(42.4), None, None, None);
    assert_eq!(known, "42 %");
}

#[test]
fn summary_counts_every_state_and_the_active_jobs() {
    let jobs = all_states()
        .iter()
        .enumerate()
        .map(|(index, job_state)| job(&format!("j{index}"), *job_state))
        .collect();
    let summary = state_with(jobs).summary();

    for job_state in all_states() {
        assert_eq!(
            summary.count(job_state),
            1,
            "cuenta de `{}`",
            job_state.as_str()
        );
    }
    assert_eq!(summary.total(), 10);
    assert_eq!(summary.active(), 5);
    let footer = summary.footer();
    assert!(footer.contains("10 en total"), "total en el pie: {footer}");
    assert!(footer.contains("5 activos"), "activos en el pie: {footer}");
    assert!(
        footer.contains("1 en espera"),
        "en espera en el pie: {footer}"
    );
}

#[test]
fn queue_paused_flag_is_reflected_in_the_phase_hint() {
    let mut state = state_with(Vec::new());
    assert!(!state.queue_paused());
    assert!(state.pause_hint().is_none());

    state.set_queue_paused(true);
    assert!(state.queue_paused());
    let hint = state
        .pause_hint()
        .expect("la cola pausada muestra el aviso");
    assert!(
        hint.contains("siguen"),
        "el aviso aclara que los trabajos en curso siguen: {hint}"
    );

    state.set_queue_paused(false);
    assert!(state.pause_hint().is_none());
}

#[test]
fn unknown_job_updates_are_ignored() {
    let mut state = state_with(vec![job("a", JobState::Running)]);
    let progress = Progress {
        percent: Some(10.0),
        ..empty_progress()
    };
    state.apply_progress("fantasma", progress);

    assert_eq!(state.jobs().len(), 1, "no se inventa una fila fantasma");
    assert_eq!(state.jobs()[0].id, "a");
    assert_eq!(state.summary().total(), 1);
    assert!(state.allowed_actions("fantasma").is_empty());
}

#[test]
fn fresh_progress_survives_a_stale_list() {
    let sample = Progress {
        percent: Some(42.0),
        updated_at: ts("2026-11-14T10:00:09Z"),
        ..empty_progress()
    };
    let mut state = state_with(vec![job("a", JobState::Running)]);
    state.apply_progress("a", sample.clone());

    // The persisted row the daemon answers is older than the live sample.
    let mut stale = job("a", JobState::Running);
    stale.progress = Progress {
        percent: Some(40.0),
        updated_at: ts("2026-11-14T10:00:08Z"),
        ..empty_progress()
    };
    state.apply_jobs(vec![stale.clone()]);
    assert_eq!(state.jobs()[0].progress, sample, "el progreso no retrocede");

    // A newer persisted row does replace the sample.
    stale.progress = Progress {
        percent: Some(55.0),
        updated_at: ts("2026-11-14T10:00:10Z"),
        ..empty_progress()
    };
    state.apply_jobs(vec![stale.clone()]);
    assert_eq!(state.jobs()[0].progress, stale.progress);
}

fn all_states() -> [JobState; 10] {
    [
        JobState::Queued,
        JobState::Probing,
        JobState::Running,
        JobState::Paused,
        JobState::Validating,
        JobState::Cancelling,
        JobState::Completed,
        JobState::Cancelled,
        JobState::Failed,
        JobState::Interrupted,
    ]
}
