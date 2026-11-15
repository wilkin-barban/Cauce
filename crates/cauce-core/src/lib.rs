//! Pure Cauce domain: presets, compatibility, conversion plans and output validation.
//!
//! This crate knows nothing about GTK, sockets, SQLite or child processes.

pub mod media;
pub mod plan;
pub mod time;

pub use media::{AudioStream, MediaSnapshot, SubtitleKind, SubtitleStream, VideoStream};
pub use plan::{
    AudioPlan, ConversionPlan, HdrAction, MetadataPolicy, StreamAction, SubtitleAction,
    SubtitlePlan, VideoPlan,
};
pub use time::Timestamp;
