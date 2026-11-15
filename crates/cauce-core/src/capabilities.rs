//! System capability report (`docs/contracts.md` §6, method `system.capabilities`).
//!
//! It is pure data: the `cauce-ffmpeg` locator fills it in T9 and the daemon adds
//! its own version in T13. `ffmpeg_missing` and `warnings` exist so the
//! diagnostic page can name every gap with a concrete action instead of failing
//! silently (`docs/architecture.md` §10).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    pub encoders: Vec<String>,
    pub filters: Vec<String>,
    pub cpu_cores: u32,
    pub disk_free_bytes: u64,
    /// Empty until the daemon reports itself (T13).
    pub daemon_version: String,
    /// Required encoders and filters the local FFmpeg does not expose.
    pub ffmpeg_missing: Vec<String>,
    /// One actionable Spanish line per problem found.
    pub warnings: Vec<String>,
}
