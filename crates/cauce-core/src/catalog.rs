//! The canonical system presets, embedded as versioned data.
//!
//! See `docs/presets.md`. The catalog is immutable: it is parsed once from the
//! embedded JSON and handed out by shared reference, never through I/O.

use std::sync::OnceLock;

use crate::preset::Preset;

const VIDEO_PRESETS_JSON: &str = include_str!("catalog-video.json");

static SYSTEM_PRESETS: OnceLock<Vec<Preset>> = OnceLock::new();

/// Returns the immutable system presets in catalog order.
pub fn system_presets() -> &'static [Preset] {
    SYSTEM_PRESETS
        .get_or_init(|| parse(VIDEO_PRESETS_JSON))
        .as_slice()
}

fn parse(raw: &str) -> Vec<Preset> {
    serde_json::from_str(raw).expect("embedded catalog must parse")
}
