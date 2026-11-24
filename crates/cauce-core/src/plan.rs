//! `ConversionPlan`: the frozen decision of what to copy and what to re-encode.
//!
//! See `docs/contracts.md` §2.2. A plan is computed once, stored on the job and
//! replayed verbatim, so every field here is part of the persisted contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionPlan {
    pub container: String,
    pub video: VideoPlan,
    pub audio: AudioPlan,
    pub subtitles: SubtitlePlan,
    /// Only meaningful for MP4 and M4A destinations.
    pub faststart: bool,
    pub metadata: MetadataPolicy,
    /// Human-readable explanation of every copy/transcode decision.
    pub reasoning: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoPlan {
    pub action: StreamAction,
    pub encoder: Option<String>,
    /// CRF for the CPU encoders supported in v0.1.
    pub quality: Option<u8>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub fps: Option<f64>,
    pub pixel_format: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub hdr_action: HdrAction,
    pub stream_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioPlan {
    pub action: StreamAction,
    pub encoder: Option<String>,
    /// FFmpeg bitrate token such as `192k`.
    pub bitrate: Option<String>,
    pub channels: Option<u8>,
    pub stream_index: Option<u32>,
    pub normalize: bool,
    /// Audio filter the builder is allowed to emit, if any.
    ///
    /// Absent from every plan written before phase 0, and skipped when absent,
    /// so a persisted v0.1 plan still round-trips byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<AudioFilter>,
}

/// The audio filters the builder may emit.
///
/// v0.1 refused every audio filter, which left no way to fix a centre channel
/// lost in a downmix. Phase 0 opens exactly this enum — never free-form filter
/// text — so the allowlist stays closed (`docs/architecture-phases-0-and-1.md`
/// §2 BUG-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFilter {
    /// Fold a >= 6 channel source into stereo with a front-centre boost, so the
    /// dialogue stays intelligible on laptops, phones and headphones.
    StereoDialogueBoost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitlePlan {
    pub action: SubtitleAction,
    pub encoder: Option<String>,
    pub stream_index: Option<u32>,
    pub source_index: Option<u32>,
}

/// Action applied to one stream family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamAction {
    Copy,
    Transcode,
    Remove,
}

/// Subtitle action, including the burn-in case that v0.1 does not implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleAction {
    Copy,
    Transcode,
    Remove,
    BurnIn,
}

/// How HDR content is handled when the target is not HDR capable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HdrAction {
    Preserve,
    ToneMapSdr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataPolicy {
    Preserve,
    Strip,
}
