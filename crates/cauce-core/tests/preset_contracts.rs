//! `Preset` contract (`docs/contracts.md` §2.3, `docs/presets.md`).

mod common;

use cauce_core::{DeviceFamily, HdrPolicy, Preset, SubtitlePolicy};

const PRESET_JSON: &str = r#"{
  "id": "system-smarttv-compat",
  "name_es": "Smart TV — Máxima Compatibilidad",
  "name_en": "Smart TV — Maximum Compatibility",
  "name_pt_br": "Smart TV — Compatibilidade Máxima",
  "description_es": "H.264 y AAC para televisores antiguos.",
  "description_en": "H.264 and AAC for older televisions.",
  "description_pt_br": "H.264 e AAC para televisores mais antigos.",
  "category": "tv",
  "device_family": "generic",
  "container": "mp4",
  "video_codec": "h264",
  "audio_codec": "aac",
  "video_quality": 21,
  "audio_bitrate": "192k",
  "max_width": 1920,
  "max_height": 1080,
  "max_fps": 60.0,
  "pixel_format": "yuv420p",
  "profile": "high",
  "level": 41,
  "max_bit_depth": 8,
  "hdr_policy": "tone-map-sdr",
  "subtitle_policy": "convert-compatible",
  "faststart": true,
  "preserve_metadata": true,
  "hardware_preference": "auto",
  "audio_channels": 2,
  "normalize_audio": false,
  "preferred_audio_language": null
}"#;

#[test]
fn preset_round_trips_and_declares_three_locales() {
    let preset = Preset {
        id: String::from("system-smarttv-compat"),
        name_es: String::from("Smart TV — Máxima Compatibilidad"),
        name_en: String::from("Smart TV — Maximum Compatibility"),
        name_pt_br: String::from("Smart TV — Compatibilidade Máxima"),
        description_es: String::from("H.264 y AAC para televisores antiguos."),
        description_en: String::from("H.264 and AAC for older televisions."),
        description_pt_br: String::from("H.264 e AAC para televisores mais antigos."),
        category: String::from("tv"),
        device_family: DeviceFamily::Generic,
        container: String::from("mp4"),
        video_codec: String::from("h264"),
        audio_codec: String::from("aac"),
        video_quality: Some(21),
        audio_bitrate: Some(String::from("192k")),
        max_width: Some(1920),
        max_height: Some(1080),
        max_fps: Some(60.0),
        pixel_format: Some(String::from("yuv420p")),
        profile: Some(String::from("high")),
        level: Some(41),
        max_bit_depth: Some(8),
        hdr_policy: HdrPolicy::ToneMapSdr,
        subtitle_policy: SubtitlePolicy::ConvertCompatible,
        faststart: true,
        preserve_metadata: true,
        hardware_preference: String::from("auto"),
        audio_channels: Some(2),
        normalize_audio: false,
        preferred_audio_language: None,
    };

    common::round_trip(&preset, PRESET_JSON);
}
