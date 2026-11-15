//! Pure Cauce domain: presets, compatibility, conversion plans and output validation.
//!
//! This crate knows nothing about GTK, sockets, SQLite or child processes.

pub mod capabilities;
pub mod catalog;
pub mod command;
pub mod compat;
pub mod error;
pub mod job;
pub mod media;
pub mod output;
pub mod plan;
pub mod preset;
pub mod probe;
pub mod settings;
pub mod time;

mod validate;

pub use capabilities::Capabilities;
pub use catalog::system_presets;
pub use command::build_ffmpeg_args;
pub use compat::plan_conversion;
pub use error::{ErrorCode, JobError};
pub use job::{Job, JobState, Progress};
pub use media::{AudioStream, MediaSnapshot, SubtitleKind, SubtitleStream, VideoStream};
pub use output::{resolve_output_path, temp_path, validate_output};
pub use plan::{
    AudioPlan, ConversionPlan, HdrAction, MetadataPolicy, StreamAction, SubtitleAction,
    SubtitlePlan, VideoPlan,
};
pub use preset::{DeviceFamily, HdrPolicy, Preset, SubtitlePolicy};
pub use probe::parse_ffprobe;
pub use settings::{DuplicatePolicy, Language, Settings, Theme};
pub use time::Timestamp;
