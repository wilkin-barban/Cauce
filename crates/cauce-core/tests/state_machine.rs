//! The job state machine of `docs/contracts.md` §4.
//!
//! The table below is a literal copy of the document; every state pair of the
//! 10 × 10 matrix is exercised, so an invented edge fails here instead of in the
//! daemon.

use cauce_core::{ErrorCode, JobAction, JobState, on_restart, transition_for};

/// Every state of `docs/contracts.md` §4, in document order.
const ALL: [JobState; 10] = [
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

/// `docs/contracts.md` §4 verbatim: `(| Estado | Permitidos |)`.
const TABLE: &[(JobState, &[JobState])] = &[
    (JobState::Queued, &[JobState::Probing, JobState::Cancelling]),
    (
        JobState::Probing,
        &[JobState::Running, JobState::Failed, JobState::Cancelling],
    ),
    (
        JobState::Running,
        &[
            JobState::Paused,
            JobState::Validating,
            JobState::Cancelling,
            JobState::Failed,
            JobState::Interrupted,
        ],
    ),
    (JobState::Paused, &[JobState::Running, JobState::Cancelling]),
    (
        JobState::Cancelling,
        &[JobState::Cancelled, JobState::Failed],
    ),
    (
        JobState::Validating,
        &[JobState::Completed, JobState::Failed, JobState::Interrupted],
    ),
    (JobState::Completed, &[]),
    (JobState::Cancelled, &[]),
    (JobState::Failed, &[JobState::Queued]),
    (JobState::Interrupted, &[JobState::Queued]),
];

fn documented(state: JobState) -> &'static [JobState] {
    TABLE
        .iter()
        .find(|(from, _)| *from == state)
        .map(|(_, allowed)| *allowed)
        .expect("every state must appear in the table")
}

/// The contract name of a state, as the store and the IPC use it.
fn name(state: JobState) -> &'static str {
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

#[test]
fn every_documented_transition_is_allowed() {
    for (from, allowed) in TABLE {
        assert_eq!(
            from.allowed_transitions(),
            *allowed,
            "`allowed_transitions` must match contracts.md §4 for `{}`",
            name(*from)
        );
        for next in *allowed {
            assert!(
                from.can_transition_to(*next),
                "`{}` -> `{}` is documented and must be allowed",
                name(*from),
                name(*next)
            );
        }
    }
}

#[test]
fn every_undocumented_transition_is_rejected() {
    for from in ALL {
        for next in ALL {
            let expected = documented(from).contains(&next);
            assert_eq!(
                from.can_transition_to(next),
                expected,
                "`{}` -> `{}` must be {}",
                name(from),
                name(next),
                if expected { "allowed" } else { "rejected" }
            );
        }
    }
}

#[test]
fn terminal_states_have_no_exit() {
    for from in [JobState::Completed, JobState::Cancelled] {
        assert!(from.is_terminal(), "`{}` is terminal", name(from));
        assert!(
            from.allowed_transitions().is_empty(),
            "`{}` has no allowed transition",
            name(from)
        );
    }

    // The only documented exits from a stopped job are the explicit requeue.
    for from in [JobState::Failed, JobState::Interrupted] {
        assert!(
            !from.is_terminal(),
            "`{}` can still be requeued",
            name(from)
        );
        assert_eq!(
            from.allowed_transitions(),
            &[JobState::Queued][..],
            "`{}` only goes back to `queued`",
            name(from)
        );
    }

    for from in ALL
        .iter()
        .copied()
        .filter(|state| !matches!(state, JobState::Completed | JobState::Cancelled))
    {
        assert!(!from.is_terminal(), "`{}` is not terminal", name(from));
    }
}

#[test]
fn pause_resume_cancel_guards() {
    for from in ALL {
        let paused = transition_for(JobAction::Pause, from);
        if from == JobState::Running {
            assert_eq!(paused.expect("pause is valid"), JobState::Paused);
        } else {
            assert_guard(paused, "pausar", from);
        }

        let resumed = transition_for(JobAction::Resume, from);
        if from == JobState::Paused {
            assert_eq!(resumed.expect("resume is valid"), JobState::Running);
        } else {
            assert_guard(resumed, "reanudar", from);
        }

        let cancelled = transition_for(JobAction::Cancel, from);
        if matches!(
            from,
            JobState::Queued | JobState::Probing | JobState::Running | JobState::Paused
        ) {
            assert_eq!(cancelled.expect("cancel is valid"), JobState::Cancelling);
        } else {
            assert_guard(cancelled, "cancelar", from);
        }
    }
}

#[test]
fn retry_only_from_failed() {
    for from in ALL {
        let retried = transition_for(JobAction::Retry, from);
        if from == JobState::Failed {
            assert_eq!(retried.expect("retry is valid"), JobState::Queued);
        } else {
            assert_guard(retried, "reintentar", from);
        }
    }
}

#[test]
fn recover_only_from_interrupted() {
    for from in ALL {
        let recovered = transition_for(JobAction::Recover, from);
        if from == JobState::Interrupted {
            assert_eq!(recovered.expect("recover is valid"), JobState::Queued);
        } else {
            assert_guard(recovered, "recuperar", from);
        }
    }
}

#[test]
fn restart_marks_active_work_interrupted() {
    // `data-model.md` §4 includes paused: its process dies with the daemon.
    for from in [
        JobState::Probing,
        JobState::Running,
        JobState::Paused,
        JobState::Validating,
    ] {
        assert_eq!(
            on_restart(from),
            JobState::Interrupted,
            "`{}` must be interrupted after a restart",
            name(from)
        );
    }
}

#[test]
fn restart_leaves_other_states_alone() {
    for from in ALL {
        if matches!(
            from,
            JobState::Probing
                | JobState::Running
                | JobState::Paused
                | JobState::Validating
                | JobState::Cancelling
        ) {
            continue;
        }
        assert_eq!(
            on_restart(from),
            from,
            "`{}` must survive a restart unchanged",
            name(from)
        );
    }
}

/// An invalid action is the human answer of rule §4: `INVALID_INPUT`, not
/// retryable, naming the action and the state it was asked from.
fn assert_guard(result: Result<JobState, cauce_core::JobError>, action: &str, from: JobState) {
    let error = result.expect_err(&format!("`{action}` must be rejected for `{}`", name(from)));
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(!error.retryable);
    assert!(
        error.message.contains(action),
        "the message must name the action: {}",
        error.message
    );
    assert!(
        error.message.contains(name(from)),
        "the message must name the current state: {}",
        error.message
    );
}

#[test]
fn restart_interrupts_a_paused_job_too() {
    // `data-model.md` §4 includes paused; a paused process cannot outlive the daemon.
    assert_eq!(on_restart(JobState::Paused), JobState::Interrupted);
}

#[test]
fn restart_interrupts_a_cancelling_job_too() {
    // `data-model.md` §4 lists the active set; `cancelling` is active for
    // `active_count` and refused by `jobs.remove`, so a crash during
    // cancellation has to leave it recoverable, never eternal.
    assert_eq!(on_restart(JobState::Cancelling), JobState::Interrupted);
}
