//! Compatibility engine, subtitles and plan-level halves (task T6 / C-014).
//!
//! Container and codec compatibility is decided per stream family, so these
//! cases use the real catalog and vary only the snapshot.

use cauce_core::compat::plan_conversion;
use cauce_core::{
    AudioStream, HdrAction, MediaSnapshot, Preset, StreamAction, SubtitleAction, SubtitleKind,
    SubtitleStream, Timestamp, VideoStream, system_presets,
};

fn probed_at() -> Timestamp {
    Timestamp::parse("2026-11-14T10:00:00Z").expect("fixture timestamp must be valid UTC")
}

fn preset(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("`{id}` must exist in the catalog"))
}

fn video(is_hdr: bool) -> VideoStream {
    VideoStream {
        index: 0,
        codec: "hevc".into(),
        profile: None,
        level: None,
        width: 3840,
        height: 2160,
        fps: 25.0,
        pixel_format: "yuv420p10le".into(),
        bit_depth: 10,
        is_hdr,
        bitrate_bps: None,
        language: None,
    }
}

fn audio() -> AudioStream {
    AudioStream {
        index: 1,
        codec: "aac".into(),
        channels: 2,
        sample_rate: 48_000,
        bitrate_bps: None,
        language: None,
        is_default: true,
    }
}

fn subtitle(codec: &str, kind: SubtitleKind) -> SubtitleStream {
    SubtitleStream {
        index: 2,
        codec: codec.into(),
        language: Some("spa".into()),
        kind,
        is_default: true,
        is_forced: false,
    }
}

fn snapshot(video_stream: VideoStream, subtitles: Vec<SubtitleStream>) -> MediaSnapshot {
    MediaSnapshot {
        path: "/home/wilkin/Vídeos/entrada.mkv".into(),
        container: "mkv".into(),
        duration_ms: 2_000,
        size_bytes: 1_024,
        bitrate_bps: 520_000,
        video_streams: vec![video_stream],
        audio_streams: vec![audio()],
        subtitle_streams: subtitles,
        probed_at: probed_at(),
    }
}

fn with_subtitles(codec: &str, kind: SubtitleKind) -> MediaSnapshot {
    snapshot(video(false), vec![subtitle(codec, kind)])
}

#[test]
fn tonemaps_hdr_for_sdr_presets() {
    let input = snapshot(video(true), Vec::new());
    let plan = plan_conversion(&input, preset("system-smarttv-compat")).expect("must plan");

    assert_eq!(plan.video.action, StreamAction::Transcode);
    assert_eq!(plan.video.encoder.as_deref(), Some("libx264"));
    assert_eq!(plan.video.hdr_action, HdrAction::ToneMapSdr);
    assert!(
        plan.reasoning.iter().any(|line| line.contains("HDR")),
        "reasoning must explain the tone mapping: {:?}",
        plan.reasoning
    );
}

#[test]
fn preserves_hdr_for_uhd_presets() {
    let input = snapshot(video(true), Vec::new());

    for id in ["system-samsung-tv", "system-lg-tv", "system-android-tv"] {
        let plan = plan_conversion(&input, preset(id)).expect("must plan");
        assert_eq!(plan.video.hdr_action, HdrAction::Preserve, "`{id}`");
        assert_eq!(
            plan.video.action,
            StreamAction::Copy,
            "`{id}` HDR source fits"
        );
        assert_eq!(plan.video.encoder, None, "`{id}`");
    }
}

#[test]
fn removes_subtitles_when_policy_forbids_them() {
    let input = with_subtitles("subrip", SubtitleKind::Text);

    for id in ["system-youtube-social", "system-audio-mp3"] {
        let plan = plan_conversion(&input, preset(id)).expect("must plan");
        assert_eq!(plan.subtitles.action, SubtitleAction::Remove, "`{id}`");
        assert_eq!(plan.subtitles.encoder, None, "`{id}`");
        assert_eq!(plan.subtitles.stream_index, None, "`{id}`");
        assert_eq!(plan.subtitles.source_index, None, "`{id}`");
        assert!(
            plan.reasoning
                .iter()
                .any(|line| line.contains("subtítulos: remove")),
            "`{id}` must explain the removal: {:?}",
            plan.reasoning
        );
    }
}

