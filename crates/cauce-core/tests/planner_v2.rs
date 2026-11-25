//! Planner v2 (phase 1, T6): `plan_conversion_with` wires an explicit selection
//! and custom options into the plan (`docs/architecture-phases-0-and-1.md` §3.1).
//!
//! The contract of the old planner still holds: without a selection the result
//! is the plan phase 1 already produced, byte for byte.

use cauce_core::compat::{plan_conversion, plan_conversion_with};
use cauce_core::{
    AudioFilter, AudioMixMode, AudioStream, ConversionPlan, CustomConversionOptions, ErrorCode,
    MediaSnapshot, Preset, ResolutionTarget, StreamAction, SubtitleAction, SubtitleKind,
    SubtitleMode, SubtitleStream, Timestamp, ToneMappingOption, TrackSelection, VideoStream,
    system_presets,
};

fn probed_at() -> Timestamp {
    Timestamp::parse("2026-11-24T10:00:00Z").expect("fixture timestamp must be valid UTC")
}

#[rustfmt::skip]
fn video() -> VideoStream {
    VideoStream {
        index: 0, codec: "hevc".into(), profile: None, level: None,
        width: 3840, height: 2160, fps: 23.976, pixel_format: "yuv420p10le".into(),
        bit_depth: 10, is_hdr: true, bitrate_bps: None, language: None,
    }
}

#[rustfmt::skip]
fn audio(index: u32, channels: u8, language: &str, is_default: bool) -> AudioStream {
    AudioStream {
        index, codec: "eac3".into(), channels, sample_rate: 48_000,
        bitrate_bps: Some(640_000), language: Some(language.into()), is_default,
    }
}

#[rustfmt::skip]
fn subtitle(index: u32, codec: &str, kind: SubtitleKind, language: &str) -> SubtitleStream {
    SubtitleStream {
        index, codec: codec.into(), language: Some(language.into()),
        kind, is_default: true, is_forced: false,
    }
}

/// *The End of Oak Street*: six surround tracks, English first, Spanish second.
fn oak_street() -> MediaSnapshot {
    MediaSnapshot {
        path: "/home/wilkin/Downloads/oak.mkv".into(),
        container: "mkv".into(),
        duration_ms: 6_135_000,
        size_bytes: 22_600_000_000,
        bitrate_bps: 29_000_000,
        video_streams: vec![video()],
        audio_streams: vec![
            audio(1, 6, "eng", true),
            audio(2, 6, "spa", false),
            audio(3, 6, "fra", false),
        ],
        subtitle_streams: vec![
            subtitle(4, "subrip", SubtitleKind::Text, "eng"),
            subtitle(5, "hdmv_pgs_subtitle", SubtitleKind::Bitmap, "spa"),
        ],
        probed_at: probed_at(),
    }
}

fn stereo_source() -> MediaSnapshot {
    let mut snapshot = oak_street();
    snapshot.audio_streams = vec![audio(1, 2, "spa", true)];
    snapshot
}

fn preset(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("`{id}` must exist in the catalog"))
}

fn selection(mix: AudioMixMode, audio_indices: Vec<u32>) -> TrackSelection {
    TrackSelection {
        audio_track_indices: audio_indices,
        subtitle_track_index: None,
        subtitle_mode: SubtitleMode::Strip,
        audio_mix_mode: mix,
    }
}

// ---------------------------------------------------------------------------
// G0: no selection means the plan phase 1 already produced
// ---------------------------------------------------------------------------

#[test]
fn without_a_selection_the_plan_is_the_one_phase_1_already_built() {
    for id in [
        "system-smarttv-compat",
        "system-smarttv-hevc",
        "system-archival-master",
        "system-audio-mp3",
    ] {
        let input = oak_street();
        let expected = plan_conversion(&input, preset(id)).expect("legacy plan");
        let actual = plan_conversion_with(&input, preset(id), None, None).expect("equivalent plan");

        assert_eq!(
            actual, expected,
            "`{id}` must not change without a selection"
        );
    }
}

// ---------------------------------------------------------------------------
// Audio selection and mix modes
// ---------------------------------------------------------------------------

#[test]
fn an_explicit_audio_index_wins_over_the_preset_language() {
    let input = oak_street();
    let mut chosen = selection(AudioMixMode::Passthrough, vec![3]);

    let plan = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("must plan");

    assert_eq!(
        plan.audio.stream_index,
        Some(3),
        "the French track was chosen"
    );

    chosen.audio_track_indices = vec![2];
    let plan = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("must plan");
    assert_eq!(
        plan.audio.stream_index,
        Some(2),
        "the Spanish track was chosen"
    );
}

#[test]
fn stereo_dialogue_boost_produces_one_indexed_track() {
    let input = oak_street();
    let chosen = selection(AudioMixMode::StereoDialogueBoost, vec![2]);

    let plan = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("must plan");

    assert_eq!(plan.audio_tracks.len(), 1);
    let track = &plan.audio_tracks[0];
    assert_eq!(track.source_index, 2);
    assert_eq!(track.channels, 2);
    assert_eq!(track.filter, Some(AudioFilter::StereoDialogueBoost));
    assert_eq!(track.encoder.as_deref(), Some("aac"));
    assert_eq!(track.language.as_deref(), Some("spa"));
    assert!(track.is_default);
}

