//! Secure FFmpeg argv builder (`docs/architecture.md` §10, §15).
//!
//! The builder is pure: it returns an argument vector and never a shell string,
//! never a spawned process. The final destination is never an argument — the
//! caller passes the unique temporary path and renames it after validation.
//!
//! Nothing here is configurable from user input beyond the frozen
//! `ConversionPlan`: v0.1 has no path for arbitrary flags.

use crate::error::{ErrorCode, JobError};
use crate::plan::{
    AudioPlan, ConversionPlan, HdrAction, MetadataPolicy, StreamAction, SubtitleAction,
    SubtitlePlan, VideoPlan,
};

/// Filters the MVP depends on; the capability gate (T9) checks the same names.
pub const REQUIRED_FILTERS: [&str; 5] = ["zscale", "tonemap", "scale", "format", "fps"];

/// CPU video encoders allowed in v0.1. `copy` is not a re-encode.
const VIDEO_ENCODERS: [&str; 4] = ["libx264", "libx265", "libsvtav1", "libvpx-vp9"];
/// CPU audio encoders allowed in v0.1.
const AUDIO_ENCODERS: [&str; 6] = ["aac", "libmp3lame", "flac", "alac", "pcm_s16le", "libopus"];
/// Subtitle encoders allowed in v0.1.
const SUBTITLE_ENCODERS: [&str; 3] = ["srt", "mov_text", "webvtt"];

const BASE_ARGS: [&str; 8] = [
    "-hide_banner",
    "-nostdin",
    "-y",
    "-loglevel",
    "error",
    "-progress",
    "pipe:1",
    "-nostats",
];

/// The HDR→SDR chain emitted by the builder, exposed so the capability gate
/// checks exactly the string that FFmpeg will receive.
pub fn tonemap_filter_chain() -> &'static str {
    concat!(
        "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709",
        ",tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv",
    )
}

/// Builds the FFmpeg argument vector for `plan`.
///
/// Rejects unsafe paths and anything outside the v0.1 allowlist with
/// `PLAN_INVALID`; it never falls back to a default encoder or a shell.
pub fn build_ffmpeg_args(
    input: &str,
    temp_output: &str,
    plan: &ConversionPlan,
) -> Result<Vec<String>, JobError> {
    check_path(input, "input")?;
    check_path(temp_output, "temporary output")?;
    if input == temp_output {
        return Err(invalid(
            "the input path and the temporary output path must differ",
        ));
    }
    if plan.audio.normalize {
        return Err(invalid(
            "audio normalization is not part of the v0.1 filter allowlist; \
             refusing to ignore `normalize`",
        ));
    }

    let mut args: Vec<String> = BASE_ARGS.iter().map(|arg| (*arg).to_string()).collect();
    args.push("-i".into());
    args.push(input.to_string());

    push_video(&mut args, &plan.video)?;
    push_audio(&mut args, &plan.audio)?;
    push_subtitles(&mut args, &plan.subtitles)?;

    let chain = video_filter_chain(&plan.video);
    if !chain.is_empty() {
        args.push("-vf".into());
        args.push(chain);
    }

    if plan.metadata == MetadataPolicy::Strip {
        args.push("-map_metadata".into());
        args.push("-1".into());
    }
    if plan.faststart {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }

    args.push(temp_output.to_string());
    Ok(args)
}

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::PlanInvalid, message, false)
}

fn check_path(path: &str, role: &str) -> Result<(), JobError> {
    if path.is_empty() {
        return Err(invalid(format!("the {role} path must not be empty")));
    }
    if path.contains('\0') {
        return Err(invalid(format!("the {role} path must not contain NUL")));
    }
    if !std::path::Path::new(path).is_absolute() {
        return Err(invalid(format!(
            "the {role} path must be absolute, got `{path}`"
        )));
    }
    Ok(())
}

fn stream_index(index: Option<u32>, family: &str) -> Result<String, JobError> {
    match index {
        Some(index) => Ok(format!("0:{index}")),
        None => Err(invalid(format!(
            "a {family} stream that is kept must declare its stream_index"
        ))),
    }
}

fn check_encoder(
    encoder: Option<&str>,
    allowed: &[&str],
    family: &str,
) -> Result<String, JobError> {
    let encoder = encoder.ok_or_else(|| {
        invalid(format!(
            "a {family} transcode must declare one of the v0.1 encoders"
        ))
    })?;
    if encoder == "copy" {
        return Err(invalid(format!(
            "a {family} transcode cannot use the `copy` encoder"
        )));
    }
    if !allowed.contains(&encoder) {
        return Err(invalid(format!(
            "`{encoder}` is not an allowed {family} encoder in v0.1"
        )));
    }
    Ok(encoder.to_string())
}

