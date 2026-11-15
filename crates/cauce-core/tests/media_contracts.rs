//! `MediaSnapshot` contract (`docs/contracts.md` §2.1).

mod common;

use cauce_core::{
    AudioStream, MediaSnapshot, SubtitleKind, SubtitleStream, Timestamp, VideoStream,
};

const SNAPSHOT_JSON: &str = r#"{
  "path": "/home/wilkin/videos/holiday.mp4",
  "container": "mp4",
  "duration_ms": 12000,
  "size_bytes": 1048576,
  "bitrate_bps": 700000,
  "video_streams": [
    {
      "index": 0,
      "codec": "h264",
      "profile": "High",
      "level": 41,
      "width": 1920,
      "height": 1080,
      "fps": 30.0,
      "pixel_format": "yuv420p",
      "bit_depth": 8,
      "is_hdr": false,
      "bitrate_bps": 600000,
      "language": null
    }
  ],
  "audio_streams": [
    {
      "index": 1,
      "codec": "aac",
      "channels": 2,
      "sample_rate": 48000,
      "bitrate_bps": 128000,
      "language": "eng",
      "is_default": true
    }
  ],
  "subtitle_streams": [
    {
      "index": 2,
      "codec": "subrip",
      "language": "spa",
      "kind": "text",
      "is_default": false,
      "is_forced": false
    }
  ],
  "probed_at": "2026-11-14T10:00:00Z"
}"#;

#[test]
fn media_snapshot_round_trips_through_golden_json() {
    let snapshot = MediaSnapshot {
        path: String::from("/home/wilkin/videos/holiday.mp4"),
        container: String::from("mp4"),
        duration_ms: 12_000,
        size_bytes: 1_048_576,
        bitrate_bps: 700_000,
        video_streams: vec![VideoStream {
            index: 0,
            codec: String::from("h264"),
            profile: Some(String::from("High")),
            level: Some(41),
            width: 1920,
            height: 1080,
            fps: 30.0,
            pixel_format: String::from("yuv420p"),
            bit_depth: 8,
            is_hdr: false,
            bitrate_bps: Some(600_000),
            language: None,
        }],
        audio_streams: vec![AudioStream {
            index: 1,
            codec: String::from("aac"),
            channels: 2,
            sample_rate: 48_000,
            bitrate_bps: Some(128_000),
            language: Some(String::from("eng")),
            is_default: true,
        }],
        subtitle_streams: vec![SubtitleStream {
            index: 2,
            codec: String::from("subrip"),
            language: Some(String::from("spa")),
            kind: SubtitleKind::Text,
            is_default: false,
            is_forced: false,
        }],
        probed_at: Timestamp::parse("2026-11-14T10:00:00Z").expect("valid timestamp"),
    };

    common::round_trip(&snapshot, SNAPSHOT_JSON);
}
