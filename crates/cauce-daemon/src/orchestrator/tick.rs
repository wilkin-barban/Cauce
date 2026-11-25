use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cauce_core::{
    ConversionPlan, DuplicatePolicy, ErrorCode, Job, JobError, JobState, MediaSnapshot, Preset,
    Progress, Timestamp, build_ffmpeg_args, next_startable, plan_conversion, temp_path,
    validate_output,
};
use cauce_ffmpeg::runner::spawn;

use crate::dispatch::{Daemon, broadcast_queue};
use crate::events::Event;

use super::{OrchestratorConfig, REASON_SCHEDULER, RunningJob, SAVE_PROGRESS_INTERVAL};

/// One scheduling tick. Returns the job it started, if any.
pub fn tick(daemon: &Daemon, config: &OrchestratorConfig) -> Result<Option<String>, JobError> {
    finish_finished(daemon, config);
    start_next(daemon, config)
}

/// Reaps and judges every conversion whose process already ended.
fn finish_finished(daemon: &Daemon, config: &OrchestratorConfig) {
    for finished in take_finished(daemon) {
        let id = finished.id.clone();
        if let Err(error) = finish_one(daemon, config, finished) {
            daemon.events().log(
                "error",
                "queue",
                format!("no se pudo cerrar el trabajo `{id}`: {}", error.message),
            );
        }
    }
}

/// One child that left the map, plus what the non-blocking status check saw.
struct Finished {
    id: String,
    running: RunningJob,
    /// Set when polling the child failed; the job is failed, never waited on.
    error: Option<JobError>,
}

/// Removes the finished children from the map, so nothing ends them twice.
///
/// `try_exit` is the source of truth and never blocks: it reaps the child as
/// soon as the OS reports the exit, so a converter that closed stderr early
/// finishes even though no [`StreamEnd`] marker ever fires. The marker remains
/// the fast path for the common case; both arrive here and the handle leaves the
/// map exactly once, which is what makes publishing and failing idempotent.
fn take_finished(daemon: &Daemon) -> Vec<Finished> {
    let mut children = daemon.children();
    let mut done: Vec<(String, Option<JobError>)> = Vec::new();
    for (id, running) in children.iter_mut() {
        match running.child.try_exit() {
            Ok(None) => {}
            Ok(Some(_)) => done.push((id.clone(), None)),
            // A status check that failed also ends the job: retrying it every
            // tick would pin the work in `running` forever.
            Err(error) => done.push((id.clone(), Some(error))),
        }
    }
    done.into_iter()
        .filter_map(|(id, error)| {
            children
                .remove(&id)
                .map(|running| Finished { id, running, error })
        })
        .collect()
}

fn finish_one(
    daemon: &Daemon,
    config: &OrchestratorConfig,
    finished: Finished,
) -> Result<(), JobError> {
    let id = finished.id.as_str();
    let mut running = finished.running;
    if let Some(error) = finished.error {
        return fail(daemon, id, &error);
    }
    // The child is already reaped and its reader threads are joined or detached,
    // so `wait` only reports the cached status and never blocks the tick.
    match running.child.wait() {
        Ok(_) => publish(daemon, config, id),
        Err(error) => fail(daemon, id, &error),
    }
}

/// `running -> validating`, validate, publish atomically, `validating -> completed`.
fn publish(daemon: &Daemon, config: &OrchestratorConfig, id: &str) -> Result<(), JobError> {
    let validating = {
        let mut store = daemon.store_mut();
        let Some(job) = store.get_job(id)? else {
            return Ok(());
        };
        // Cancelled or paused while the process was finishing: leave it alone.
        if job.state != JobState::Running {
            return Ok(());
        }
        store.transition(id, JobState::Validating, REASON_SCHEDULER)?
    };
    daemon.events().job_updated(&validating);

    match validate_and_publish(&validating, config) {
        Ok(()) => {
            let completed = {
                let mut store = daemon.store_mut();
                store.set_temp_path(id, None)?;
                let completed = store.transition(id, JobState::Completed, REASON_SCHEDULER)?;
                store.release_inactive_reservations()?;
                completed
            };
            daemon.events().job_updated(&completed);
            broadcast_queue(daemon);
            daemon.events().log(
                "info",
                "queue",
                format!("trabajo `{id}` completado en `{}`", completed.output_path),
            );
            Ok(())
        }
        Err(error) => fail(daemon, id, &error),
    }
}

