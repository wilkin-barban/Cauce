//! Command Builder v2 (phase 1, T4 / `docs/architecture-phases-0-and-1.md` §3.4, §4.3).
//!
//! The builder has two shapes on purpose: a plan with no explicit track list
//! emits the phase 0 argv byte for byte, and a plan that names tracks emits the
//! indexed form. A single-behaviour builder cannot do both without breaking one
//! of them, and breaking 0.1.x is not on the table.

use cauce_core::command::{DIALOGUE_BOOST_FILTER, HostFilters, build_ffmpeg_args};
use cauce_core::{
    AudioFilter, AudioPlan, AudioTrackPlan, ConversionPlan, HdrAction, MetadataPolicy,
    StreamAction, SubtitleAction, SubtitlePlan, SubtitleTrackPlan, ToneMappingOption, VideoPlan,
};

const INPUT: &str = "/home/wilkin/Downloads/The.End.Of.Oak.Street.2026.mkv";
const TEMP: &str = "/home/wilkin/Downloads/.cauce-oak.part.mp4";

fn arg_of<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    let index = argv.iter().position(|arg| arg == flag)?;
    argv.get(index + 1).map(String::as_str)
}

/// Every `flag value` pair in argv, in order.
fn pairs(argv: &[String], flag: &str) -> Vec<String> {
    argv.windows(2)
        .filter(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .collect()
}

#[rustfmt::skip]
fn video() -> VideoPlan {
    VideoPlan {
        action: StreamAction::Transcode, encoder: Some("libx264".into()), quality: Some(20),
        max_width: Some(1920), max_height: Some(1080), fps: None,
        pixel_format: Some("yuv420p".into()), profile: Some("high".into()), level: None,
        hdr_action: HdrAction::ToneMapSdr, stream_index: Some(0),
        tone_mapping: Some(ToneMappingOption::Hable),
    }
}

#[rustfmt::skip]
fn legacy_audio() -> AudioPlan {
    AudioPlan {
        action: StreamAction::Transcode, encoder: Some("aac".into()), bitrate: Some("192k".into()),
        channels: Some(2), stream_index: Some(2), normalize: false,
        filter: Some(AudioFilter::StereoDialogueBoost),
    }
}

#[rustfmt::skip]
fn no_subtitles() -> SubtitlePlan {
    SubtitlePlan { action: SubtitleAction::Remove, encoder: None, stream_index: None, source_index: None }
}

#[rustfmt::skip]
fn plan(video: VideoPlan, audio: AudioPlan, subtitles: SubtitlePlan) -> ConversionPlan {
    ConversionPlan {
        container: "mp4".into(), video, audio, subtitles,
        faststart: false, metadata: MetadataPolicy::Preserve,
        reasoning: vec!["fixture plan built by hand".into()],
        audio_tracks: Vec::new(), subtitle_tracks: Vec::new(),
    }
}

fn dual_track(source: u32) -> Vec<AudioTrackPlan> {
    vec![
        AudioTrackPlan {
            source_index: source,
            output_index: 0,
            action: StreamAction::Transcode,
            encoder: Some("aac".into()),
            bitrate: Some("192k".into()),
            channels: 2,
            filter: Some(AudioFilter::StereoDialogueBoost),
            language: Some("spa".into()),
            title: Some("Español Latino (Estéreo Voz Realzada)".into()),
            is_default: true,
        },
        AudioTrackPlan {
            source_index: source,
            output_index: 1,
            action: StreamAction::Transcode,
            encoder: Some("ac3".into()),
            bitrate: Some("448k".into()),
            channels: 6,
            filter: None,
            language: Some("spa".into()),
            title: Some("Español Latino (Dolby Digital 5.1)".into()),
            is_default: false,
        },
    ]
}

// ---------------------------------------------------------------------------
// G0: a plan with no track list keeps the phase 0 argv
// ---------------------------------------------------------------------------

#[test]
fn a_plan_without_a_track_list_keeps_the_single_track_flags() {
    let conversion = plan(video(), legacy_audio(), no_subtitles());

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    assert_eq!(arg_of(&argv, "-c:a"), Some("aac"), "{argv:?}");
    assert_eq!(arg_of(&argv, "-ac"), Some("2"), "{argv:?}");
    assert_eq!(
        arg_of(&argv, "-af"),
        Some(DIALOGUE_BOOST_FILTER),
        "{argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| arg.starts_with("-c:a:")),
        "the legacy path must not index its flags: {argv:?}"
    );
}

// ---------------------------------------------------------------------------
// Dual track (S11, S18)
// ---------------------------------------------------------------------------

#[test]
fn a_dual_track_plan_emits_indexed_stream_options() {
    let mut conversion = plan(video(), legacy_audio(), no_subtitles());
    conversion.audio_tracks = dual_track(2);

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    assert_eq!(pairs(&argv, "-map"), vec!["0:0", "0:2", "0:2"], "{argv:?}");
    assert_eq!(arg_of(&argv, "-c:a:0"), Some("aac"), "{argv:?}");
    assert_eq!(arg_of(&argv, "-b:a:0"), Some("192k"), "{argv:?}");
    assert_eq!(arg_of(&argv, "-ac:0"), Some("2"), "{argv:?}");
    assert_eq!(
        arg_of(&argv, "-filter:a:0"),
        Some(DIALOGUE_BOOST_FILTER),
        "{argv:?}"
    );

    assert_eq!(arg_of(&argv, "-c:a:1"), Some("ac3"), "{argv:?}");
    assert_eq!(arg_of(&argv, "-b:a:1"), Some("448k"), "{argv:?}");
    assert_eq!(arg_of(&argv, "-ac:1"), Some("6"), "{argv:?}");
    assert_eq!(
        arg_of(&argv, "-filter:a:1"),
        None,
        "the surround companion is not filtered: {argv:?}"
    );

    assert_eq!(
        arg_of(&argv, "-disposition:a:0"),
        Some("default"),
        "{argv:?}"
    );
    assert_eq!(
        arg_of(&argv, "-disposition:a:1"),
        Some("0"),
        "the companion must not inherit the default disposition: {argv:?}"
    );
}

#[test]
fn track_metadata_names_the_language_and_the_title() {
    let mut conversion = plan(video(), legacy_audio(), no_subtitles());
    conversion.audio_tracks = dual_track(2);

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    let language = pairs(&argv, "-metadata:s:a:0");
    assert!(
        language.contains(&"language=spa".to_string()),
        "language metadata missing: {argv:?}"
    );
    let title = pairs(&argv, "-metadata:s:a:1");
    assert!(
        title.contains(&"title=Español Latino (Dolby Digital 5.1)".to_string()),
        "title metadata missing: {argv:?}"
    );
}

// ---------------------------------------------------------------------------
// Subtitles (S14, S15)
// ---------------------------------------------------------------------------

#[test]
fn indexed_subtitles_emit_type_scoped_options() {
    let mut conversion = plan(video(), legacy_audio(), no_subtitles());
    conversion.subtitle_tracks = vec![SubtitleTrackPlan {
        source_index: 7,
        output_index: Some(0),
        action: SubtitleAction::Transcode,
        encoder: Some("mov_text".into()),
        language: Some("spa".into()),
        title: Some("Español Latino".into()),
        is_default: true,
    }];

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    assert_eq!(arg_of(&argv, "-map"), Some("0:0"), "{argv:?}");
    assert!(
        pairs(&argv, "-map").contains(&"0:7".to_string()),
        "{argv:?}"
    );
    assert_eq!(arg_of(&argv, "-c:s:0"), Some("mov_text"), "{argv:?}");
    assert!(
        pairs(&argv, "-metadata:s:s:0").contains(&"language=spa".to_string()),
        "{argv:?}"
    );
}

#[test]
fn a_burned_in_track_is_not_mapped_as_a_stream() {
    let mut conversion = plan(video(), legacy_audio(), no_subtitles());
    conversion.subtitle_tracks = vec![SubtitleTrackPlan {
        source_index: 7,
        output_index: None,
        action: SubtitleAction::BurnIn,
        encoder: None,
        language: Some("spa".into()),
        title: None,
        is_default: false,
    }];

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    assert!(
        !pairs(&argv, "-map").contains(&"0:7".to_string()),
        "a burned-in track is pixels, not a stream: {argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| arg.starts_with("-c:s")),
        "no subtitle encoder applies to a burned-in track: {argv:?}"
    );
}

// ---------------------------------------------------------------------------
// Tone mapping and filter order (S16, S25)
// ---------------------------------------------------------------------------

#[test]
fn the_video_chain_uses_the_algorithm_the_plan_declares() {
    for (algorithm, expected) in [
        (ToneMappingOption::Hable, "tonemap=tonemap=hable:desat=0"),
        (ToneMappingOption::Mobius, "tonemap=tonemap=mobius:desat=0"),
        (
            ToneMappingOption::Reinhard,
            "tonemap=tonemap=reinhard:desat=0",
        ),
    ] {
        let mut video_plan = video();
        video_plan.tone_mapping = Some(algorithm);
        let conversion = plan(video_plan, legacy_audio(), no_subtitles());

        let argv =
            build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");
        let chain = arg_of(&argv, "-vf").expect("tone mapping needs a chain");

        assert!(chain.contains(expected), "`{algorithm:?}`: {chain}");
    }
}

#[test]
fn a_plan_without_an_algorithm_keeps_hable() {
    let mut video_plan = video();
    video_plan.tone_mapping = None;
    let conversion = plan(video_plan, legacy_audio(), no_subtitles());

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    assert!(
        arg_of(&argv, "-vf")
            .expect("chain")
            .contains("tonemap=hable"),
        "the default curve is the one phase 0 shipped"
    );
}

#[test]
fn a_bitmap_burn_in_uses_the_overlay_composite() {
    let mut conversion = plan(video(), legacy_audio(), no_subtitles());
    conversion.subtitle_tracks = vec![SubtitleTrackPlan {
        source_index: 8,
        output_index: None,
        action: SubtitleAction::BurnIn,
        encoder: Some("overlay".into()),
        language: Some("spa".into()),
        title: None,
        is_default: false,
    }];

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");

    let graph = arg_of(&argv, "-filter_complex").expect("a bitmap burn-in needs a composite");
    assert!(
        graph.contains("[0:s:8]overlay=eof_action=pass"),
        "the subtitle stream must be the overlay input: {graph}"
    );
    assert!(
        pairs(&argv, "-map").contains(&"[vout]".to_string()),
        "the composite output is what gets encoded: {argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| arg == "-vf"),
        "`-vf` and `-filter_complex` cannot share one output: {argv:?}"
    );
}

#[test]
fn a_bitmap_burn_in_refuses_a_video_copy() {
    let mut video_plan = video();
    video_plan.action = StreamAction::Copy;
    video_plan.encoder = None;
    let mut conversion = plan(video_plan, legacy_audio(), no_subtitles());
    conversion.subtitle_tracks = vec![SubtitleTrackPlan {
        source_index: 8,
        output_index: None,
        action: SubtitleAction::BurnIn,
        encoder: Some("overlay".into()),
        language: None,
        title: None,
        is_default: false,
    }];

    let error = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
        .expect_err("painting pixels into frames re-encodes the video");

    assert_eq!(error.code, cauce_core::ErrorCode::PlanInvalid);
}

#[test]
fn the_filter_chain_runs_scale_then_tonemap_then_subtitles() {
    let mut conversion = plan(video(), legacy_audio(), no_subtitles());
    conversion.subtitle_tracks = vec![SubtitleTrackPlan {
        source_index: 7,
        output_index: None,
        action: SubtitleAction::BurnIn,
        encoder: None,
        language: None,
        title: None,
        is_default: false,
    }];

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("must build");
    let chain = arg_of(&argv, "-vf").expect("burn-in and tone mapping share one chain");

    let scale = chain.find("scale=").expect("scale step");
    let tonemap = chain.find("tonemap=").expect("tonemap step");
    let subtitles = chain.find("subtitles=").expect("burn-in step");
    assert!(
        scale < tonemap && tonemap < subtitles,
        "burn-in must run after tone mapping so text colours survive: {chain}"
    );
}
