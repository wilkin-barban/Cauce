//! The canonical system presets, embedded as versioned data.
//!
//! See `docs/presets.md`. The catalog is immutable: it is parsed once from the
//! embedded JSON and handed out by shared reference, never through I/O. The
//! data is split by domain — the video presets first, then the audio presets.

use std::sync::OnceLock;

use crate::preset::Preset;

const VIDEO_PRESETS_JSON: &str = include_str!("catalog-video.json");
const AUDIO_PRESETS_JSON: &str = include_str!("catalog-audio.json");

static SYSTEM_PRESETS: OnceLock<Vec<Preset>> = OnceLock::new();

/// Returns the thirteen immutable system presets in catalog order.
pub fn system_presets() -> &'static [Preset] {
    SYSTEM_PRESETS
        .get_or_init(|| {
            let mut presets: Vec<Preset> = parse(VIDEO_PRESETS_JSON);
            presets.extend(parse(AUDIO_PRESETS_JSON));
            presets.shrink_to_fit();
            presets
        })
        .as_slice()
}

fn parse(raw: &str) -> Vec<Preset> {
    serde_json::from_str(raw).expect("embedded catalog must parse")
}
