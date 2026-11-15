//! Catalog-wide invariants of the thirteen system presets.
//!
//! Domain-specific expectations live in `catalog_video_contracts.rs` and
//! `catalog_audio_contracts.rs`; this file pins the catalog as a whole:
//! identity, locales, immutability and the v0.1 CPU-only encoder rule.

use cauce_core::{
    CustomAudio, CustomBitrate, CustomContainer, CustomPreset, CustomQuality, CustomQualityLevel,
    CustomResolution, CustomSubtitles, CustomVideo, ErrorCode, Preset, system_presets,
};

const CANONICAL_IDS: [&str; 13] = [
    "system-smarttv-compat",
    "system-smarttv-hevc",
    "system-youtube-social",
    "system-archival-master",
    "system-fast-compress",
    "system-audio-mp3",
    "system-audio-flac",
    "system-apple-music-export",
    "system-samsung-tv",
    "system-lg-tv",
    "system-android-tv",
    "system-audio-alac",
    "system-audio-wav",
];

/// `id`, `name_es`, `name_en`, `name_pt_br`, copied verbatim from `presets.md`.
const NAMES: [(&str, &str, &str, &str); 13] = [
    (
        "system-smarttv-compat",
        "Smart TV — Máxima Compatibilidad",
        "Maximum Compatibility",
        "Smart TV — Compatibilidade Máxima",
    ),
    (
        "system-smarttv-hevc",
        "Smart TV — Calidad Eficiente",
        "Efficient Quality",
        "Smart TV — Qualidade Eficiente",
    ),
    (
        "system-youtube-social",
        "YouTube & Redes Sociales",
        "YouTube & Social",
        "YouTube e Redes Sociais",
    ),
    (
        "system-archival-master",
        "Archivo / Máxima Calidad",
        "Archival Master",
        "Arquivo / Qualidade Máxima",
    ),
    (
        "system-fast-compress",
        "Compresión Rápida",
        "Fast Compression",
        "Compressão Rápida",
    ),
    (
        "system-audio-mp3",
        "Extraer Audio — MP3 320k",
        "Extract Audio — MP3 320k",
        "Extrair Áudio — MP3 320k",
    ),
    (
        "system-audio-flac",
        "Extraer Audio — FLAC",
        "Extract Audio — FLAC",
        "Extrair Áudio — FLAC",
    ),
    (
        "system-apple-music-export",
        "Exportación Apple Music",
        "Apple Music Export",
        "Exportação Apple Music",
    ),
    (
        "system-samsung-tv",
        "Samsung TV — 4K",
        "Samsung TV — 4K",
        "Samsung TV — 4K",
    ),
    (
        "system-lg-tv",
        "LG TV — HEVC 4K",
        "LG TV — HEVC 4K",
        "LG TV — HEVC 4K",
    ),
    (
        "system-android-tv",
        "Android TV",
        "Google TV",
        "Android TV / Google TV",
    ),
    (
        "system-audio-alac",
        "Extraer Audio — ALAC",
        "Extract Audio — ALAC",
        "Extrair Áudio — ALAC",
    ),
    (
        "system-audio-wav",
        "Extraer Audio — WAV PCM",
        "Extract Audio — WAV PCM",
        "Extrair Áudio — WAV PCM",
    ),
];

fn lookup(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("catalog must contain `{id}`"))
}

#[test]
fn catalog_has_the_thirteen_system_presets() {
    let ids: Vec<&str> = system_presets()
        .iter()
        .map(|preset| preset.id.as_str())
        .collect();

    let mut sorted = ids.clone();
    sorted.sort_unstable();
    let mut expected = CANONICAL_IDS.to_vec();
    expected.sort_unstable();
    assert_eq!(
        sorted, expected,
        "catalog must hold exactly the presets.md IDs"
    );

    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "preset IDs must be unique");
    assert_eq!(system_presets().len(), CANONICAL_IDS.len());
}

