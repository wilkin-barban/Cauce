//! Selection and option contract (phase 1, T1 / `docs/architecture-phases-0-and-1.md` §3.1.1).
//!
//! These types are the shape of `track_selection` and `custom_options` in the IPC
//! payload, so every field is pinned here: a rename or a missing variant would
//! silently keep working in Rust and break the wire contract.

use cauce_core::plan::{
    AudioFilter, AudioMixMode, AudioTrackPlan, CustomConversionOptions, ResolutionTarget,
    StreamAction, SubtitleAction, SubtitleMode, SubtitleTrackPlan, ToneMappingOption,
    TrackSelection,
};

// ---------------------------------------------------------------------------
// TrackSelection
// ---------------------------------------------------------------------------

#[test]
fn track_selection_round_trips_through_golden_json() {
    let golden = r#"{
      "audio_track_indices": [2],
      "subtitle_track_index": 7,
      "subtitle_mode": "soft_embed",
      "audio_mix_mode": "dual_track_universal"
    }"#;

    let selection: TrackSelection = serde_json::from_str(golden).expect("golden must deserialize");

    assert_eq!(selection.audio_track_indices, vec![2]);
    assert_eq!(selection.subtitle_track_index, Some(7));
    assert_eq!(selection.subtitle_mode, SubtitleMode::SoftEmbed);
    assert_eq!(selection.audio_mix_mode, AudioMixMode::DualTrackUniversal);

    let encoded = serde_json::to_value(&selection).expect("must serialize");
    assert_eq!(encoded["audio_mix_mode"], "dual_track_universal");
    assert_eq!(encoded["subtitle_mode"], "soft_embed");
    assert_eq!(encoded["audio_track_indices"], serde_json::json!([2]));
}

#[test]
fn track_selection_rejects_an_unknown_audio_mix_mode() {
    let payload = r#"{
      "audio_track_indices": [1],
      "subtitle_track_index": null,
      "subtitle_mode": "strip",
      "audio_mix_mode": "quadraphonic"
    }"#;

    let error = serde_json::from_str::<TrackSelection>(payload)
        .expect_err("an unknown mix mode must be rejected");

    assert!(
        error.to_string().contains("quadraphonic"),
        "the error must name the rejected value: {error}"
    );
}

#[test]
fn subtitle_mode_has_exactly_the_three_phase_1_variants() {
    // `external_file` was dropped in phase 1: exporting a sidecar `.srt` has no
    // naming, collision or validation contract, so it is not a valid mode.
    let payload = r#"{
      "audio_track_indices": [1],
      "subtitle_track_index": 2,
      "subtitle_mode": "external_file",
      "audio_mix_mode": "passthrough"
    }"#;

    let error = serde_json::from_str::<TrackSelection>(payload)
        .expect_err("`external_file` is out of scope for phase 1");

    assert!(error.to_string().contains("external_file"), "{error}");
}

#[test]
fn track_selection_rejects_empty_audio_indices_outside_passthrough() {
    let payload = r#"{
      "audio_track_indices": [],
      "subtitle_track_index": null,
      "subtitle_mode": "strip",
      "audio_mix_mode": "stereo_dialogue_boost"
    }"#;

    let error = serde_json::from_str::<TrackSelection>(payload)
        .expect_err("a downmix with no source track has nothing to mix");

    assert!(error.to_string().contains("audio_track_indices"), "{error}");
}

#[test]
fn track_selection_accepts_empty_audio_indices_for_passthrough() {
    // Passthrough with no explicit track means "the preset decides", which is
    // exactly the 0.1.x behaviour the field is allowed to preserve.
    let payload = r#"{
      "audio_track_indices": [],
      "subtitle_track_index": null,
      "subtitle_mode": "strip",
      "audio_mix_mode": "passthrough"
    }"#;

    let selection: TrackSelection =
        serde_json::from_str(payload).expect("passthrough may be empty");

    assert!(selection.audio_track_indices.is_empty());
}

// ---------------------------------------------------------------------------
// Track plans
// ---------------------------------------------------------------------------

