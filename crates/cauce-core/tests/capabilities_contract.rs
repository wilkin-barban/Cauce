//! `Capabilities` contract (`docs/contracts.md` §6, method `system.capabilities`).
//!
//! The report is what the diagnostic page shows and what the capability gate
//! checks, so its wire shape and the encoder list it validates are pinned here.

mod common;

use cauce_core::command::{HostFilters, REQUIRED_ENCODERS, build_ffmpeg_args};
use cauce_core::{
    AudioPlan, Capabilities, ConversionPlan, HdrAction, MetadataPolicy, StreamAction,
    SubtitleAction, SubtitlePlan, VideoPlan,
};

const CAPABILITIES_JSON: &str = r#"{
  "ffmpeg_version": "8.0.1",
  "ffprobe_version": "8.0.1",
  "encoders": ["aac", "alac", "flac"],
  "filters": ["format", "scale", "tonemap", "zscale"],
  "cpu_cores": 6,
  "disk_free_bytes": 5481005056,
  "daemon_version": "0.1.0",
  "ffmpeg_missing": ["libx265"],
  "warnings": ["Falta el encoder `libx265` que exige el MVP."]
}"#;

/// Builder allowlists, read from the source so the test needs no new API.
fn allowlist(source: &str, name: &str) -> Vec<String> {
    let marker = format!("const {name}:");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("the `{name}` allowlist must exist in src/command.rs"));
    let declaration = &source[start..];
    let assignment = declaration
        .find('=')
        .expect("the allowlist must be initialized");
    let body = &declaration[assignment..];
    let open = body.find('[').expect("the allowlist must be an array");
    let close = body.find("];").expect("the allowlist array must be closed");
    body[open..close]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn video_plan(encoder: &str) -> VideoPlan {
    VideoPlan {
        action: StreamAction::Transcode,
        encoder: Some(encoder.to_string()),
        quality: Some(23),
        max_width: None,
        max_height: None,
        fps: None,
        pixel_format: None,
        profile: None,
        level: None,
        hdr_action: HdrAction::Preserve,
        stream_index: Some(0),
        tone_mapping: None,
    }
}

fn audio_plan(encoder: &str) -> AudioPlan {
    AudioPlan {
        action: StreamAction::Transcode,
        encoder: Some(encoder.to_string()),
        bitrate: None,
        channels: None,
        stream_index: Some(1),
        normalize: false,
        filter: None,
    }
}

fn plan(video: VideoPlan, audio: AudioPlan) -> ConversionPlan {
    ConversionPlan {
        container: "mp4".to_string(),
        video,
        audio,
        subtitles: SubtitlePlan {
            action: SubtitleAction::Remove,
            encoder: None,
            stream_index: None,
            source_index: None,
        },
        faststart: false,
        metadata: MetadataPolicy::Preserve,
        reasoning: Vec::new(),
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
    }
}

#[test]
fn capabilities_round_trip_through_golden_json() {
    let capabilities = Capabilities {
        ffmpeg_version: "8.0.1".to_string(),
        ffprobe_version: "8.0.1".to_string(),
        encoders: vec!["aac".to_string(), "alac".to_string(), "flac".to_string()],
        filters: vec![
            "format".to_string(),
            "scale".to_string(),
            "tonemap".to_string(),
            "zscale".to_string(),
        ],
        cpu_cores: 6,
        disk_free_bytes: 5_481_005_056,
        daemon_version: "0.1.0".to_string(),
        ffmpeg_missing: vec!["libx265".to_string()],
        warnings: vec!["Falta el encoder `libx265` que exige el MVP.".to_string()],
    };

    common::round_trip(&capabilities, CAPABILITIES_JSON);
}

#[test]
fn required_encoders_match_the_builder_allowlists() {
    let source = include_str!("../src/command.rs");
    let video = allowlist(source, "VIDEO_ENCODERS");
    let audio = allowlist(source, "AUDIO_ENCODERS");

    assert!(
        !video.is_empty() && !audio.is_empty(),
        "both builder allowlists must be readable in src/command.rs"
    );

    // Every MVP encoder is accepted by the builder that will emit it.
    for encoder in REQUIRED_ENCODERS {
        let args = if video.iter().any(|allowed| allowed == encoder) {
            build_ffmpeg_args(
                "/home/wilkin/Vídeos/entrada.mp4",
                "/home/wilkin/Vídeos/.cauce/entrada.part.mp4",
                &plan(
                    video_plan(encoder),
                    AudioPlan {
                        action: StreamAction::Remove,
                        encoder: None,
                        bitrate: None,
                        channels: None,
                        stream_index: None,
                        normalize: false,
                        filter: None,
                    },
                ),
                &HostFilters::ALL,
            )
        } else {
            build_ffmpeg_args(
                "/home/wilkin/Vídeos/entrada.mp4",
                "/home/wilkin/Vídeos/.cauce/entrada.part.mp4",
                &plan(
                    VideoPlan {
                        action: StreamAction::Remove,
                        tone_mapping: None,
                        ..video_plan("libx264")
                    },
                    audio_plan(encoder),
                ),
                &HostFilters::ALL,
            )
        };

        assert!(
            args.is_ok(),
            "`{encoder}` is required by the MVP but the builder rejects it: {:?}",
            args.err()
        );
    }

    // And nothing the builder accepts escapes the capability gate.
    for encoder in video.iter().chain(audio.iter()) {
        assert!(
            REQUIRED_ENCODERS.contains(&encoder.as_str()),
            "the builder accepts `{encoder}`, which `REQUIRED_ENCODERS` does not check"
        );
    }

    assert_eq!(
        video.len() + audio.len(),
        REQUIRED_ENCODERS.len(),
        "the allowlists and `REQUIRED_ENCODERS` must cover the same encoders"
    );
}
