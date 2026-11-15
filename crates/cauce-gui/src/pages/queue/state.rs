//! Decision core of the Cola page: no GTK type appears here, so the phase, the
//! actions each state offers, the progress merge and the footer counters are all
//! tested without a display (`docs/ui-mvp.md` §5, §9, §12; `docs/contracts.md`
//! §2.4, §2.5, §4, §6 and §7).
//!
//! The page never keeps a divergent copy of the queue: [`QueueState::apply_jobs`]
//! replaces the whole list with the daemon's answer, and
//! [`QueueState::apply_progress`] only refines a job that already exists, so an
//! event for an unknown id cannot invent a row.

use cauce_core::{Job, JobState, Progress};

use crate::i18n::t;

/// The ten states of `docs/contracts.md` §4, in transition order.
pub const STATES: [JobState; 10] = [
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
];

/// The four states of `docs/ui-mvp.md` §5: loading, no jobs, a list, or the error
/// page with its retry action. Empty and error are never the same screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePhase {
    Loading,
    Empty,
    Ready,
    Error,
}

/// Everything a row of the list can ask the daemon to do, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobAction {
    Pause,
    Resume,
    Cancel,
    Retry,
    /// Requeues an `interrupted` job (`docs/contracts.md` §4 and §6).
    Recover,
    Remove,
}

impl JobAction {
    /// The `docs/contracts.md` §6 method this button invokes.
    pub fn method(self) -> &'static str {
        match self {
            JobAction::Pause => "jobs.pause",
            JobAction::Resume => "jobs.resume",
            JobAction::Cancel => "jobs.cancel",
            JobAction::Retry => "jobs.retry",
            JobAction::Recover => "jobs.recover",
            JobAction::Remove => "jobs.remove",
        }
    }

    /// Catalog key of the label, which is also the accessible name of the button.
    pub fn label_key(self) -> &'static str {
        match self {
            JobAction::Pause => "queue.action_pause",
            JobAction::Resume => "queue.action_resume",
            JobAction::Cancel => "queue.action_cancel",
            JobAction::Retry => "queue.action_retry",
            JobAction::Recover => "queue.recover",
            JobAction::Remove => "queue.action_remove",
        }
    }

    /// The translated label of the action in the current locale.
    pub fn label(self) -> &'static str {
        t(self.label_key())
    }

    /// Symbolic icon shipped by the Adwaita theme: no borrowed asset.
    pub fn icon(self) -> &'static str {
        match self {
            JobAction::Pause => "media-playback-pause-symbolic",
            JobAction::Resume => "media-playback-start-symbolic",
            JobAction::Cancel => "process-stop-symbolic",
            JobAction::Retry => "view-refresh-symbolic",
            // A different glyph from `retry`: recovery is not a second attempt.
            JobAction::Recover => "edit-redo-symbolic",
            JobAction::Remove => "edit-delete-symbolic",
        }
    }

    /// Destructive actions ask first (`docs/ui-mvp.md` §5 and §9).
    pub fn needs_confirmation(self) -> bool {
        matches!(self, JobAction::Cancel | JobAction::Remove)
    }
}

/// Counts per state plus the number of jobs holding a slot, for the footer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueSummary {
    queued: usize,
    probing: usize,
    running: usize,
    paused: usize,
    validating: usize,
    cancelling: usize,
    completed: usize,
    cancelled: usize,
    failed: usize,
    interrupted: usize,
}

impl QueueSummary {
    /// Jobs in one state; a state with no rows answers zero.
    pub fn count(&self, state: JobState) -> usize {
        match state {
            JobState::Queued => self.queued,
            JobState::Probing => self.probing,
            JobState::Running => self.running,
            JobState::Paused => self.paused,
            JobState::Validating => self.validating,
            JobState::Cancelling => self.cancelling,
            JobState::Completed => self.completed,
            JobState::Cancelled => self.cancelled,
            JobState::Failed => self.failed,
            JobState::Interrupted => self.interrupted,
        }
    }

