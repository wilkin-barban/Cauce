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
    AudioFilter, AudioPlan, AudioTrackPlan, ConversionPlan, HdrAction, MetadataPolicy,
    StreamAction, SubtitleAction, SubtitlePlan, ToneMappingOption, VideoPlan,
};

/// Filters the MVP depends on; the capability gate (T9) checks the same names.
pub const REQUIRED_FILTERS: [&str; 4] = ["tonemap", "scale", "format", "fps"];

/// CPU encoders the thirteen presets select (`docs/presets.md`, "Resolución en
/// v0.1"): the union of the video and audio allowlists below, so the capability
/// gate checks exactly what the builder is able to emit.
pub const REQUIRED_ENCODERS: [&str; 11] = [
    "libx264",
    "libx265",
    "libsvtav1",
    "libvpx-vp9",
    "aac",
    "ac3",
    "libmp3lame",
    "flac",
    "alac",
    "pcm_s16le",
    "libopus",
];

/// CPU video encoders allowed in v0.1. `copy` is not a re-encode.
const VIDEO_ENCODERS: [&str; 4] = ["libx264", "libx265", "libsvtav1", "libvpx-vp9"];
/// CPU audio encoders allowed in v0.1.
const AUDIO_ENCODERS: [&str; 7] = [
    "aac",
    "ac3",
    "libmp3lame",
    "flac",
    "alac",
    "pcm_s16le",
    "libopus",
];
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

/// The matrix that folds a >= 6 channel source into stereo while lifting the
/// front centre, where film dialogue lives (`docs/architecture-phases-0-and-1.md`
/// §2 BUG-001). `volume=1.8` compensates the headroom of a cinema mix and
/// `alimiter=limit=0.98` keeps an explosion from clipping on the way out (§3.2).
///
/// `alimiter` ships with every libavfilter build, so it needs no capability gate
/// of its own the way `zscale` does.
pub const DIALOGUE_BOOST_FILTER: &str =
    "pan=stereo|FL=0.5*FL+0.85*FC+0.5*SL|FR=0.5*FR+0.85*FC+0.5*SR,volume=1.8,alimiter=limit=0.98";

/// Single-pass EBU R128 target that `normalize` maps to. One pass keeps the
/// conversion a single FFmpeg run; the two-pass variant needs a measurement
/// pass the scheduler does not have.
pub const LOUDNORM_FILTER: &str = "loudnorm=I=-16:TP=-1.5:LRA=11";

/// The optional filters this host can actually run.
///
/// `cauce-core` never spawns a process, so `cauce-ffmpeg` derives this report
/// from `Capabilities.filters` once and hands it to the builder. A filter the
/// host lacks is never emitted (`docs/architecture-phases-0-and-1.md` §2
/// BUG-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostFilters {
    pub zscale: bool,
    pub loudnorm: bool,
}

impl HostFilters {
    /// Every optional filter available: the assumption of a stock FFmpeg build.
    pub const ALL: Self = Self {
        zscale: true,
        loudnorm: true,
    };

    /// Reads the report from the names `ffmpeg -filters` prints.
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Self {
        Self {
            zscale: names.iter().any(|name| name.as_ref() == "zscale"),
            loudnorm: names.iter().any(|name| name.as_ref() == "loudnorm"),
        }
    }
}

/// The HDR→SDR chain emitted by the builder, exposed so the capability gate
/// checks exactly the string that FFmpeg will receive.
pub fn tonemap_filter_chain() -> &'static str {
    concat!(
        "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709",
        ",tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv",
    )
}

/// The native HDR→SDR fallback chain used when `zscale` is absent from the host.
pub fn native_tonemap_filter_chain() -> &'static str {
    "tonemap=mobius,format=yuv420p"
}

