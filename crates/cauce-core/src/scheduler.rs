//! The scheduling decision (`docs/contracts.md` §4).
//!
//! `next_startable` is the only scheduling policy of v0.1: `queued` jobs by
//! `priority` descending, then `created_at` ascending, then `id` ascending. It
//! never mutates a job, never blocks and never looks at the hardware.

use crate::job::{Job, JobState};

/// Picks the job that may start now, or `None` when no `queued` job can run.
#[allow(clippy::needless_lifetimes)]
pub fn next_startable<'a>(
    jobs: &'a [Job],
    running: usize,
    max_concurrent: u8,
    queue_paused: bool,
) -> Option<&'a Job> {
    if queue_paused || running >= usize::from(max_concurrent) {
        return None;
    }

    jobs.iter()
        .filter(|job| job.state == JobState::Queued)
        .min_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.id.cmp(&right.id))
        })
}