    /// Jobs that hold a slot, the same set `cauce-jobs` counts as active.
    pub fn active(&self) -> usize {
        self.probing + self.running + self.paused + self.validating + self.cancelling
    }

    pub fn queued(&self) -> usize {
        self.queued
    }

    pub fn total(&self) -> usize {
        self.queued
            + self.active()
            + self.completed
            + self.cancelled
            + self.failed
            + self.interrupted
    }

    /// One line for the page footer, skipping the groups that are empty.
    pub fn footer(&self) -> String {
        let mut parts = vec![t("queue.footer_total").replace("{n}", &self.total().to_string())];
        if self.active() > 0 {
            parts.push(named(
                self.active(),
                t("queue.footer_active"),
                t("queue.footer_active_plural"),
            ));
        }
        if self.queued > 0 {
            parts.push(named(
                self.queued,
                t("queue.footer_queued"),
                t("queue.footer_queued_plural"),
            ));
        }
        parts.join(" · ")
    }
}

fn named(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

/// Everything the Cola page decides, with no GTK types.
#[derive(Debug, Default)]
pub struct QueueState {
    jobs: Vec<Job>,
    queue_paused: bool,
    last_error: Option<String>,
    loaded: bool,
}

impl QueueState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loading until the first `jobs.list` answers; error wins over the list.
    pub fn phase(&self) -> QueuePhase {
        if self.last_error.is_some() {
            QueuePhase::Error
        } else if !self.loaded {
            QueuePhase::Loading
        } else if self.jobs.is_empty() {
            QueuePhase::Empty
        } else {
            QueuePhase::Ready
        }
    }

    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    pub fn queue_paused(&self) -> bool {
        self.queue_paused
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Newest first, as `docs/ui-mvp.md` §5 asks; the daemon's own listing order
    /// is ascending because that is what the scheduler consumes.
    ///
    /// A `jobs.list` answer carries the progress the daemon persisted (at most
    /// once per second) while `job.progress` samples arrive twice per second, so
    /// a fresher sample already in memory is kept instead of moving backwards.
    pub fn apply_jobs(&mut self, mut jobs: Vec<Job>) {
        for job in jobs.iter_mut() {
            let fresher = self
                .jobs
                .iter()
                .find(|known| known.id == job.id)
                .map(|known| &known.progress)
                .filter(|known| known.updated_at > job.progress.updated_at);
            if let Some(progress) = fresher {
                job.progress = progress.clone();
            }
        }
        jobs.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| right.id.cmp(&left.id))
        });
        self.jobs = jobs;
        self.loaded = true;
        self.last_error = None;
    }

    /// Refines the progress of a job that is already listed; unknown ids are
    /// ignored so a late event cannot create a phantom row.
    pub fn apply_progress(&mut self, id: &str, progress: Progress) {
        if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
            job.progress = progress;
        }
    }

    pub fn set_queue_paused(&mut self, paused: bool) {
        self.queue_paused = paused;
    }

    /// The header control: `queue.start` while paused, `queue.pause` while active
    /// (`docs/contracts.md` §6). The key is both the call and the label, so the
    /// two cannot disagree.
    pub fn toggle(&self) -> &'static str {
        if self.queue_paused {
            "queue.start"
        } else {
            "queue.pause"
        }
    }

    /// Surfaces a concrete failure inside the page (`docs/ui-mvp.md` §9).
    pub fn set_error(&mut self, message: String) {
        self.last_error = Some(message);
    }

    /// Only the actions the state machine allows, in display order.
    pub fn allowed_actions(&self, id: &str) -> Vec<JobAction> {
        let Some(job) = self.jobs.iter().find(|job| job.id == id) else {
            return Vec::new();
        };
        [
            JobAction::Pause,
            JobAction::Resume,
            JobAction::Cancel,
            JobAction::Retry,
            JobAction::Recover,
            JobAction::Remove,
        ]
        .into_iter()
        .filter(|action| allowed(*action, job.state))
        .collect()
    }

    /// The footer counters.
    pub fn summary(&self) -> QueueSummary {
        let mut summary = QueueSummary::default();
        for job in self.jobs.iter() {
            let field = match job.state {
                JobState::Queued => &mut summary.queued,
                JobState::Probing => &mut summary.probing,
                JobState::Running => &mut summary.running,
                JobState::Paused => &mut summary.paused,
                JobState::Validating => &mut summary.validating,
                JobState::Cancelling => &mut summary.cancelling,
                JobState::Completed => &mut summary.completed,
                JobState::Cancelled => &mut summary.cancelled,
                JobState::Failed => &mut summary.failed,
                JobState::Interrupted => &mut summary.interrupted,
            };
            *field += 1;
        }
        summary
    }

    /// A quiet line under the header while the queue does not start new jobs.
    pub fn pause_hint(&self) -> Option<&'static str> {
        self.queue_paused.then(|| t(PAUSED_HINT))
    }
}