/// Builds the FFmpeg argument vector for `plan`.
///
/// Rejects unsafe paths and anything outside the v0.1 allowlist with
/// `PLAN_INVALID`; it never falls back to a default encoder or a shell.
/// `filters` reports the optional filters this host can run.
pub fn build_ffmpeg_args(
    input: &str,
    temp_output: &str,
    plan: &ConversionPlan,
    filters: &HostFilters,
) -> Result<Vec<String>, JobError> {
    check_path(input, "input")?;
    check_path(temp_output, "temporary output")?;
    if input == temp_output {
        return Err(invalid(
            "the input path and the temporary output path must differ",
        ));
    }
    check_audio_filters(plan, filters)?;

    let mut args: Vec<String> = BASE_ARGS.iter().map(|arg| (*arg).to_string()).collect();
    args.push("-i".into());
    args.push(input.to_string());

    push_video(&mut args, &plan.video, filters)?;
    push_audio(&mut args, &plan.audio)?;
    push_subtitles(&mut args, &plan.subtitles)?;

    let chain = video_filter_chain(&plan.video, filters);
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

/// Refuses a filter request the plan or the host cannot honor, so a flag is
/// never silently dropped (`docs/architecture-phases-0-and-1.md` §2 BUG-001).
fn check_audio_filters(plan: &ConversionPlan, filters: &HostFilters) -> Result<(), JobError> {
    let audio = &plan.audio;
    if audio.filter.is_some() && audio.action != StreamAction::Transcode {
        return Err(invalid(
            "an audio filter needs an audio transcode; a copied or removed stream cannot be filtered",
        ));
    }
    if !audio.normalize {
        return Ok(());
    }
    if audio.action != StreamAction::Transcode {
        return Err(invalid(
            "`normalize` needs an audio transcode; a copied or removed stream cannot be filtered",
        ));
    }
    if !filters.loudnorm {
        return Err(invalid(
            "`normalize` requires the `loudnorm` filter, and this FFmpeg build does not provide it",
        ));
    }
    Ok(())
}

/// The audio filter chain of `plan`, comma-joined in the order it must run.
///
/// `None` when the plan asks for nothing: the builder then emits no `-af` at
/// all, which is what every plan written before phase 0 expects.
pub fn audio_filter_chain(plan: &AudioPlan) -> Option<String> {
    let mut pieces: Vec<&'static str> = Vec::new();
    if plan.filter == Some(AudioFilter::StereoDialogueBoost) {
        pieces.push(DIALOGUE_BOOST_FILTER);
    }
    if plan.normalize {
        pieces.push(LOUDNORM_FILTER);
    }
    (!pieces.is_empty()).then(|| pieces.join(","))
}

/// The two tracks of [`crate::plan::AudioMixMode::DualTrackUniversal`] (§3.2).
///
/// Track 0 is the boosted stereo the user hears by default; track 1 is a plain
/// 5.1 AC-3 re-encode for home cinema, with no filter because it keeps the
/// source layout. Titles are left empty: they are presentation and belong to the
/// planner, which knows the locale.
pub fn dual_track_plans(source_index: u32, language: Option<&str>) -> Vec<AudioTrackPlan> {
    let language = language.map(str::to_owned);
    vec![
        AudioTrackPlan {
            source_index,
            output_index: 0,
            action: StreamAction::Transcode,
            encoder: Some("aac".to_owned()),
            bitrate: Some("192k".to_owned()),
            channels: 2,
            filter: Some(AudioFilter::StereoDialogueBoost),
            language: language.clone(),
            title: None,
            is_default: true,
        },
        AudioTrackPlan {
            source_index,
            output_index: 1,
            action: StreamAction::Transcode,
            encoder: Some("ac3".to_owned()),
            bitrate: Some("448k".to_owned()),
            channels: 6,
            filter: None,
            language,
            title: None,
            is_default: false,
        },
    ]
}

/// The HDR→SDR chain for `algorithm` on a host that can run `zscale` or not.
///
/// With `zscale` the linearisation round trip is kept and only the curve changes;
/// without it the native `tonemap` filter does the whole job, which is the
/// fallback phase 0 stabilized for `mobius` and now extends to every algorithm.
pub fn tonemap_chain(algorithm: ToneMappingOption, host: &HostFilters) -> String {
    if !host.zscale {
        return format!("tonemap={},format=yuv420p", algorithm.ffmpeg_name());
    }
    format!(
        "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,\
         tonemap=tonemap={}:desat=0,zscale=t=bt709:m=bt709:r=tv",
        algorithm.ffmpeg_name()
    )
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

fn push_video(
    args: &mut Vec<String>,
    plan: &VideoPlan,
    _filters: &HostFilters,
) -> Result<(), JobError> {
    match plan.action {
        StreamAction::Remove => args.push("-vn".into()),
        StreamAction::Copy => {
            let index = stream_index(plan.stream_index, "video")?;
            args.extend(["-map".to_string(), index, "-c:v".to_string(), "copy".into()]);
        }
        StreamAction::Transcode => {
            let index = stream_index(plan.stream_index, "video")?;
            let encoder = check_encoder(plan.encoder.as_deref(), &VIDEO_ENCODERS, "video")?;
            args.extend([
                "-map".to_string(),
                index,
                "-c:v".to_string(),
                encoder.clone(),
            ]);
            if let Some(quality) = plan.quality {
                args.extend(["-crf".to_string(), quality.to_string()]);
            }
            if let Some(profile) = &plan.profile {
                args.extend(["-profile:v".to_string(), profile.clone()]);
            }
            if let Some(level) = plan.level {
                push_level(args, &encoder, level)?;
            }
            if let Some(pixel_format) = &plan.pixel_format {
                args.push("-pix_fmt".into());
                args.push(pixel_format.clone());
            }
        }
    }
    Ok(())
}

/// Emits the video level in the notation each encoder actually honors.
///
/// FFmpeg 8.0.1 exposes `-level` as an Annex A string for `libx264` and as a
/// float for `libvpx-vp9`; `41` is rejected by libvpx ("out of range"). The
/// libx265 wrapper has no level option (the level travels through
/// `-x265-params level-idc=`), and libsvtav1 ignores `-level` silently, so a
/// declared level is refused instead of quietly dropped.
fn push_level(args: &mut Vec<String>, encoder: &str, level_idc: i32) -> Result<(), JobError> {
    match encoder {
        "libx264" | "libvpx-vp9" => {
            let annex_a = annex_a_level(level_idc)?;
            args.extend(["-level".to_string(), annex_a.to_owned()]);
        }
        "libx265" => {
            if annex_a_level(level_idc).is_err() {
                return Err(annex_a_level(level_idc).unwrap_err());
            }
            args.extend(["-x265-params".to_string(), format!("level-idc={level_idc}")]);
        }
        other => {
            return Err(JobError::new(
                ErrorCode::PlanInvalid,
                format!("El encoder `{other}` no acepta niveles de vídeo en Cauce v0.1."),
                false,
            ));
        }
    }
    Ok(())
}

/// Converts an H.264/HEVC `level_idc` (41) into Annex A notation ("4.1").
fn annex_a_level(level_idc: i32) -> Result<&'static str, JobError> {
    let level = match level_idc {
        10 => "1",
        11 => "1.1",
        12 => "1.2",
        13 => "1.3",
        20 => "2",
        21 => "2.1",
        22 => "2.2",
        30 => "3",
        31 => "3.1",
        32 => "3.2",
        40 => "4",
        41 => "4.1",
        42 => "4.2",
        50 => "5",
        51 => "5.1",
        52 => "5.2",
        60 => "6",
        61 => "6.1",
        62 => "6.2",
        other => {
            return Err(JobError::new(
                ErrorCode::PlanInvalid,
                format!("El nivel de vídeo `{other}` no es un level_idc válido."),
                false,
            ));
        }
    };
    Ok(level)
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
            if let Some(chain) = audio_filter_chain(plan) {
                args.extend(["-af".to_string(), chain]);
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
fn video_filter_chain(plan: &VideoPlan, filters: &HostFilters) -> String {
    let mut pieces: Vec<String> = Vec::new();
    if let Some(scale) = scale_filter(plan) {
        pieces.push(scale);
    }
    if let Some(fps) = plan.fps {
        pieces.push(format!("fps={}", format_fps(fps)));
    }
    if plan.hdr_action == HdrAction::ToneMapSdr {
        if filters.zscale {
            pieces.push(tonemap_filter_chain().to_string());
        } else {
            pieces.push(native_tonemap_filter_chain().to_string());
        }
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
