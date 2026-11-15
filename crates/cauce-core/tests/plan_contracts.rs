//! `ConversionPlan` contract (`docs/contracts.md` §2.2).

mod common;

use cauce_core::{
    AudioPlan, ConversionPlan, HdrAction, MetadataPolicy, StreamAction, SubtitleAction,
    SubtitlePlan, VideoPlan,
};

const PLAN_JSON: &str = r#"{
  "container": "mp4",
  "video": {
    "action": "transcode",
    "encoder": "libx264",
    "quality": 21,
    "max_width": 1920,
    "max_height": 1080,
    "fps": 60.0,
    "pixel_format": "yuv420p",
    "profile": "high",
    "level": 41,
    "hdr_action": "tone_map_sdr",
    "stream_index": 0
  },
  "audio": {
    "action": "transcode",
    "encoder": "aac",
    "bitrate": "192k",
    "channels": 2,
    "stream_index": 1,
    "normalize": false
  },
  "subtitles": {
    "action": "remove",
    "encoder": null,
    "stream_index": null,
    "source_index": null
  },
  "faststart": true,
  "metadata": "preserve",
  "reasoning": [
    "video: h264 high profile is not compatible with the preset target, transcoding with libx264",
    "audio: aac keeps 2 channels at 192k"
  ]
}"#;

#[test]
fn conversion_plan_round_trips_and_keeps_reasoning() {
    let plan = ConversionPlan {
        container: String::from("mp4"),
        video: VideoPlan {
            action: StreamAction::Transcode,
            encoder: Some(String::from("libx264")),
            quality: Some(21),
            max_width: Some(1920),
            max_height: Some(1080),
            fps: Some(60.0),
            pixel_format: Some(String::from("yuv420p")),
            profile: Some(String::from("high")),
            level: Some(41),
            hdr_action: HdrAction::ToneMapSdr,
            stream_index: Some(0),
        },
        audio: AudioPlan {
            action: StreamAction::Transcode,
            encoder: Some(String::from("aac")),
            bitrate: Some(String::from("192k")),
            channels: Some(2),
            stream_index: Some(1),
            normalize: false,
        },
        subtitles: SubtitlePlan {
            action: SubtitleAction::Remove,
            encoder: None,
            stream_index: None,
            source_index: None,
        },
        faststart: true,
        metadata: MetadataPolicy::Preserve,
        reasoning: vec![
            String::from(
                "video: h264 high profile is not compatible with the preset target, transcoding with libx264",
            ),
            String::from("audio: aac keeps 2 channels at 192k"),
        ],
    };

    common::round_trip(&plan, PLAN_JSON);
    assert_eq!(plan.reasoning.len(), 2, "reasoning explains every decision");
}