/// Catalog key of the line shown while `queue.pause` is in effect
/// (`docs/ui-mvp.md` §5), so the hint follows the locale.
pub const PAUSED_HINT: &str = "queue.paused_hint";

/// The five action guards of `docs/ui-mvp.md` §5 and `docs/contracts.md` §4.
fn allowed(action: JobAction, state: JobState) -> bool {
    match action {
        JobAction::Pause => state == JobState::Running,
        JobAction::Resume => state == JobState::Paused,
        JobAction::Cancel => matches!(
            state,
            JobState::Queued | JobState::Probing | JobState::Running | JobState::Paused
        ),
        JobAction::Retry => state == JobState::Failed,
        // Only the recovery path of `docs/contracts.md` §4: it is not `retry`, and
        // a job that was interrupted never resumes by itself.
        JobAction::Recover => state == JobState::Interrupted,
        // Terminal (completed, cancelled) or still waiting; an active job can
        // never be removed from under its process.
        JobAction::Remove => state.is_terminal() || matches!(state, JobState::Queued),
    }
}

/// Catalog key of the state of a row; the name is never colour alone
/// (`docs/ui-mvp.md` §10).
pub fn state_key(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "state.queued",
        JobState::Probing => "state.probing",
        JobState::Running => "state.running",
        JobState::Paused => "state.paused",
        JobState::Validating => "state.validating",
        JobState::Cancelling => "state.cancelling",
        JobState::Completed => "state.completed",
        JobState::Cancelled => "state.cancelled",
        JobState::Failed => "state.failed",
        JobState::Interrupted => "state.interrupted",
    }
}

/// The translated state of a row.
pub fn state_label(state: JobState) -> &'static str {
    t(state_key(state))
}

/// Copy of a job whose process died with the daemon (`docs/ui-mvp.md` §12).
pub fn interrupted_hint() -> &'static str {
    t("queue.interrupted_hint")
}

/// What the progress bar says. No duration means unknown percent, never `0 %`
/// (`docs/contracts.md` §2.5).
pub fn progress_label(
    percent: Option<f64>,
    fps: Option<f64>,
    speed: Option<f64>,
    eta: Option<u64>,
) -> String {
    let mut parts = Vec::with_capacity(4);
    parts.push(match percent {
        Some(percent) => format!("{percent:.0} {}", t("queue.percent_unit")),
        None => t("queue.unknown_percent").to_owned(),
    });
    if let Some(fps) = fps {
        parts.push(format!("{fps:.1} {}", t("queue.fps_unit")));
    }
    if let Some(speed) = speed {
        parts.push(format!("{speed:.2}{}", t("queue.speed_unit")));
    }
    if let Some(eta) = eta {
        parts.push(t("queue.eta").replace("{clock}", &clock(eta)));
    }
    parts.join(" · ")
}

/// Hours only when they are there, so short jobs read as `2:05`.
pub fn clock(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}