#[test]
fn audio_track_plan_round_trips_and_closes_the_filter_set() {
    let plan = AudioTrackPlan {
        source_index: 2,
        output_index: 0,
        action: StreamAction::Transcode,
        encoder: Some("aac".into()),
        bitrate: Some("192k".into()),
        channels: 2,
        filter: Some(AudioFilter::StereoDialogueBoost),
        language: Some("spa".into()),
        title: Some("Español Latino (Estéreo Voz Realzada)".into()),
        is_default: true,
    };

    let encoded = serde_json::to_string(&plan).expect("must serialize");
    let decoded: AudioTrackPlan = serde_json::from_str(&encoded).expect("must deserialize");

    assert_eq!(decoded, plan);
    assert_eq!(decoded.filter, Some(AudioFilter::StereoDialogueBoost));
}

#[test]
fn subtitle_track_plan_carries_no_stream_when_burned_in() {
    let burned = SubtitleTrackPlan {
        source_index: 7,
        output_index: None,
        action: SubtitleAction::BurnIn,
        encoder: None,
        language: Some("spa".into()),
        title: None,
        is_default: false,
    };

    let encoded = serde_json::to_string(&burned).expect("must serialize");
    let decoded: SubtitleTrackPlan = serde_json::from_str(&encoded).expect("must deserialize");

    assert_eq!(decoded, burned);
    assert_eq!(
        decoded.output_index, None,
        "a burned-in track is pixels, not a stream"
    );
}

// ---------------------------------------------------------------------------
// CustomConversionOptions
// ---------------------------------------------------------------------------

#[test]
fn custom_options_deserialize_from_the_minimal_ipc_payload() {
    // §3.5 sends only two fields; everything else has to default.
    let payload = r#"{"resolution": "1080p", "crf": 20}"#;

    let options: CustomConversionOptions =
        serde_json::from_str(payload).expect("a partial payload must be legal");

    assert_eq!(options.resolution, ResolutionTarget::MaxHeight(1080));
    assert_eq!(options.crf, Some(20));
    assert_eq!(options.video_bitrate_kbps, None);
    assert_eq!(options.video_codec, None);
    assert_eq!(options.audio_codec, None);
    assert_eq!(options.audio_bitrate_kbps, None);
    assert_eq!(options.tone_mapping, ToneMappingOption::Hable);
}

#[test]
fn custom_options_round_trip_through_golden_json() {
    let options = CustomConversionOptions {
        resolution: ResolutionTarget::Fixed {
            width: 1920,
            height: 1080,
        },
        crf: Some(20),
        video_bitrate_kbps: None,
        video_codec: Some("libx264".into()),
        audio_codec: Some("aac".into()),
        audio_bitrate_kbps: Some(192),
        tone_mapping: ToneMappingOption::Reinhard,
    };

    let encoded = serde_json::to_value(&options).expect("must serialize");
    assert_eq!(
        encoded["resolution"],
        serde_json::json!({"fixed": {"width": 1920, "height": 1080}})
    );
    assert_eq!(encoded["tone_mapping"], "reinhard");

    let decoded: CustomConversionOptions =
        serde_json::from_value(encoded).expect("must deserialize");
    assert_eq!(decoded, options);
}

#[test]
fn resolution_target_rejects_a_zero_dimension() {
    for payload in [
        r#"{"resolution": {"fixed": {"width": 0, "height": 1080}}}"#,
        r#"{"resolution": {"fixed": {"width": 1920, "height": 0}}}"#,
        r#"{"resolution": {"max_height": 0}}"#,
    ] {
        let error = serde_json::from_str::<CustomConversionOptions>(payload)
            .expect_err("a zero dimension is never a legal target");
        assert!(
            error.to_string().contains("greater than zero"),
            "the error must explain the rule for `{payload}`: {error}"
        );
    }
}

#[test]
fn custom_options_reject_a_crf_outside_the_encoder_range() {
    let error = serde_json::from_str::<CustomConversionOptions>(r#"{"crf": 52}"#)
        .expect_err("CRF 52 is outside every CPU encoder in Cauce");

    assert!(error.to_string().contains("crf"), "{error}");
}

#[test]
fn tone_mapping_option_offers_the_three_documented_algorithms() {
    for (encoded, expected) in [
        ("hable", ToneMappingOption::Hable),
        ("mobius", ToneMappingOption::Mobius),
        ("reinhard", ToneMappingOption::Reinhard),
    ] {
        let payload = format!(r#"{{"tone_mapping": "{encoded}"}}"#);
        let options: CustomConversionOptions =
            serde_json::from_str(&payload).expect("every documented algorithm must parse");
        assert_eq!(options.tone_mapping, expected);
    }
}
