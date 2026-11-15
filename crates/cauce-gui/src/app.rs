//! Application wiring for the Cauce shell (`docs/architecture.md` §13).
//!
//! There is exactly one `AdwApplication` with the identity
//! `io.github.wilkin.Cauce`; activation shows the window and nothing else, so the
//! shell has no background work and no side effects on the daemon.

use adw::prelude::*;
use libadwaita as adw;

use crate::window;

/// Connects the application's signals. Called once, before `run`.
pub fn wire(application: &adw::Application) {
    application.connect_startup(|application| {
        // `Ctrl+Q` closes the window and leaves the daemon converting
        // (`docs/ui-mvp.md` §10); `S9` forbids cancelling jobs on exit.
        application.set_accels_for_action("window.close", &["<Control>q"]);
    });
    application.connect_activate(window::present);
}
