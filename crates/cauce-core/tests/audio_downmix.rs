//! Audio filter chain and dialogue-boost downmix (phase 0, BUG-001 / T0.1 / T0.3).
//!
//! The builder emits the chain the plan asks for, only from the frozen allowlist
//! in `cauce_core::plan::AudioFilter`, and never runs a process
//! (`docs/architecture-phases-0-and-1.md` §2 BUG-001, §6 G1). The planner half
//! lives in `compat_video_audio.rs`.

use cauce_core::command::{HostFilters, audio_filter_chain, build_ffmpeg_args};
use cauce_core::{
    AudioFilter, AudioPlan, ConversionPlan, HdrAction, MetadataPolicy, StreamAction,
    SubtitleAction, SubtitlePlan, VideoPlan,
};

const INPUT: &str = "/home/wilkin/Vídeos/entrada.mkv";
const TEMP: &str = "/home/wilkin/Vídeos/.cauce-job-1.part.mp4";

/// The matrix of `docs/architecture-phases-0-and-1.md` §2 BUG-001, verbatim.
const DIALOGUE_BOOST: &str =
    "pan=stereo|FL=0.5*FL+0.85*FC+0.5*SL|FR=0.5*FR+0.85*FC+0.5*SR,volume=1.8,alimiter=limit=0.98";

#[rustfmt::skip]
fn video_plan() -> VideoPlan {
    VideoPlan {
        action: StreamAction::Copy, encoder: None, quality: None, max_width: None,
        max_height: None, fps: None, pixel_format: None, profile: None, level: None,
        hdr_action: HdrAction::Preserve, stream_index: Some(0),
    }
}

#[rustfmt::skip]
fn audio_plan(action: StreamAction, filter: Option<AudioFilter>, normalize: bool) -> AudioPlan {
    AudioPlan {
        action,
        encoder: (action == StreamAction::Transcode).then(|| "aac".to_string()),
        bitrate: Some("192k".into()),
        channels: (action == StreamAction::Transcode).then_some(2),
        stream_index: (action != StreamAction::Remove).then_some(1),
        normalize,
        filter,
    }
}

#[rustfmt::skip]
fn plan(audio: AudioPlan) -> ConversionPlan {
    ConversionPlan {
        container: "mp4".into(),
        video: video_plan(),
        audio,
        subtitles: SubtitlePlan {
            action: SubtitleAction::Remove, encoder: None, stream_index: None, source_index: None,
        },
        faststart: false,
        metadata: MetadataPolicy::Preserve,
        reasoning: vec!["fixture plan built by hand".into()],
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
    }
}

fn arg_of<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    let index = argv.iter().position(|arg| arg == flag)?;
    argv.get(index + 1).map(String::as_str)
}

// ---------------------------------------------------------------------------
// The builder emits the chain (BUG-001, T0.1).
// ---------------------------------------------------------------------------

#[test]
fn dialogue_boost_chain_is_the_documented_matrix() {
    let audio = audio_plan(
        StreamAction::Transcode,
        Some(AudioFilter::StereoDialogueBoost),
        false,
    );

    assert_eq!(audio_filter_chain(&audio).as_deref(), Some(DIALOGUE_BOOST));
}

#[test]
fn dialogue_boost_is_injected_as_an_audio_filter_in_the_argv() {
    let conversion = plan(audio_plan(
        StreamAction::Transcode,
        Some(AudioFilter::StereoDialogueBoost),
        false,
    ));

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    assert_eq!(
        arg_of(&argv, "-af"),
        Some(DIALOGUE_BOOST),
        "the downmix must reach FFmpeg as `-af`: {argv:?}"
    );
    assert_eq!(
        arg_of(&argv, "-ac"),
        Some("2"),
        "the target channel count stays declared: {argv:?}"
    );
}

#[test]
fn no_audio_filter_is_emitted_when_the_plan_requests_none() {
    let conversion = plan(audio_plan(StreamAction::Transcode, None, false));

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    assert_eq!(arg_of(&argv, "-af"), None, "unexpected filter: {argv:?}");
    assert_eq!(audio_filter_chain(&conversion.audio), None);
}

// ---------------------------------------------------------------------------
// `normalize` is honored instead of refused (BUG-001, T0.1).
// ---------------------------------------------------------------------------

#[test]
fn normalize_emits_loudnorm_instead_of_failing() {
    let conversion = plan(audio_plan(StreamAction::Transcode, None, true));

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
        .expect("normalize must no longer be rejected");

    let chain = arg_of(&argv, "-af").expect("a normalized transcode needs an audio filter");
    assert!(
        chain.starts_with("loudnorm="),
        "normalize must map to loudnorm, got `{chain}`"
    );
}

#[test]
fn downmix_and_normalize_compose_in_that_order() {
    let conversion = plan(audio_plan(
        StreamAction::Transcode,
        Some(AudioFilter::StereoDialogueBoost),
        true,
    ));

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    let chain = arg_of(&argv, "-af").expect("both filters need one chain");
    assert_eq!(
        chain,
        format!("{DIALOGUE_BOOST},loudnorm=I=-16:TP=-1.5:LRA=11"),
        "the pan matrix runs before the loudness pass"
    );
}

#[test]
fn normalize_is_refused_when_the_plan_copies_the_audio() {
    let conversion = plan(audio_plan(StreamAction::Copy, None, true));

    let error = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
        .expect_err("a copied stream cannot be normalized");

    assert_eq!(error.code, cauce_core::ErrorCode::PlanInvalid);
}

#[test]
fn normalize_is_refused_when_the_host_lacks_loudnorm() {
    let conversion = plan(audio_plan(StreamAction::Transcode, None, true));
    let host = HostFilters {
        zscale: true,
        loudnorm: false,
    };

    let error = build_ffmpeg_args(INPUT, TEMP, &conversion, &host)
        .expect_err("a host without loudnorm must not pretend to normalize");

    assert_eq!(error.code, cauce_core::ErrorCode::PlanInvalid);
}
