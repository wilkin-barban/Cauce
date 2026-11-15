//! Compatibility engine, video and audio halves (task T6 / C-014).
//!
//! The engine is pure, so every case drives the real thirteen-preset catalog
//! through `system_presets()` and looks presets up by ID.

use cauce_core::compat::plan_conversion;
use cauce_core::{
    AudioStream, ConversionPlan, ErrorCode, MediaSnapshot, MetadataPolicy, Preset, StreamAction,
    SubtitleAction, Timestamp, VideoStream, system_presets,
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

fn video(codec: &str, width: u32, height: u32, bit_depth: u8) -> VideoStream {
    VideoStream {
        index: 0,
        codec: codec.into(),
        profile: None,
        level: None,
        width,
        height,
        fps: 25.0,
        pixel_format: if bit_depth > 8 {
            format!("yuv420p{bit_depth}le")
        } else {
            "yuv420p".into()
        },
        bit_depth,
        is_hdr: false,
        bitrate_bps: Some(4_000_000),
        language: None,
    }
}

fn audio(codec: &str, channels: u8) -> AudioStream {
    AudioStream {
        index: 1,
        codec: codec.into(),
        channels,
        sample_rate: 48_000,
        bitrate_bps: Some(192_000),
        language: None,
        is_default: true,
    }
}

fn snapshot(video_streams: Vec<VideoStream>, audio_streams: Vec<AudioStream>) -> MediaSnapshot {
    MediaSnapshot {
        path: "/home/wilkin/Vídeos/entrada.mp4".into(),
        container: "mp4".into(),
        duration_ms: 2_000,
        size_bytes: 1_024,
        bitrate_bps: 520_000,
        video_streams,
        audio_streams,
        subtitle_streams: Vec::new(),
        probed_at: probed_at(),
    }
}

/// `docs/contracts.md` §3: copy has no encoder, remove has no stream index.
fn assert_invariants(plan: &ConversionPlan) {
    if plan.video.action == StreamAction::Copy {
        assert_eq!(
            plan.video.encoder, None,
            "video copy must not name an encoder"
        );
    }
    if plan.video.action == StreamAction::Remove {
        assert_eq!(
            plan.video.stream_index, None,
            "video remove must not keep a stream"
        );
        assert_eq!(
            plan.video.encoder, None,
            "video remove must not name an encoder"
        );
    }
    if plan.video.action == StreamAction::Transcode {
        assert!(
            plan.video.encoder.is_some(),
            "video transcode must name an encoder"
        );
    }
    if plan.audio.action == StreamAction::Copy {
        assert_eq!(
            plan.audio.encoder, None,
            "audio copy must not name an encoder"
        );
        assert_eq!(
            plan.audio.channels, None,
            "audio copy must not set channels"
        );
    }
    if plan.audio.action == StreamAction::Remove {
        assert_eq!(
            plan.audio.stream_index, None,
            "audio remove must not keep a stream"
        );
    }
    if plan.audio.action == StreamAction::Transcode {
        assert!(
            plan.audio.encoder.is_some(),
            "audio transcode must name an encoder"
        );
    }
    if plan.subtitles.action == SubtitleAction::Copy {
        assert_eq!(
            plan.subtitles.encoder, None,
            "subtitle copy must not name an encoder"
        );
    }
    if plan.subtitles.action == SubtitleAction::Remove {
        assert_eq!(
            plan.subtitles.stream_index, None,
            "subtitle remove must not keep a stream"
        );
        assert_eq!(
            plan.subtitles.source_index, None,
            "subtitle remove must not keep a source"
        );
    }
    if plan.subtitles.action == SubtitleAction::Transcode {
        assert!(
            plan.subtitles.encoder.is_some(),
            "subtitle transcode must name an encoder"
        );
    }
    assert_ne!(
        plan.subtitles.action,
        SubtitleAction::BurnIn,
        "v0.1 never burns in"
    );
}

#[test]
fn every_catalog_preset_produces_a_valid_plan() {
    let input = snapshot(vec![video("h264", 1280, 720, 8)], vec![audio("aac", 2)]);

    for preset in system_presets() {
        let plan = plan_conversion(&input, preset)
            .unwrap_or_else(|error| panic!("`{}` must plan: {error:?}", preset.id));

        assert_eq!(
            plan.container, preset.container,
            "`{}` container",
            preset.id
        );
        assert_eq!(
            plan.faststart, preset.faststart,
            "`{}` faststart",
            preset.id
        );
        let metadata = if preset.preserve_metadata {
            MetadataPolicy::Preserve
        } else {
            MetadataPolicy::Strip
        };
        assert_eq!(plan.metadata, metadata, "`{}` metadata", preset.id);
        assert_invariants(&plan);

        // One entry per family present in the snapshot: video and audio.
        assert!(
            plan.reasoning.len() >= 2,
            "`{}` must explain video and audio: {:?}",
            preset.id,
            plan.reasoning
        );
        assert!(
            plan.reasoning.iter().any(|line| line.contains("video:")),
            "`{}` must explain the video decision: {:?}",
            preset.id,
            plan.reasoning
        );
        assert!(
            plan.reasoning.iter().any(|line| line.contains("audio:")),
            "`{}` must explain the audio decision: {:?}",
            preset.id,
            plan.reasoning
        );

        // `presets.md` obligatoria: hardware `auto` resolves to CPU and says so.
        if preset.hardware_preference == "auto" {
            assert!(
                plan.reasoning.iter().any(|line| line.contains("CPU")),
                "`{}` must declare its CPU resolution: {:?}",
                preset.id,
                plan.reasoning
            );
        }
    }
}

#[test]
fn audio_presets_remove_video() {
    let input = snapshot(vec![video("h264", 1920, 1080, 8)], vec![audio("aac", 2)]);
    let audio_presets: Vec<&Preset> = system_presets()
        .iter()
        .filter(|preset| preset.video_codec == "none")
        .collect();
    assert_eq!(
        audio_presets.len(),
        5,
        "`presets.md` declares five audio presets"
    );

    for preset in audio_presets {
        let plan = plan_conversion(&input, preset).expect("audio preset must plan");
        assert_eq!(plan.video.action, StreamAction::Remove, "`{}`", preset.id);
        assert_eq!(plan.video.encoder, None, "`{}`", preset.id);
        assert_eq!(plan.video.stream_index, None, "`{}`", preset.id);
        assert!(
            plan.reasoning
                .iter()
                .any(|line| line.contains("video: remove")),
            "`{}` must explain the removed video: {:?}",
            preset.id,
            plan.reasoning
        );
    }
}

#[test]
fn copies_when_codec_container_and_limits_match() {
    let input = snapshot(vec![video("hevc", 3840, 2160, 10)], vec![audio("aac", 2)]);
    let plan = plan_conversion(&input, preset("system-archival-master")).expect("must plan");

    assert_eq!(plan.container, "mkv");
    assert_eq!(plan.video.action, StreamAction::Copy);
    assert_eq!(plan.video.encoder, None);
    assert_eq!(plan.video.stream_index, Some(0));
    assert_eq!(plan.audio.action, StreamAction::Copy);
    assert_eq!(plan.audio.encoder, None);
    assert_eq!(plan.audio.stream_index, Some(1));
    assert_invariants(&plan);
}

#[test]
fn transcodes_when_codec_differs() {
    let input = snapshot(vec![video("h264", 1920, 1080, 8)], vec![audio("aac", 2)]);
    let plan = plan_conversion(&input, preset("system-smarttv-hevc")).expect("must plan");

    assert_eq!(plan.container, "mkv");
    assert_eq!(plan.video.action, StreamAction::Transcode);
    assert_eq!(plan.video.encoder.as_deref(), Some("libx265"));
    assert_eq!(
        plan.video.quality,
        Some(24),
        "preset CRF is copied into the plan"
    );
    assert_eq!(plan.video.stream_index, Some(0));
    assert_eq!(plan.video.max_width, Some(3840));
    assert_eq!(plan.video.max_height, Some(2160));
    assert!(
        plan.reasoning.iter().any(|line| line.contains("h264")),
        "reasoning must name the source codec: {:?}",
        plan.reasoning
    );
}

#[test]
fn downmixes_six_channels_to_two() {
    let input = snapshot(vec![video("h264", 1920, 1080, 8)], vec![audio("ac3", 6)]);

    let plan = plan_conversion(&input, preset("system-smarttv-compat")).expect("must plan");
    assert_eq!(plan.audio.action, StreamAction::Transcode);
    assert_eq!(plan.audio.encoder.as_deref(), Some("aac"));
    assert_eq!(
        plan.audio.channels,
        Some(2),
        "the preset's 2 channels downmix"
    );
    assert_eq!(plan.audio.bitrate.as_deref(), Some("192k"));
    assert_eq!(plan.audio.stream_index, Some(1));

    // The real catalog declares six channels for Samsung: no downmix to two.
    let samsung = plan_conversion(&input, preset("system-samsung-tv")).expect("must plan");
    assert_eq!(samsung.audio.channels, Some(6));
}

#[test]
fn keeps_mono_audio_without_resampling() {
    let input = snapshot(vec![video("h264", 1920, 1080, 8)], vec![audio("aac", 1)]);
    let plan = plan_conversion(&input, preset("system-smarttv-compat")).expect("must plan");

    assert_eq!(plan.audio.action, StreamAction::Copy);
    assert_eq!(plan.audio.encoder, None);
    assert_eq!(plan.audio.channels, None);
    assert_eq!(plan.audio.bitrate, None);
    assert!(
        plan.reasoning.iter().any(|line| line.contains("upmix")),
        "reasoning must state that no upmix happens: {:?}",
        plan.reasoning
    );
}

#[test]
fn rejects_unsupported_preset_codec() {
    let input = snapshot(vec![video("h264", 1920, 1080, 8)], vec![audio("aac", 2)]);

    let mut unsupported = preset("system-smarttv-compat").clone();
    unsupported.video_codec = "mpeg4".into();
    let error = plan_conversion(&input, &unsupported).expect_err("mpeg4 has no CPU encoder");
    assert_eq!(error.code, ErrorCode::PlanInvalid);
    assert!(!error.retryable, "a bad preset is not retryable");

    let mut wrong_container = preset("system-smarttv-compat").clone();
    wrong_container.video_codec = "vp9".into();
    let error = plan_conversion(&input, &wrong_container).expect_err("mp4 does not carry vp9");
    assert_eq!(error.code, ErrorCode::PlanInvalid);

    let mut unsupported_audio = preset("system-smarttv-compat").clone();
    unsupported_audio.audio_codec = "vorbis".into();
    let error = plan_conversion(&input, &unsupported_audio).expect_err("vorbis is not in v0.1");
    assert_eq!(error.code, ErrorCode::PlanInvalid);
}

#[test]
fn audio_transcodes_use_the_real_ffmpeg_encoder() {
    // presets.md "Resolución en v0.1" item 4 names the CPU audio encoders.
    let source = snapshot(vec![video("h264", 1280, 720, 8)], vec![audio("ac3", 2)]);

    for (preset_id, encoder) in [
        ("system-audio-mp3", "libmp3lame"),
        ("system-audio-flac", "flac"),
        ("system-audio-alac", "alac"),
        ("system-audio-wav", "pcm_s16le"),
        ("system-apple-music-export", "aac"),
        ("system-smarttv-compat", "aac"),
    ] {
        let plan = plan_conversion(&source, preset(preset_id)).expect("plan must build");
        assert_eq!(
            plan.audio.action,
            StreamAction::Transcode,
            "{preset_id} must re-encode ac3"
        );
        assert_eq!(
            plan.audio.encoder.as_deref(),
            Some(encoder),
            "{preset_id} must use the FFmpeg encoder {encoder}"
        );
    }
}
