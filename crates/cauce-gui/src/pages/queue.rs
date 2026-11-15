//! The Cola page: the real progress of every job, its state in Spanish and the
//! controls the state machine allows (`docs/ui-mvp.md` §5, §9, §10 and §12;
//! `docs/contracts.md` §2.4, §2.5, §4, §6 and §7).
//!
//! The module is split like the Convertir page so each part stays small:
//!
//! - [`state`] is the decision core. It has no GTK type, so the phase, the
//!   allowed actions per state, the progress merge and the footer counters are
//!   tested without a display.
//! - [`view`] builds the widgets: the header toggle, the list of rows, the
//!   indeterminate progress bar and the empty/error pages.
//! - [`flows`] holds every daemon call and every event subscription, each
//!   awaited through `glib::spawn_future_local`, so the main loop never blocks on
//!   the socket and nothing polls on a timer.
//!
//! The page never owns the truth of the queue: it redraws from `jobs.list` plus
//! the `job.updated`, `job.progress` and `queue.updated` events
//! (`docs/ui-mvp.md` §12, T17 acceptance).

pub mod state;

pub use state::{
    JobAction, PAUSED_HINT, QueuePhase, QueueState, QueueSummary, clock, interrupted_hint,
    progress_label, state_label,
};