/// `docs/architecture.md` §12: the temporary exists, FFprobe reads it, the plan
/// matches and only then is it renamed onto the destination.
fn validate_and_publish(job: &Job, config: &OrchestratorConfig) -> Result<(), JobError> {
    let temp = job.temp_path.clone().ok_or_else(|| {
        validation_failed(format!(
            "El trabajo no tiene temporal que validar: no se publica `{}`.",
            job.output_path
        ))
    })?;
    let probed = std::fs::metadata(&temp);
    match probed {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {}
        Ok(_) => {
            return Err(validation_failed(format!(
                "El conversor no produjo ningún archivo en `{temp}`; no se publica nada."
            )));
        }
        Err(error) => {
            return Err(validation_failed(format!(
                "El temporal `{temp}` no existe ({error}); no se publica nada."
            )));
        }
    }

    let source = job.probe.clone().ok_or_else(|| {
        validation_failed(format!(
            "El trabajo no guardó el probe de `{}`.",
            job.input_path
        ))
    })?;
    let plan = job.plan.clone().ok_or_else(|| {
        validation_failed("El trabajo no guardó el plan y un plan persistido nunca se recalcula.")
    })?;
    let probed_at = utc_now()?;
    let output = cauce_ffmpeg::probe::probe_file(&config.toolchain, Path::new(&temp), probed_at)?;
    validate_output(&plan, &source, &output)?;

    let destination = Path::new(&job.output_path);
    if config.duplicate_policy != DuplicatePolicy::Overwrite && destination.exists() {
        return Err(JobError::new(
            ErrorCode::OutputCollision,
            format!(
                "El destino `{}` apareció durante la conversión y la política `{}` no \
                 sobrescribe; el temporal quedó en `{temp}`.",
                job.output_path,
                policy_name(config.duplicate_policy),
            ),
            true,
        ));
    }
    std::fs::rename(&temp, destination).map_err(|error| {
        validation_failed(format!(
            "No se pudo publicar `{}` desde `{temp}`: {error}.",
            job.output_path
        ))
    })
}

/// Stores the error *before* the transition: the schema refuses a `failed` row
/// without `error_code` (`docs/data-model.md` §2.2).
fn fail(daemon: &Daemon, id: &str, error: &JobError) -> Result<(), JobError> {
    let keep_temp = {
        let store = daemon.store_mut();
        store.settings_get()?.keep_temp_on_failure
    };

    let failed = {
        let mut store = daemon.store_mut();
        let Some(job) = store.get_job(id)? else {
            return Ok(());
        };
        if matches!(
            job.state,
            JobState::Completed | JobState::Cancelled | JobState::Failed
        ) {
            return Ok(());
        }
        store.set_error(id, error)?;
        let failed = store.transition(id, JobState::Failed, REASON_SCHEDULER)?;
        if !keep_temp {
            if let Some(temp) = failed.temp_path.as_deref() {
                let _ = std::fs::remove_file(temp);
            }
            store.set_temp_path(id, None)?;
        }
        store.release_inactive_reservations()?;
        failed
    };

    daemon.events().job_updated(&failed);
    broadcast_queue(daemon);
    daemon.events().log(
        "warn",
        "queue",
        format!("trabajo `{id}` falló: {}", error.message),
    );
    Ok(())
}

/// Starts the next job the scheduler allows, or nothing.
fn start_next(daemon: &Daemon, config: &OrchestratorConfig) -> Result<Option<String>, JobError> {
    let (jobs, running) = {
        let store = daemon.store_mut();
        (store.list_jobs(None)?, store.active_count()?)
    };
    let paused = daemon.events().queue_paused();
    let Some(job) = next_startable(&jobs, running, config.max_concurrent, paused) else {
        return Ok(None);
    };
    let job = job.clone();

    match start(daemon, config, &job) {
        Ok(()) => Ok(Some(job.id)),
        Err(error) => {
            daemon.events().log(
                "error",
                "queue",
                format!("trabajo `{}` no pudo arrancar: {}", job.id, error.message),
            );
            fail(daemon, &job.id, &error)?;
            Ok(None)
        }
    }
}

