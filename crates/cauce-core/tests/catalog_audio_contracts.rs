//! Contract of the five audio system presets (`docs/presets.md`).
//!
//! Audio presets declare `video_codec: none` and drop the video and subtitle
//! streams: none of their containers can carry them.

use cauce_core::{Preset, SubtitlePolicy, system_presets};

const AUDIO_IDS: [&str; 5] = [
    "system-audio-mp3",
    "system-audio-flac",
    "system-apple-music-export",
    "system-audio-alac",
    "system-audio-wav",
];

fn lookup(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("catalog must contain `{id}`"))
}

#[test]
fn audio_presets_remove_video() {
    for id in AUDIO_IDS {
        let preset = lookup(id);
        assert_eq!(
            preset.video_codec, "none",
            "{id} must remove the video stream"
        );
        assert!(
            matches!(preset.container.as_str(), "mp3" | "flac" | "m4a" | "wav"),
            "{id} must target an audio container, got `{}`",
            preset.container
        );
        assert_eq!(
            preset.subtitle_policy,
            SubtitlePolicy::Remove,
            "{id} cannot carry subtitles"
        );
        assert_eq!(preset.hardware_preference, "cpu", "{id}");
    }

    let with_video = system_presets()
        .iter()
        .filter(|preset| preset.video_codec != "none")
        .count();
    assert_eq!(with_video, 8, "only the audio presets drop video");
}

#[test]
fn audio_presets_are_complete_in_three_locales() {
    for id in AUDIO_IDS {
        let preset = lookup(id);
        for (label, text) in [
            ("name_es", &preset.name_es),
            ("name_en", &preset.name_en),
            ("name_pt_br", &preset.name_pt_br),
            ("description_es", &preset.description_es),
            ("description_en", &preset.description_en),
            ("description_pt_br", &preset.description_pt_br),
        ] {
            assert!(!text.trim().is_empty(), "{id} {label} must not be empty");
        }
        assert_eq!(preset.category, "audio", "{id}");
    }
}

#[test]
fn audio_presets_declare_no_video_limits() {
    for id in AUDIO_IDS {
        let preset = lookup(id);
        assert!(preset.max_width.is_none(), "{id}");
        assert!(preset.max_height.is_none(), "{id}");
        assert!(preset.max_fps.is_none(), "{id}");
        assert!(preset.pixel_format.is_none(), "{id}");
        assert!(preset.video_quality.is_none(), "{id}");
    }
}
