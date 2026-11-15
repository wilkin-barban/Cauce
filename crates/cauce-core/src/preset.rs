//! `Preset`: identity, presentation and constraints of one conversion target.
//!
//! See `docs/contracts.md` §2.3 and `docs/presets.md`. The 13 system presets are
//! data seeded from the canonical catalog; this module only defines the shape.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name_es: String,
    pub name_en: String,
    pub name_pt_br: String,
    pub description_es: String,
    pub description_en: String,
    pub description_pt_br: String,
    pub category: String,
    pub device_family: DeviceFamily,
    pub container: String,
    /// `none` for audio-only presets, which remove the video stream.
    pub video_codec: String,
    pub audio_codec: String,
    /// CRF for the CPU encoders supported in v0.1.
    pub video_quality: Option<u8>,
    /// FFmpeg bitrate token such as `192k`.
    pub audio_bitrate: Option<String>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_fps: Option<f64>,
    pub pixel_format: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub max_bit_depth: Option<u8>,
    pub hdr_policy: HdrPolicy,
    pub subtitle_policy: SubtitlePolicy,
    pub faststart: bool,
    pub preserve_metadata: bool,
    /// Kept as metadata in v0.1: every preset resolves to a CPU encoder.
    pub hardware_preference: String,
    pub audio_channels: Option<u8>,
    pub normalize_audio: bool,
    pub preferred_audio_language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceFamily {
    Generic,
    Samsung,
    Lg,
    AndroidTv,
    Web,
    Archive,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HdrPolicy {
    Preserve,
    ToneMapSdr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitlePolicy {
    PreserveCompatible,
    ConvertCompatible,
    Remove,
    BurnIn,
}