#[test]
fn dual_track_produces_two_tracks_for_a_surround_source() {
    let input = oak_street();
    let chosen = selection(AudioMixMode::DualTrackUniversal, vec![2]);

    let plan = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("must plan");

    assert_eq!(plan.audio_tracks.len(), 2);
    assert_eq!(plan.audio_tracks[0].encoder.as_deref(), Some("aac"));
    assert_eq!(plan.audio_tracks[1].encoder.as_deref(), Some("ac3"));
    assert_eq!(plan.audio_tracks[1].channels, 6);
    assert!(plan.audio_tracks[0].is_default);
    assert!(!plan.audio_tracks[1].is_default);
}

#[test]
fn dual_track_degrades_to_stereo_for_a_stereo_source() {
    let input = stereo_source();
    let chosen = selection(AudioMixMode::DualTrackUniversal, vec![1]);

    let plan = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("a stereo source cannot become 5.1, but it still converts");

    assert_eq!(
        plan.audio_tracks.len(),
        1,
        "inventing a surround companion out of two channels is a lie"
    );
    assert!(
        plan.reasoning
            .iter()
            .any(|line| line.contains("estéreo") && line.contains("5.1")),
        "the degradation must be explained: {:?}",
        plan.reasoning
    );
}

#[test]
fn an_audio_index_that_does_not_exist_is_rejected() {
    let input = oak_street();
    let chosen = selection(AudioMixMode::StereoDialogueBoost, vec![99]);

    let error = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect_err("there is no stream 99");

    assert_eq!(error.code, ErrorCode::PlanInvalid);
    assert!(error.message.contains("99"), "{}", error.message);
}

// ---------------------------------------------------------------------------
// Custom options
// ---------------------------------------------------------------------------

#[test]
fn custom_resolution_and_crf_override_the_preset() {
    let input = oak_street();
    let options = CustomConversionOptions {
        resolution: ResolutionTarget::MaxHeight(720),
        crf: Some(18),
        ..CustomConversionOptions::default()
    };

    let plan = plan_conversion_with(
        &input,
        preset("system-smarttv-compat"),
        None,
        Some(&options),
    )
    .expect("must plan");

    assert_eq!(plan.video.max_height, Some(720));
    assert_eq!(plan.video.quality, Some(18));
}

#[test]
fn the_tone_mapping_override_reaches_the_plan() {
    let input = oak_street();
    let options = CustomConversionOptions {
        tone_mapping: ToneMappingOption::Reinhard,
        ..CustomConversionOptions::default()
    };

    let plan = plan_conversion_with(
        &input,
        preset("system-smarttv-compat"),
        None,
        Some(&options),
    )
    .expect("must plan");

    assert_eq!(plan.video.hdr_action, cauce_core::HdrAction::ToneMapSdr);
    assert_eq!(plan.video.tone_mapping, Some(ToneMappingOption::Reinhard));
}

// ---------------------------------------------------------------------------
// Subtitles
// ---------------------------------------------------------------------------

#[test]
fn the_subtitle_mode_and_index_drive_the_track_list() {
    let input = oak_street();
    let mut chosen = selection(AudioMixMode::Passthrough, vec![]);
    chosen.subtitle_track_index = Some(5);
    chosen.subtitle_mode = SubtitleMode::BurnIn;

    let plan = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("PGS into MP4 is legal when burned in");

    assert_eq!(plan.subtitle_tracks.len(), 1);
    assert_eq!(plan.subtitle_tracks[0].action, SubtitleAction::BurnIn);
    assert_eq!(plan.subtitle_tracks[0].encoder.as_deref(), Some("overlay"));
    assert_eq!(
        plan.subtitles.action,
        SubtitleAction::BurnIn,
        "the legacy view must agree with the track list"
    );
}

#[test]
fn a_stripped_selection_leaves_no_subtitle_tracks() {
    let input = oak_street();
    let chosen = selection(AudioMixMode::Passthrough, vec![]);

    let plan = plan_conversion_with(&input, preset("system-smarttv-hevc"), Some(&chosen), None)
        .expect("must plan");

    assert!(plan.subtitle_tracks.is_empty());
    assert_eq!(plan.subtitles.action, SubtitleAction::Remove);
}

#[test]
fn the_plan_is_reproducible_with_a_selection() {
    let input = oak_street();
    let chosen = selection(AudioMixMode::DualTrackUniversal, vec![2]);

    let first = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("must plan");
    let second = plan_conversion_with(&input, preset("system-smarttv-compat"), Some(&chosen), None)
        .expect("must plan");

    assert_eq!(first, second);
    assert!(!first.reasoning.is_empty());
}

/// The plan type is part of the persisted contract: the reference is here so a
/// field rename fails this file and not a downstream crate.
#[allow(dead_code)]
fn assert_plan_shape(plan: &ConversionPlan) {
    let _ = (&plan.container, &plan.video, &plan.audio, &plan.subtitles);
    let _ = (&plan.audio_tracks, &plan.subtitle_tracks);
    let _ = plan.video.action == StreamAction::Transcode;
}
