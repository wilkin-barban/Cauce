//! The Convertir page: inspect the inputs, preview the plan and add jobs to the
//! queue without ever starting them (`docs/ui-mvp.md` §3, §4, §9 and §10;
//! `docs/contracts.md` §6).
//!
//! The module is split so each part stays small and single-purpose:
//!
//! - [`state`] is the decision core. It has no GTK type, so the phase, the enabled
//!   button, the single queue request and the folder expansion are tested without
//!   a display.
//! - [`view`] builds the widgets (`GtkDropDown`, drop zone, lists, spinner).
//! - [`flows`] holds every daemon call and widget signal, each awaited through
//!   `glib::spawn_future_local`, so the main loop never blocks on the socket.
//!
//! Adding never starts anything: this page calls `jobs.add` and nothing else.
//! `queue.resume` and `jobs.resume` belong to the Cola page (T17).

pub mod flows;
pub mod state;
pub mod track_dialog;
pub mod view;

pub use state::{
    ConvertPhase, ConvertState, FFMPEG_COMMAND, QueueRequest, RejectedInput, base_name,
    expand_dropped_folder, plan_summary, summarize,
};
pub use view::{DROP_HINT, page};
