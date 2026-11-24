//! The job state machine (`docs/contracts.md` §4).
//!
//! One table, one place: [`JobState::can_transition_to`] is a literal copy of the
//! document, and every user action is derived from it by [`transition_for`], so a
//! guard can never disagree with the table. The module is pure: no clock, no
//! store, no IPC.

use crate::error::{ErrorCode, JobError};
use crate::job::JobState;

/// The transitions of `docs/contracts.md` §4, state by state.
const QUEUED: &[JobState] = &[JobState::Probing, JobState::Cancelling];
const PROBING: &[JobState] = &[JobState::Running, JobState::Failed, JobState::Cancelling];
const RUNNING: &[JobState] = &[
    JobState::Paused,
    JobState::Validating,
    JobState::Cancelling,
    JobState::Failed,
    JobState::Interrupted,
];
const PAUSED: &[JobState] = &[JobState::Running, JobState::Cancelling];
const CANCELLING: &[JobState] = &[JobState::Cancelled, JobState::Failed];
const VALIDATING: &[JobState] = &[JobState::Completed, JobState::Failed, JobState::Interrupted];
const COMPLETED: &[JobState] = &[];
const CANCELLED: &[JobState] = &[];
const FAILED: &[JobState] = &[JobState::Queued];
const INTERRUPTED: &[JobState] = &[JobState::Queued];

impl JobState {
    /// `completed` and `cancelled` close the job; `failed` and `interrupted` keep
    /// their documented way back to `queued`.
    pub fn is_terminal(self) -> bool {
        matches!(self, JobState::Completed | JobState::Cancelled)
    }

    /// Exactly the table of `docs/contracts.md` §4. Anything else is invalid.
    pub fn can_transition_to(self, next: JobState) -> bool {
        self.allowed_transitions().contains(&next)
    }

    /// The `Permitidos` column of `docs/contracts.md` §4.
    pub fn allowed_transitions(self) -> &'static [JobState] {
        match self {
            JobState::Queued => QUEUED,
            JobState::Probing => PROBING,
            JobState::Running => RUNNING,
            JobState::Paused => PAUSED,
            JobState::Validating => VALIDATING,
            JobState::Cancelling => CANCELLING,
            JobState::Completed => COMPLETED,
            JobState::Cancelled => CANCELLED,
            JobState::Failed => FAILED,
            JobState::Interrupted => INTERRUPTED,
        }
    }

    /// The contract name of the state, as SQLite and the IPC write it.
    pub fn as_str(self) -> &'static str {
        match self {
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
}

/// The user actions of `docs/contracts.md` §4, with their own guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobAction {
    Pause,
    Resume,
    Cancel,
    Retry,
    Recover,
}

impl JobAction {
    /// Spanish verb used in the rejection message the UI shows.
    pub fn as_str(self) -> &'static str {
        match self {
            JobAction::Pause => "pausar",
            JobAction::Resume => "reanudar",
            JobAction::Cancel => "cancelar",
            JobAction::Retry => "reintentar",
            JobAction::Recover => "recuperar",
        }
    }
}

/// Resolves a user action into its target state.
///
/// Every pair outside the documented cases is an `INVALID_INPUT` that is not
/// retryable: repeating the same request can never succeed.
pub fn transition_for(action: JobAction, from: JobState) -> Result<JobState, JobError> {
    let target = match (action, from) {
        (JobAction::Pause, JobState::Running) => Some(JobState::Paused),
        (JobAction::Resume, JobState::Paused) => Some(JobState::Running),
        (
            JobAction::Cancel,
            JobState::Queued | JobState::Probing | JobState::Running | JobState::Paused,
        ) => Some(JobState::Cancelling),
        (JobAction::Retry, JobState::Failed) => Some(JobState::Queued),
        (JobAction::Recover, JobState::Interrupted) => Some(JobState::Queued),
        _ => None,
    };

    // The candidate is validated against the table itself, so the guards and the
    // table can never drift apart.
    match target {
        Some(next) if from.can_transition_to(next) => Ok(next),
        _ => Err(JobError::new(
            ErrorCode::InvalidInput,
            format!(
                "No se puede {} un trabajo en estado `{}`.",
                action.as_str(),
                from.as_str()
            ),
            false,
        )),
    }
}

/// The daemon restart rule of `docs/contracts.md` §4: active work is interrupted,
/// and nothing is requeued automatically.
pub fn on_restart(state: JobState) -> JobState {
    match state {
        // `contracts.md` §4 lists probing/running/validating; `data-model.md` §4 also
        // lists paused, and a paused job's process dies with the daemon, so pausing
        // cannot survive a restart. The recovery path (T11) already interrupts it.
        // `cancelling` belongs here too: it counts as active for the scheduler
        // and `jobs.remove` refuses it, so a crash mid-cancellation would leave a
        // row that blocks the queue forever.
        JobState::Probing
        | JobState::Running
        | JobState::Paused
        | JobState::Validating
        | JobState::Cancelling => JobState::Interrupted,
        other => other,
    }
}