/// The plan a job executes is the one frozen when it was queued
/// (`docs/contracts.md` §3: a persisted plan is never recomputed).
///
/// Recomputing it here silently dropped every phase 1 choice: the argv was
/// rebuilt from the preset alone, the track list vanished, and a subtitle
/// transcode ended up in the legacy single-track path with no stream index.
fn plan_for_execution(
    persisted: Option<&ConversionPlan>,
    probe: &MediaSnapshot,
    preset: &Preset,
) -> Result<ConversionPlan, JobError> {
    match persisted {
        Some(plan) => Ok(plan.clone()),
        None => plan_conversion(probe, preset),
    }
}

/// `queued -> probing`, probe, plan, persist, `probing -> running`, spawn.
fn start(daemon: &Daemon, config: &OrchestratorConfig, job: &Job) -> Result<(), JobError> {
    let probing = {
        let mut store = daemon.store_mut();
        store.transition(&job.id, JobState::Probing, REASON_SCHEDULER)?
    };
    daemon.events().job_updated(&probing);

    let probed_at = utc_now()?;
    let probe =
        cauce_ffmpeg::probe::probe_file(&config.toolchain, Path::new(&job.input_path), probed_at)?;
    // System or custom: one resolution path for every job (`docs/contracts.md` §6).
    let preset = daemon.preset(&job.preset_id)?;
    let plan = plan_for_execution(job.plan.as_ref(), &probe, &preset)?;
    let temp = temp_path(&job.output_path, &job.id)?;
    let args = build_ffmpeg_args(&job.input_path, &temp, &plan, &config.host_filters)?;

    let running = {
        let mut store = daemon.store_mut();
        store.set_probe_and_plan(&job.id, &probe, &plan)?;
        store.set_temp_path(&job.id, Some(&temp))?;
        store.transition(&job.id, JobState::Running, REASON_SCHEDULER)?
    };
    daemon.events().job_updated(&running);

    let (exited, on_progress, on_stderr) = sinks(daemon, &job.id);
    let child = spawn(
        &config.toolchain.ffmpeg,
        &args,
        probe.duration_ms,
        on_progress,
        on_stderr,
    )?;
    daemon
        .children()
        .insert(job.id.clone(), RunningJob { child, exited });

    broadcast_queue(daemon);
    daemon.events().log(
        "info",
        "ffmpeg",
        format!(
            "trabajo `{}` convirtiendo `{}` en `{temp}`",
            job.id, job.input_path
        ),
    );
    Ok(())
}

/// Callbacks for the runner: progress to the store and the socket, stderr to the
/// log ring, and the [`StreamEnd`] marker that tells the next tick to reap.
type Sinks = (
    Arc<AtomicBool>,
    Box<dyn FnMut(Progress) + Send>,
    Box<dyn FnMut(&str) + Send>,
);

fn sinks(daemon: &Daemon, id: &str) -> Sinks {
    let exited = Arc::new(AtomicBool::new(false));
    let store = daemon.store_handle();
    let events = daemon.events_handle();

    let job_id = id.to_owned();
    let last_saved: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let progress_store = Arc::clone(&store);
    let progress_events = Arc::clone(&events);
    let progress_id = job_id.clone();
    let progress_last = Arc::clone(&last_saved);
    let on_progress: Box<dyn FnMut(Progress) + Send> = Box::new(move |progress: Progress| {
        progress_events.broadcast(Event {
            method: "job.progress".to_owned(),
            params: serde_json::json!({"id": progress_id, "progress": progress}),
        });
        if due(&progress_last) {
            if let Ok(mut store) = progress_store.lock() {
                let _ = store.save_progress(&progress_id, &progress);
            }
            if let Ok(mut last) = progress_last.lock() {
                *last = Some(Instant::now());
            }
        }
    });

    let marker = StreamEnd(Arc::clone(&exited));
    let log_events = events;
    let on_stderr: Box<dyn FnMut(&str) + Send> = Box::new(move |line: &str| {
        // The marker dies with this callback, which is how the tick learns the
        // process ended.
        let _held = &marker;
        log_events.log("debug", "ffmpeg", line);
    });

    (exited, on_progress, on_stderr)
}

