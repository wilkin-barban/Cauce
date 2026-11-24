//! Track selection by language and disposition (phase 0, BUG-002 / T0.2).
//!
//! v0.1 took `audio_streams.first()` and `subtitle_streams.first()` blindly, so a
//! Spanish-speaking user got the original English track of an international
//! release and every other language was dropped without a word. The planner now
//! resolves the documented priority order: preferred language, then the default
//! disposition, then the first track.

use cauce_core::compat::plan_conversion;
use cauce_core::{
    AudioStream, MediaSnapshot, Preset, StreamAction, SubtitleAction, SubtitleKind, SubtitleStream,
    Timestamp, VideoStream, system_presets,
};

fn probed_at() -> Timestamp {
    Timestamp::parse("2026-11-23T10:00:00Z").expect("fixture timestamp must be valid UTC")
}

#[rustfmt::skip]
fn video() -> VideoStream {
    VideoStream {
        index: 0, codec: "h264".into(), profile: None, level: None,
        width: 1920, height: 1080, fps: 24.0, pixel_format: "yuv420p".into(),
        bit_depth: 8, is_hdr: false, bitrate_bps: None, language: None,
    }
}

#[rustfmt::skip]
fn audio(index: u32, channels: u8, language: Option<&str>, is_default: bool) -> AudioStream {
    AudioStream {
        index, codec: "aac".into(), channels, sample_rate: 48_000,
        bitrate_bps: Some(192_000), language: language.map(Into::into), is_default,
    }
}

#[rustfmt::skip]
fn subtitle(index: u32, language: Option<&str>, is_default: bool) -> SubtitleStream {
    SubtitleStream {
        index, codec: "subrip".into(), language: language.map(Into::into),
        kind: SubtitleKind::Text, is_default, is_forced: false,
    }
}

fn snapshot(audio_streams: Vec<AudioStream>, subtitle_streams: Vec<SubtitleStream>) -> MediaSnapshot {
    MediaSnapshot {
        path: "/home/wilkin/Vídeos/entrada.mkv".into(),
        container: "mkv".into(),
        duration_ms: 90_000,
        size_bytes: 4_000_000,
        bitrate_bps: 8_000_000,
        video_streams: vec![video()],
        audio_streams,
        subtitle_streams,
        probed_at: probed_at(),
    }
}

fn preset(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("`{id}` must exist in the catalog"))
}

/// The catalog ships every preset without a language preference, so the fixture
/// turns one into the "Spanish user" case the bug report describes. The field is
/// public and persisted, exactly like `preset.rs` declares it.
fn preset_with_language(id: &str, language: &str) -> Preset {
    let mut preset = preset(id).clone();
    preset.preferred_audio_language = Some(language.to_owned());
    preset
}

/// The benchmark shape of *The End of Oak Street*: the original English track
/// first, Spanish second, and a French track that nobody asked for.
fn multilingual() -> MediaSnapshot {
    snapshot(
        vec![
            audio(1, 6, Some("eng"), true),
            audio(2, 6, Some("spa"), false),
            audio(3, 6, Some("fra"), false),
        ],
        vec![
            subtitle(4, Some("eng"), true),
            subtitle(5, Some("spa"), false),
        ],
    )
}

// ---------------------------------------------------------------------------
// Audio (T0.2 acceptance: the requested language wins over the first track).
// ---------------------------------------------------------------------------

#[test]
fn audio_selection_prefers_the_preset_language_over_the_first_track() {
    let input = multilingual();
    let preset = preset_with_language("system-fast-compress", "es");

    let plan = plan_conversion(&input, &preset).expect("must plan");

    assert_eq!(
        plan.audio.stream_index,
        Some(2),
        "the Spanish track must win over the English one at index 1"
    );
    assert_eq!(plan.audio.action, StreamAction::Transcode);
}

