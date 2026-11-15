//! The Cola page: live jobs, real progress and the actions the state machine
//! allows (`docs/ui-mvp.md` §5-§7, `docs/contracts.md` §4, §6 and §7).
//!
//! The module is split like the Convertir page so each part stays small:
//!
//! - [`state`] is the decision core (phase, allowed actions, summary) with no GTK
//!   type, so every guard is tested without a display.
//! - [`view`] builds the widgets: header toggle, rows, empty and error pages.
//! - [`flows`] owns the daemon calls and the event loop; nothing polls on a timer.
//!
//! The page never starts a conversion: jobs start when the queue is active, which
//! is what the header toggle controls (`queue.pause` / `queue.resume`).

pub mod flows;
pub mod state;
pub mod view;

pub use state::{JobAction, QueuePhase, QueueState, QueueSummary, progress_label};
pub use view::page;