#[test]
fn every_preset_is_complete_in_three_locales() {
    for (id, name_es, name_en, name_pt_br) in NAMES {
        let preset = lookup(id);

        assert_eq!(preset.name_es, name_es, "{id} name_es");
        assert_eq!(preset.name_en, name_en, "{id} name_en");
        assert_eq!(preset.name_pt_br, name_pt_br, "{id} name_pt_br");

        for (label, text) in [
            ("description_es", &preset.description_es),
            ("description_en", &preset.description_en),
            ("description_pt_br", &preset.description_pt_br),
        ] {
            assert!(!text.trim().is_empty(), "{id} {label} must not be empty");
            assert!(
                text.chars().count() < 90,
                "{id} {label} must stay under 90 characters, got {}",
                text.chars().count()
            );
        }

        assert!(
            matches!(
                preset.category.as_str(),
                "tv" | "social" | "archive" | "audio"
            ),
            "{id} category must be one of the documented four, got `{}`",
            preset.category
        );
        assert_eq!(preset.level, None, "{id} must leave level unset");
        assert!(!preset.normalize_audio, "{id} must not normalize audio");
        assert_eq!(preset.preferred_audio_language, None, "{id}");
    }
}

#[test]
fn catalog_is_stable_across_calls() {
    let first: Vec<String> = system_presets().iter().map(|p| p.id.clone()).collect();
    let second: Vec<String> = system_presets().iter().map(|p| p.id.clone()).collect();
    assert_eq!(first, second, "repeated calls return the same catalog");

    let mut tampered = lookup("system-smarttv-compat").clone();
    tampered.container = String::from("mkv");
    tampered.faststart = false;

    assert_eq!(lookup("system-smarttv-compat").container, "mp4");
    assert!(lookup("system-smarttv-compat").faststart);
    assert_eq!(system_presets().len(), 13);
    assert_eq!(tampered.id, "system-smarttv-compat");
}

#[test]
fn hardware_preference_is_metadata_only() {
    const GPU_ENCODERS: [&str; 7] = [
        "h264_nvenc",
        "hevc_nvenc",
        "h264_vaapi",
        "hevc_vaapi",
        "h264_qsv",
        "hevc_qsv",
        "av1_nvenc",
    ];

    for preset in system_presets() {
        assert!(
            matches!(preset.hardware_preference.as_str(), "auto" | "cpu"),
            "`{}` declares an unknown hardware preference `{}`",
            preset.id,
            preset.hardware_preference
        );
        assert!(
            !GPU_ENCODERS.contains(&preset.video_codec.as_str()),
            "`{}` must not name a GPU encoder in v0.1",
            preset.id
        );
    }
}

// The one custom preset (`custom-1`, `docs/contracts.md` §2.3). It is not part of
// the immutable catalog, but it is defined here because these cases judge the
// definition of a preset, not a conversion.

/// A combination with no argument against it: MKV carries H.265, Opus and
/// subtitles, so every field is legal at the same time.
fn valid_custom() -> CustomPreset {
    CustomPreset {
        container: CustomContainer::Mkv,
        video: CustomVideo::H265,
        quality: CustomQuality::Level(CustomQualityLevel::High),
        resolution: CustomResolution::Original,
        audio: CustomAudio::Opus,
        bitrate: Some(CustomBitrate::K256),
        subtitles: CustomSubtitles::Keep,
    }
}