/// True when a sample is old enough to be persisted (at most 1 Hz per job).
fn due(last: &Mutex<Option<Instant>>) -> bool {
    let last = last.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    last.is_none_or(|at| at.elapsed() >= SAVE_PROGRESS_INTERVAL)
}

/// Signals that FFmpeg's output streams closed, i.e. that the process ended.
struct StreamEnd(Arc<AtomicBool>);

impl Drop for StreamEnd {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn validation_failed(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::OutputValidationFailed, message, true)
}

fn policy_name(policy: DuplicatePolicy) -> &'static str {
    match policy {
        DuplicatePolicy::Suffix => "suffix",
        DuplicatePolicy::Skip => "skip",
        DuplicatePolicy::Overwrite => "overwrite",
    }
}

fn utc_now() -> Result<Timestamp, JobError> {
    cauce_jobs::migrations::utc_now()
}

#[cfg(test)]
mod tests {
    use super::plan_for_execution;
    use cauce_core::{
        AudioMixMode, MediaSnapshot, StreamAction, SubtitleMode, Timestamp, TrackSelection,
        plan_conversion, plan_conversion_with, system_presets,
    };

    #[rustfmt::skip]
    fn snapshot() -> MediaSnapshot {
        use cauce_core::{AudioStream, VideoStream};
        MediaSnapshot {
            path: "/media/oak.mkv".into(),
            container: "mkv".into(),
            duration_ms: 60_000,
            size_bytes: 1024,
            bitrate_bps: 8_000_000,
            video_streams: vec![VideoStream {
                index: 0, codec: "hevc".into(), profile: None, level: None,
                width: 3840, height: 1606, fps: 23.976, pixel_format: "yuv420p10le".into(),
                bit_depth: 10, is_hdr: true, bitrate_bps: None, language: None,
            }],
            audio_streams: vec![AudioStream {
                index: 2, codec: "eac3".into(), channels: 6, sample_rate: 48_000,
                bitrate_bps: Some(960_000), language: Some("spa".into()), is_default: false,
            }],
            subtitle_streams: Vec::new(),
            probed_at: Timestamp::parse("2026-11-24T10:00:00Z").expect("stamp"),
        }
    }

    fn preset() -> &'static cauce_core::Preset {
        system_presets()
            .iter()
            .find(|preset| preset.id == "system-smarttv-compat")
            .expect("preset")
    }

    #[test]
    fn the_frozen_plan_is_the_one_the_job_executes() {
        // The regression: recomputing the plan at start dropped every phase 1
        // choice and rebuilt the argv from the preset alone.
        let source = snapshot();
        let selection = TrackSelection {
            audio_track_indices: vec![2],
            subtitle_track_index: None,
            subtitle_mode: SubtitleMode::Strip,
            audio_mix_mode: AudioMixMode::DualTrackUniversal,
        };
        let frozen =
            plan_conversion_with(&source, preset(), Some(&selection), None).expect("frozen plan");
        assert_eq!(frozen.audio_tracks.len(), 2, "the fixture plans two tracks");

        let executed = plan_for_execution(Some(&frozen), &source, preset()).expect("executed plan");

        assert_eq!(executed, frozen, "the executor must not re-plan");
        assert_ne!(
            executed,
            plan_conversion(&source, preset()).expect("legacy plan"),
            "and that matters because the preset alone cannot produce it"
        );
        assert_eq!(executed.audio_tracks[1].action, StreamAction::Transcode);
    }

    #[test]
    fn a_job_without_a_frozen_plan_still_gets_one() {
        let source = snapshot();
        let executed = plan_for_execution(None, &source, preset()).expect("plan");
        assert_eq!(
            executed,
            plan_conversion(&source, preset()).expect("legacy plan")
        );
    }
}
