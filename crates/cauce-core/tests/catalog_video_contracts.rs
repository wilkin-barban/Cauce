//! Contract of the eight video system presets (`docs/presets.md`).
//!
//! Every expectation is copied from the preset row; a row that is silent about a
//! constraint leaves the field unset. Audio presets have their own file.

use cauce_core::{DeviceFamily, HdrPolicy, Preset, SubtitlePolicy, system_presets};

fn lookup(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("catalog must contain `{id}`"))
}

#[test]
fn compat_and_social_target_sdr_eight_bit() {
    for id in ["system-smarttv-compat", "system-youtube-social"] {
        let preset = lookup(id);
        assert_eq!(preset.hdr_policy, HdrPolicy::ToneMapSdr, "{id}");
        assert_eq!(preset.max_bit_depth, Some(8), "{id}");
        assert_eq!(preset.pixel_format.as_deref(), Some("yuv420p"), "{id}");
        assert_eq!(preset.profile.as_deref(), Some("high"), "{id}");
        assert_ne!(preset.video_codec, "none", "{id} keeps its video stream");
    }
}

#[test]
fn uhd_tv_presets_preserve_hdr_at_ten_bit() {
    for id in [
        "system-smarttv-hevc",
        "system-samsung-tv",
        "system-lg-tv",
        "system-android-tv",
    ] {
        let preset = lookup(id);
        assert_eq!(preset.hdr_policy, HdrPolicy::Preserve, "{id}");
        assert_eq!(preset.max_bit_depth, Some(10), "{id}");
        assert_eq!(preset.max_width, Some(3840), "{id}");
        assert_eq!(preset.max_height, Some(2160), "{id}");
    }

    // `presets.md` states "60 fps" only for the three device presets; the
    // generic Smart TV HEVC row gives just "3840×2160", so it stays unset.
    for id in ["system-samsung-tv", "system-lg-tv", "system-android-tv"] {
        assert_eq!(lookup(id).max_fps, Some(60.0), "{id}");
    }
    assert!(
        lookup("system-smarttv-hevc").max_fps.is_none(),
        "system-smarttv-hevc must not invent a frame-rate limit"
    );
}

#[test]
fn archival_master_copies_audio_and_keeps_subtitles() {
    let preset = lookup("system-archival-master");

    assert_eq!(preset.audio_codec, "copy");
    assert_eq!(preset.hdr_policy, HdrPolicy::Preserve);
    assert_eq!(preset.subtitle_policy, SubtitlePolicy::PreserveCompatible);
    assert!(
        preset.max_width.is_none(),
        "archival master has no size limit"
    );
    assert!(preset.max_height.is_none());
    assert!(preset.max_fps.is_none());
    assert!(
        preset.audio_bitrate.is_none(),
        "copied audio declares no bitrate"
    );
    assert_eq!(preset.device_family, DeviceFamily::Archive);
    assert_eq!(preset.hardware_preference, "cpu");
}

#[test]
fn mp4_video_presets_request_faststart() {
    let mut checked = 0;

    for preset in system_presets() {
        if preset.video_codec == "none" {
            continue;
        }
        checked += 1;
        let seeks_mp4 = preset.container == "mp4";
        assert_eq!(
            preset.faststart, seeks_mp4,
            "`{}` in `{}` must have faststart == {seeks_mp4}",
            preset.id, preset.container
        );
    }

    assert_eq!(
        checked, 8,
        "the video half of the catalog has eight presets"
    );
    assert!(
        lookup("system-smarttv-compat").faststart,
        "docs/architecture.md §11"
    );
    assert!(
        lookup("system-youtube-social").faststart,
        "docs/architecture.md §11"
    );
}
