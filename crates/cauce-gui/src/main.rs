//! Cauce: a native media converter for the desktop (`docs/ui-mvp.md`).
//!
//! The binary is only the shell: a GTK4 + Libadwaita application that talks to
//! the Cauce daemon over a Unix socket. It never opens SQLite and never runs
//! FFmpeg, so closing the window leaves the queue running (`S9`).

#![deny(missing_debug_implementations)]

use adw::prelude::*;
use cauce_gui::{app, window};
use libadwaita as adw;

fn main() -> adw::glib::ExitCode {
    adw::glib::set_application_name("Cauce");
    let application = adw::Application::builder()
        .application_id(window::APPLICATION_ID)
        .build();
    app::wire(&application);
    application.run()
}