fn push_video(args: &mut Vec<String>, plan: &VideoPlan) -> Result<(), JobError> {
    match plan.action {
        StreamAction::Remove => args.push("-vn".into()),
        StreamAction::Copy => {
            let index = stream_index(plan.stream_index, "video")?;
            args.extend(["-map".to_string(), index, "-c:v".to_string(), "copy".into()]);
        }
        StreamAction::Transcode => {
            let index = stream_index(plan.stream_index, "video")?;
            let encoder = check_encoder(plan.encoder.as_deref(), &VIDEO_ENCODERS, "video")?;
            args.extend(["-map".to_string(), index, "-c:v".to_string(), encoder]);
            if let Some(quality) = plan.quality {
                args.extend(["-crf".to_string(), quality.to_string()]);
            }
            if let Some(profile) = &plan.profile {
                args.extend(["-profile:v".to_string(), profile.clone()]);
            }
            if let Some(level) = plan.level {
                args.extend(["-level".to_string(), level.to_string()]);
            }
            if let Some(pixel_format) = &plan.pixel_format {
                args.push("-pix_fmt".into());
                args.push(pixel_format.clone());
            }
        }
    }
    Ok(())
}

fn push_audio(args: &mut Vec<String>, plan: &AudioPlan) -> Result<(), JobError> {
    match plan.action {
        StreamAction::Remove => args.push("-an".into()),
        StreamAction::Copy => {
            let index = stream_index(plan.stream_index, "audio")?;
            args.extend(["-map".to_string(), index, "-c:a".to_string(), "copy".into()]);
        }
        StreamAction::Transcode => {
            let index = stream_index(plan.stream_index, "audio")?;
            let encoder = check_encoder(plan.encoder.as_deref(), &AUDIO_ENCODERS, "audio")?;
            args.extend(["-map".to_string(), index, "-c:a".to_string(), encoder]);
            if let Some(bitrate) = &plan.bitrate {
                args.extend(["-b:a".to_string(), bitrate.clone()]);
            }
            if let Some(channels) = plan.channels {
                args.extend(["-ac".to_string(), channels.to_string()]);
            }
        }
    }
    Ok(())
}

fn push_subtitles(args: &mut Vec<String>, plan: &SubtitlePlan) -> Result<(), JobError> {
    match plan.action {
        SubtitleAction::Remove => args.push("-sn".into()),
        SubtitleAction::Copy => {
            let index = stream_index(plan.stream_index, "subtitle")?;
            args.extend(["-map".to_string(), index, "-c:s".to_string(), "copy".into()]);
        }
        SubtitleAction::Transcode => {
            let index = stream_index(plan.stream_index, "subtitle")?;
            let encoder = check_encoder(plan.encoder.as_deref(), &SUBTITLE_ENCODERS, "subtitle")?;
            args.extend(["-map".to_string(), index, "-c:s".to_string(), encoder]);
        }
        SubtitleAction::BurnIn => {
            return Err(invalid(
                "subtitle burn-in has no filter in the v0.1 allowlist",
            ));
        }
    }
    Ok(())
}

/// Scale → fps → tone-map, comma-joined in that fixed order.
fn video_filter_chain(plan: &VideoPlan) -> String {
    let mut pieces: Vec<String> = Vec::new();
    if let Some(scale) = scale_filter(plan) {
        pieces.push(scale);
    }
    if let Some(fps) = plan.fps {
        pieces.push(format!("fps={}", format_fps(fps)));
    }
    if plan.hdr_action == HdrAction::ToneMapSdr {
        pieces.push(tonemap_filter_chain().to_string());
    }
    pieces.join(",")
}

fn scale_filter(plan: &VideoPlan) -> Option<String> {
    let width = plan.max_width;
    let height = plan.max_height;
    if width.is_none() && height.is_none() {
        return None;
    }
    // `\,` is what FFmpeg's filter parser needs; no shell is involved here.
    let mut bounds = Vec::new();
    if let Some(width) = width {
        bounds.push(format!(r"w=min(iw\,{width})"));
    }
    if let Some(height) = height {
        bounds.push(format!(r"h=min(ih\,{height})"));
    }
    Some(format!(
        "scale={}:force_original_aspect_ratio=decrease:force_divisible_by=2",
        bounds.join(":")
    ))
}

/// Integers are formatted without a trailing `.0`: `60`, not `60.0`.
fn format_fps(fps: f64) -> String {
    if fps.fract() == 0.0 {
        format!("{}", fps as i64)
    } else {
        format!("{fps}")
    }
}
