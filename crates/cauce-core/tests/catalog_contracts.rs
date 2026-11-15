//! Catalog-wide invariants of the thirteen system presets.
//!
//! Domain-specific expectations live in `catalog_video_contracts.rs` and
//! `catalog_audio_contracts.rs`; this file pins the catalog as a whole:
//! identity, locales, immutability and the v0.1 CPU-only encoder rule.

use cauce_core::{Preset, system_presets};

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
