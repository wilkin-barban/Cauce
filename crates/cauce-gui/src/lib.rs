//! Cauce desktop application: the Libadwaita shell and its daemon client.
//!
//! The GUI is a pure client (`docs/architecture.md` §5): it never opens SQLite and
//! never spawns FFmpeg, so every decision lives in `cauce-daemon`.

pub mod app;
pub mod client;
pub mod pages;
pub mod window;