#[test]
fn copies_compatible_subtitles_into_mkv() {
    for (codec, kind) in [
        ("subrip", SubtitleKind::Text),
        ("ass", SubtitleKind::Text),
        ("ssa", SubtitleKind::Text),
        ("webvtt", SubtitleKind::Text),
        ("mov_text", SubtitleKind::Text),
        ("dvd_subtitle", SubtitleKind::Bitmap),
        ("hdmv_pgs_subtitle", SubtitleKind::Bitmap),
    ] {
        let input = with_subtitles(codec, kind);
        let plan = plan_conversion(&input, preset("system-smarttv-hevc")).expect("must plan");
        assert_eq!(
            plan.subtitles.action,
            SubtitleAction::Copy,
            "mkv already carries `{codec}`"
        );
        assert_eq!(plan.subtitles.encoder, None, "`{codec}`");
        assert_eq!(plan.subtitles.stream_index, Some(2), "`{codec}`");
        assert_eq!(plan.subtitles.source_index, None, "`{codec}`");
    }
}

#[test]
fn converts_text_subtitles_for_mp4() {
    let input = with_subtitles("subrip", SubtitleKind::Text);
    let plan = plan_conversion(&input, preset("system-smarttv-compat")).expect("must plan");

    assert_eq!(plan.subtitles.action, SubtitleAction::Transcode);
    assert_eq!(plan.subtitles.encoder.as_deref(), Some("mov_text"));
    assert_eq!(plan.subtitles.source_index, Some(2));
    assert_eq!(plan.subtitles.stream_index, None);
}

#[test]
fn removes_bitmap_subtitles_that_would_need_ocr() {
    let input = with_subtitles("hdmv_pgs_subtitle", SubtitleKind::Bitmap);
    let plan = plan_conversion(&input, preset("system-smarttv-compat")).expect("must plan");

    assert_eq!(plan.subtitles.action, SubtitleAction::Remove);
    assert_eq!(plan.subtitles.encoder, None);
    assert_eq!(plan.subtitles.source_index, None);
    assert!(
        plan.reasoning.iter().any(|line| line.contains("OCR")),
        "reasoning must state that v0.1 has no OCR: {:?}",
        plan.reasoning
    );

    // The same bitmap track survives in a container that carries it.
    let plan = plan_conversion(&input, preset("system-lg-tv")).expect("must plan");
    assert_eq!(plan.subtitles.action, SubtitleAction::Copy);
}

#[test]
fn copy_has_no_encoder_and_remove_has_no_stream() {
    let input = with_subtitles("subrip", SubtitleKind::Text);

    for preset in system_presets() {
        let plan = plan_conversion(&input, preset).expect("every preset must plan");
        assert_ne!(
            plan.subtitles.action,
            SubtitleAction::BurnIn,
            "`{}`",
            preset.id
        );

        if plan.video.action == StreamAction::Copy {
            assert_eq!(plan.video.encoder, None, "`{}` video copy", preset.id);
        }
        if plan.video.action == StreamAction::Remove {
            assert_eq!(
                plan.video.stream_index, None,
                "`{}` video remove",
                preset.id
            );
        }
        if plan.video.action == StreamAction::Transcode {
            assert!(
                plan.video.encoder.is_some(),
                "`{}` video transcode",
                preset.id
            );
        }

        if plan.audio.action == StreamAction::Copy {
            assert_eq!(plan.audio.encoder, None, "`{}` audio copy", preset.id);
            assert_eq!(plan.audio.channels, None, "`{}` audio copy", preset.id);
        }
        if plan.audio.action == StreamAction::Remove {
            assert_eq!(
                plan.audio.stream_index, None,
                "`{}` audio remove",
                preset.id
            );
        }
        if plan.audio.action == StreamAction::Transcode {
            assert!(
                plan.audio.encoder.is_some(),
                "`{}` audio transcode",
                preset.id
            );
        }

        if plan.subtitles.action == SubtitleAction::Copy {
            assert_eq!(
                plan.subtitles.encoder, None,
                "`{}` subtitle copy",
                preset.id
            );
        }
        if plan.subtitles.action == SubtitleAction::Remove {
            assert_eq!(
                plan.subtitles.stream_index, None,
                "`{}` subtitle remove",
                preset.id
            );
            assert_eq!(
                plan.subtitles.source_index, None,
                "`{}` subtitle remove",
                preset.id
            );
        }
        if plan.subtitles.action == SubtitleAction::Transcode {
            assert!(
                plan.subtitles.encoder.is_some(),
                "`{}` subtitle transcode",
                preset.id
            );
        }
    }
}

#[test]
fn plan_is_reproducible() {
    let input = with_subtitles("subrip", SubtitleKind::Text);

    for preset in system_presets() {
        let first = plan_conversion(&input, preset).expect("must plan");
        let second = plan_conversion(&input, preset).expect("must plan");
        assert_eq!(first, second, "`{}` must be reproducible", preset.id);
        assert_eq!(
            serde_json::to_string(&first).expect("plan must serialize"),
            serde_json::to_string(&second).expect("plan must serialize"),
            "`{}` must serialize identically",
            preset.id
        );
    }
}