#[test]
fn custom_preset_rejects_each_incompatible_combination() {
    assert_eq!(
        valid_custom().validate(),
        Ok(()),
        "MKV + H.265 + Opus + subtítulos es una combinación legal"
    );

    // `mp4` does not carry VP9 in the canonical table (`presets.md`).
    let mut mp4_vp9 = valid_custom();
    mp4_vp9.container = CustomContainer::Mp4;
    mp4_vp9.video = CustomVideo::Vp9;
    let error = mp4_vp9.validate().expect_err("mp4 + vp9 must be rejected");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert!(
        error.message.contains("mp4") && error.message.contains("vp9"),
        "el mensaje nombra el contenedor y el códec: {}",
        error.message
    );

    // `webm` carries neither H.264 nor H.265.
    for video in [CustomVideo::H264, CustomVideo::H265] {
        let mut webm = valid_custom();
        webm.container = CustomContainer::Webm;
        webm.video = video;
        let error = webm
            .validate()
            .expect_err("webm must reject a codec it cannot carry");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    }

    // `webm` carries Opus only: no AAC, MP3 or FLAC.
    for audio in [CustomAudio::Aac, CustomAudio::Mp3, CustomAudio::Flac] {
        let mut webm = valid_custom();
        webm.container = CustomContainer::Webm;
        webm.video = CustomVideo::Vp9;
        webm.audio = audio;
        assert!(
            webm.validate().is_err(),
            "webm + {audio:?} must be rejected: el contenedor no transporta ese códec"
        );
    }

    // A bitrate without an audio track has nothing to describe.
    let mut silent_with_bitrate = valid_custom();
    silent_with_bitrate.audio = CustomAudio::None;
    silent_with_bitrate.subtitles = CustomSubtitles::Remove;
    let error = silent_with_bitrate
        .validate()
        .expect_err("audio = none with a bitrate must be rejected");
    assert!(
        error.message.contains("bitrate"),
        "el mensaje nombra el bitrate: {}",
        error.message
    );

    // A copied audio track keeps its own bitrate: declaring one is a contradiction.
    let mut copied_with_bitrate = valid_custom();
    copied_with_bitrate.audio = CustomAudio::Copy;
    let error = copied_with_bitrate
        .validate()
        .expect_err("audio = copy with a bitrate must be rejected");
    assert!(
        error.message.contains("bitrate"),
        "el mensaje nombra el bitrate: {}",
        error.message
    );

    // Keeping subtitles without an audio track or without a video stream leaves
    // them hanging: both are refused for the same reason.
    let mut no_audio = valid_custom();
    no_audio.audio = CustomAudio::None;
    no_audio.bitrate = None;
    assert!(
        no_audio.validate().is_err(),
        "audio = none with subtitles = keep must be rejected"
    );

    let mut no_video = valid_custom();
    no_video.video = CustomVideo::None;
    assert!(
        no_video.validate().is_err(),
        "video = none with subtitles = keep must be rejected"
    );

    // The documented legal matrix: every container with a codec it carries.
    let legal = [
        (CustomContainer::Mp4, CustomVideo::H264, CustomAudio::Aac),
        (CustomContainer::Mp4, CustomVideo::H265, CustomAudio::Mp3),
        (CustomContainer::Mp4, CustomVideo::Copy, CustomAudio::Copy),
        (CustomContainer::Mkv, CustomVideo::Vp9, CustomAudio::Flac),
        (CustomContainer::Mkv, CustomVideo::Av1, CustomAudio::Opus),
        (CustomContainer::Webm, CustomVideo::Vp9, CustomAudio::Opus),
        (CustomContainer::Webm, CustomVideo::Av1, CustomAudio::Copy),
        (CustomContainer::Webm, CustomVideo::Copy, CustomAudio::None),
    ];
    for (container, video, audio) in legal {
        let mut candidate = valid_custom();
        candidate.container = container;
        candidate.video = video;
        candidate.audio = audio;
        candidate.bitrate = match audio {
            CustomAudio::Aac | CustomAudio::Mp3 | CustomAudio::Flac | CustomAudio::Opus => {
                Some(CustomBitrate::K192)
            }
            CustomAudio::Copy | CustomAudio::None => None,
        };
        candidate.subtitles = match audio {
            CustomAudio::None => CustomSubtitles::Remove,
            _ => CustomSubtitles::Keep,
        };
        assert_eq!(
            candidate.validate(),
            Ok(()),
            "{container:?} + {video:?} + {audio:?} debe ser legal"
        );
    }
}