#[test]
fn audio_selection_matches_alpha2_alpha3_and_region_aliases() {
    for (preferred, tag) in [
        ("es", "spa"),
        ("spa", "es"),
        ("es-419", "spa"),
        ("es", "es-419"),
        ("eng", "en"),
    ] {
        let input = snapshot(
            vec![audio(1, 2, Some("und"), false), audio(2, 2, Some(tag), true)],
            Vec::new(),
        );
        let preset = preset_with_language("system-fast-compress", preferred);

        let plan = plan_conversion(&input, &preset).expect("must plan");

        assert_eq!(
            plan.audio.stream_index,
            Some(2),
            "`{preferred}` must match a track tagged `{tag}`"
        );
    }
}

#[test]
fn audio_selection_falls_back_to_the_default_disposition() {
    let input = snapshot(
        vec![
            audio(1, 2, Some("fra"), false),
            audio(2, 2, Some("deu"), true),
        ],
        Vec::new(),
    );
    let preset = preset_with_language("system-fast-compress", "es");

    let plan = plan_conversion(&input, &preset).expect("must plan");

    assert_eq!(
        plan.audio.stream_index,
        Some(2),
        "with no language match the default disposition decides"
    );
}

#[test]
fn audio_selection_falls_back_to_the_first_track() {
    let input = snapshot(
        vec![audio(1, 2, Some("fra"), false), audio(2, 2, Some("deu"), false)],
        Vec::new(),
    );
    let preset = preset_with_language("system-fast-compress", "es");

    let plan = plan_conversion(&input, &preset).expect("must plan");

    assert_eq!(
        plan.audio.stream_index,
        Some(1),
        "with neither a match nor a default, the first track is the last resort"
    );
}

#[test]
fn audio_selection_is_stable_without_a_preferred_language() {
    let input = multilingual();

    let plan = plan_conversion(&input, preset("system-fast-compress")).expect("must plan");

    assert_eq!(
        plan.audio.stream_index,
        Some(1),
        "a preset without a preference keeps v0.1 behaviour"
    );
}

#[test]
fn audio_reasoning_reports_index_language_and_channels() {
    let input = multilingual();
    let preset = preset_with_language("system-fast-compress", "es");

    let plan = plan_conversion(&input, &preset).expect("must plan");

    let line = plan
        .reasoning
        .iter()
        .find(|line| line.contains("selección"))
        .unwrap_or_else(|| panic!("the plan must explain the pick: {:?}", plan.reasoning));
    assert!(line.contains("pista 2"), "index missing: {line}");
    assert!(line.contains("spa"), "language missing: {line}");
    assert!(line.contains("6 canales"), "channel count missing: {line}");
}

// ---------------------------------------------------------------------------
// Subtitles (the same blind `.first()` lived in `plan_subtitles`).
// ---------------------------------------------------------------------------

#[test]
fn subtitle_selection_prefers_the_preferred_language() {
    let input = multilingual();
    let preset = preset_with_language("system-smarttv-hevc", "es");

    let plan = plan_conversion(&input, &preset).expect("must plan");

    assert_eq!(plan.subtitles.action, SubtitleAction::Copy);
    assert_eq!(
        plan.subtitles.stream_index,
        Some(5),
        "the Spanish subtitle must win over the English one at index 4"
    );
}

#[test]
fn subtitle_selection_prefers_the_default_when_the_language_is_absent() {
    let input = snapshot(
        Vec::new(),
        vec![
            subtitle(4, Some("fra"), false),
            subtitle(5, Some("deu"), true),
        ],
    );
    let preset = preset_with_language("system-smarttv-hevc", "es");

    let plan = plan_conversion(&input, &preset).expect("must plan");

    assert_eq!(plan.subtitles.stream_index, Some(5));
}

#[test]
fn subtitle_selection_falls_back_to_the_first_track() {
    let input = snapshot(
        Vec::new(),
        vec![
            subtitle(4, Some("fra"), false),
            subtitle(5, Some("deu"), false),
        ],
    );

    let plan = plan_conversion(&input, preset("system-smarttv-hevc")).expect("must plan");

    assert_eq!(plan.subtitles.stream_index, Some(4));
}
