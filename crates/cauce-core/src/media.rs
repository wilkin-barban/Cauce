//! `MediaSnapshot`: what `ffprobe` told the daemon about an input file.
//!
//! See `docs/contracts.md` §2.1. The snapshot is the only media description the
//! planner receives, so the GUI never has to parse FFprobe output itself.

use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaSnapshot {
    /// Absolute, canonicalized path.
    pub path: String,
    /// Normalized container name (`mp4`, `mkv`, `webm`, `mp3`, …).
    pub container: String,
    /// `0` when FFprobe cannot determine the duration.
    pub duration_ms: u64,
    pub size_bytes: u64,
    /// `0` when the bitrate is unknown.
    pub bitrate_bps: u64,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_streams: Vec<SubtitleStream>,
    pub probed_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStream {
    pub index: u32,
    pub codec: String,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub pixel_format: String,
    pub bit_depth: u8,
    pub is_hdr: bool,
    pub bitrate_bps: Option<u64>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStream {
    pub index: u32,
    pub codec: String,
    pub channels: u8,
    pub sample_rate: u32,
    pub bitrate_bps: Option<u64>,
    pub language: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleStream {
    pub index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub kind: SubtitleKind,
    pub is_default: bool,
    pub is_forced: bool,
}

/// Subtitle payload shape, which decides whether a container can carry it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleKind {
    Text,
    Bitmap,
}