#[test]
fn custom_preset_quality_maps_to_crf() {
    const H264: [u8; 3] = [18, 23, 28];
    const H265: [u8; 3] = [20, 26, 30];
    const VP9: [u8; 3] = [24, 32, 38];
    const AV1: [u8; 3] = [24, 30, 36];
    let levels = [
        CustomQualityLevel::High,
        CustomQualityLevel::Medium,
        CustomQualityLevel::Low,
    ];

    for (index, level) in levels.iter().enumerate() {
        assert_eq!(
            CustomQuality::Level(*level).crf_for(CustomVideo::H264),
            Some(H264[index]),
            "{level:?} en libx264"
        );
        assert_eq!(
            CustomQuality::Level(*level).crf_for(CustomVideo::H265),
            Some(H265[index]),
            "{level:?} en libx265"
        );
        assert_eq!(
            CustomQuality::Level(*level).crf_for(CustomVideo::Vp9),
            Some(VP9[index]),
            "{level:?} en libvpx-vp9"
        );
        assert_eq!(
            CustomQuality::Level(*level).crf_for(CustomVideo::Av1),
            Some(AV1[index]),
            "{level:?} en libsvtav1"
        );
    }

    // An explicit CRF is the same number for every encoder.
    for video in [
        CustomVideo::H264,
        CustomVideo::H265,
        CustomVideo::Vp9,
        CustomVideo::Av1,
    ] {
        assert_eq!(CustomQuality::Crf(31).crf_for(video), Some(31));
    }
    // A stream that is copied or removed has no quality of its own.
    assert_eq!(CustomQuality::Crf(20).crf_for(CustomVideo::Copy), None);
    assert_eq!(
        CustomQuality::Level(CustomQualityLevel::High).crf_for(CustomVideo::None),
        None
    );

    // The mapped value is what `Preset::video_quality` carries.
    let mut medium = valid_custom();
    medium.video = CustomVideo::H265;
    medium.quality = CustomQuality::Level(CustomQualityLevel::Medium);
    assert_eq!(
        medium.to_preset().expect("valid").video_quality,
        Some(26),
        "`quality` llega al preset como CRF del codificador elegido"
    );
    let mut exact = valid_custom();
    exact.quality = CustomQuality::Crf(20);
    assert_eq!(exact.to_preset().expect("valid").video_quality, Some(20));
    let mut copied = valid_custom();
    copied.video = CustomVideo::Copy;
    assert_eq!(
        copied.to_preset().expect("valid").video_quality,
        None,
        "un video copiado no declara CRF"
    );
}

#[test]
fn custom_preset_rejects_unknown_values_at_deserialize() {
    let raw = |patch: serde_json::Value| {
        let mut base = serde_json::json!({
            "container": "mkv",
            "video": "h265",
            "quality": "high",
            "resolution": "original",
            "audio": "opus",
            "bitrate": 256,
            "subtitles": "keep",
        });
        for (key, value) in patch.as_object().expect("objeto").clone() {
            base[key] = value;
        }
        serde_json::from_value::<CustomPreset>(base)
    };

    // The baseline parses, so a rejection below is about the patched value.
    assert!(raw(serde_json::json!({})).is_ok());

    for patch in [
        serde_json::json!({"container": "avi"}),
        serde_json::json!({"video": "mpeg4"}),
        serde_json::json!({"quality": "ultra"}),
        serde_json::json!({"resolution": "4k"}),
        serde_json::json!({"audio": "vorbis"}),
        serde_json::json!({"bitrate": 96}),
        serde_json::json!({"subtitles": "burn"}),
        serde_json::json!({"quality": 52}),
        serde_json::json!({"quality": -1}),
        serde_json::json!({"bitrate": "256"}),
    ] {
        assert!(
            raw(patch.clone()).is_err(),
            "un valor fuera del contrato debe rechazarse al deserializar: {patch}"
        );
    }

    // An integer CRF inside 0..=51 and a named level are both accepted.
    assert_eq!(
        raw(serde_json::json!({"quality": 20}))
            .expect("crf explícito")
            .quality,
        CustomQuality::Crf(20)
    );
    assert_eq!(
        raw(serde_json::json!({"quality": "low"}))
            .expect("nivel con nombre")
            .quality,
        CustomQuality::Level(CustomQualityLevel::Low)
    );
    // No bitrate is the documented way to say "no bitrate applies".
    assert_eq!(
        raw(serde_json::json!({"bitrate": null}))
            .expect("bitrate nulo")
            .bitrate,
        None
    );
}
