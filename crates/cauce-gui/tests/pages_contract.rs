//! Contract of the Presets, Diagnóstico and Ajustes page cores
//! (`docs/ui-mvp.md` §6-§11, `docs/contracts.md` §2.3, §2.6 and §6).
//!
//! The three pages decide in plain functions with no GTK widget, so the grouping
//! of the catalog, the detail rows, the diagnostic report, the theme mapping and
//! the settings edit are all tested without a display. The widgets are only a
//! rendering of these answers. Every `Preset`, `Capabilities` and `Settings`
//! value comes from `cauce-core` and is built literally, never from a fixture.

use cauce_core::{
    Capabilities, DeviceFamily, DuplicatePolicy, HdrPolicy, Language, Preset, Settings,
    SubtitlePolicy, Theme, system_presets,
};
use cauce_gui::pages::diagnostics::{capabilities_report, human_bytes};
use cauce_gui::pages::presets::{group_presets, preset_detail};
use cauce_gui::pages::settings::{SettingsEdit, settings_after_edit, theme_from_str, theme_to_str};

fn settings() -> Settings {
    Settings {
        output_dir: "/home/wilkin/Vídeos/salida".to_owned(),
        max_concurrent_jobs: 2,
        theme: Theme::System,
        language: Language::Es,
        notifications: false,
        duplicate_policy: DuplicatePolicy::Suffix,
        keep_temp_on_failure: false,
    }
}

/// A literal preset with every optional field unset.
fn sparse_preset(category: &str) -> Preset {
    Preset {
        id: format!("system-{category}-sparse"),
        name_es: "Prueba".to_owned(),
        name_en: "Test".to_owned(),
        name_pt_br: "Teste".to_owned(),
        description_es: "Un preset de prueba.".to_owned(),
        description_en: "A test preset.".to_owned(),
        description_pt_br: "Um preset de teste.".to_owned(),
        category: category.to_owned(),
        device_family: DeviceFamily::Generic,
        container: "mkv".to_owned(),
        video_codec: "h265".to_owned(),
        audio_codec: "opus".to_owned(),
        video_quality: None,
        audio_bitrate: None,
        max_width: None,
        max_height: None,
        max_fps: None,
        pixel_format: None,
        profile: None,
        level: None,
        max_bit_depth: None,
        hdr_policy: HdrPolicy::Preserve,
        subtitle_policy: SubtitlePolicy::PreserveCompatible,
        faststart: false,
        preserve_metadata: true,
        hardware_preference: "cpu".to_owned(),
        audio_channels: None,
        normalize_audio: false,
        preferred_audio_language: None,
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        ffmpeg_version: "8.0.1".to_owned(),
        ffprobe_version: "8.0.1-3ubuntu2".to_owned(),
        encoders: vec![
            "libx264".to_owned(),
            "libx265".to_owned(),
            "libopus".to_owned(),
        ],
        filters: vec!["scale".to_owned(), "tonemap".to_owned()],
        cpu_cores: 6,
        // 250 GiB exactly, so the human unit is unambiguous.
        disk_free_bytes: 268_435_456_000,
        daemon_version: "0.1.0".to_owned(),
        ffmpeg_missing: Vec::new(),
        warnings: Vec::new(),
    }
}

#[test]
fn presets_are_grouped_in_the_documented_order() {
    let presets = system_presets();
    assert_eq!(presets.len(), 13, "el catálogo tiene trece presets");

    let groups = group_presets(presets);
    let categories: Vec<&str> = groups.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(categories, ["tv", "social", "archive", "audio"]);

    let total: usize = groups.iter().map(|(_, items)| items.len()).sum();
    assert_eq!(total, 13, "ningún preset se pierde al agrupar");
    assert_eq!(groups[0].1.len(), 5, "cinco presets de Smart TV");
    assert_eq!(groups[3].1.len(), 5, "cinco presets de audio");

    // Catalog order is preserved inside each group.
    let first_ids: Vec<&str> = groups[0]
        .1
        .iter()
        .map(|preset| preset.id.as_str())
        .collect();
    assert_eq!(
        first_ids,
        [
            "system-smarttv-compat",
            "system-smarttv-hevc",
            "system-samsung-tv",
            "system-lg-tv",
            "system-android-tv",
        ]
    );
}

#[test]
fn unknown_category_goes_last_alphabetically() {
    let mut presets: Vec<Preset> = system_presets().to_vec();
    presets.push(sparse_preset("zebra"));
    presets.push(sparse_preset("beta"));

    let groups = group_presets(&presets);
    let categories: Vec<&str> = groups.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        categories,
        ["tv", "social", "archive", "audio", "beta", "zebra"]
    );
}

#[test]
fn preset_detail_skips_unset_fields() {
    let sparse = sparse_preset("archive");
    let rows = preset_detail(&sparse);

    let labels: Vec<&str> = rows.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Nombre",
            "Descripción",
            "Contenedor",
            "Códec de video",
            "Códec de audio",
            "HDR",
            "Subtítulos",
            "Faststart",
            "Metadatos",
        ],
        "sin resolución, sin CRF, sin bitrate, sin FPS y sin bits no hay fila"
    );
    assert!(
        rows.iter().all(|(_, value)| !value.is_empty()),
        "ninguna fila queda vacía: {rows:?}"
    );

    // With limits set, the resolution row appears as an exact `ancho x alto`.
    let mut limited = sparse.clone();
    limited.max_width = Some(1920);
    limited.max_height = Some(1080);
    limited.video_quality = Some(24);
    limited.audio_bitrate = Some("160k".to_owned());
    limited.max_fps = Some(60.0);
    limited.max_bit_depth = Some(10);
    let rows = preset_detail(&limited);
    let find = |label: &str| {
        rows.iter()
            .find(|(name, _)| name == label)
            .map(|(_, value)| value.clone())
    };
    assert_eq!(find("Resolución máxima").as_deref(), Some("1920×1080"));
    assert_eq!(find("Calidad (CRF)").as_deref(), Some("24"));
    assert_eq!(find("Bitrate de audio").as_deref(), Some("160k"));
    assert_eq!(find("FPS máximos").as_deref(), Some("60"));
    assert_eq!(find("Profundidad de bits").as_deref(), Some("10 bits"));
}

#[test]
fn human_bytes_uses_gib_and_mib() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
    assert_eq!(human_bytes(536_870_912), "512.0 MiB");
    assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GiB");
    assert_eq!(human_bytes(2_684_354_560), "2.5 GiB");
}

#[test]
fn capabilities_report_lists_every_fact() {
    let report = capabilities_report(&capabilities());
    let mut expected = [
        "FFmpeg: 8.0.1",
        "FFprobe: 8.0.1-3ubuntu2",
        "Encoders: 3",
        "Filtros: 2",
        "Núcleos de CPU: 6",
        "Disco libre: 250.0 GiB",
        "Daemon: 0.1.0",
    ]
    .join("\n");
    expected.push('\n');
    assert_eq!(
        report, expected,
        "una línea por hecho, en orden estable y con salto final"
    );

    // The missing list only appears when something is actually missing.
    let mut incomplete = capabilities();
    incomplete.ffmpeg_missing = vec!["libsvtav1".to_owned(), "zscale".to_owned()];
    incomplete.warnings = vec!["Instalá un FFmpeg completo.".to_owned()];
    let report = capabilities_report(&incomplete);
    assert!(report.contains("Daemon: 0.1.0\n"), "el resto sigue ahí");
    assert!(report.contains("\nFalta libsvtav1\n"), "{report}");
    assert!(report.contains("\nFalta zscale\n"), "{report}");
    assert!(
        report.contains("\nAviso: Instalá un FFmpeg completo.\n"),
        "{report}"
    );
}

#[test]
fn theme_round_trips_through_its_string() {
    for value in ["system", "light", "dark"] {
        let scheme = theme_from_str(value);
        assert_eq!(theme_to_str(scheme), value, "ida y vuelta de {value}");
    }

    // The live manager gets exactly the scheme `docs/ui-mvp.md` §8 names.
    assert_eq!(theme_from_str("system"), libadwaita::ColorScheme::Default);
    assert_eq!(theme_from_str("light"), libadwaita::ColorScheme::ForceLight);
    assert_eq!(theme_from_str("dark"), libadwaita::ColorScheme::ForceDark);
}

#[test]
fn settings_edit_changes_only_the_edited_field() {
    let base = settings();

    let edited = settings_after_edit(&base, SettingsEdit::MaxConcurrentJobs(5));
    assert_eq!(edited.max_concurrent_jobs, 5);
    assert_eq!(edited.output_dir, base.output_dir);
    assert_eq!(edited.theme, base.theme);
    assert_eq!(edited.language, base.language);
    assert_eq!(edited.notifications, base.notifications);
    assert_eq!(edited.duplicate_policy, base.duplicate_policy);
    assert_eq!(edited.keep_temp_on_failure, base.keep_temp_on_failure);
    assert_eq!(base, settings(), "el original no se muta");

    let edited = settings_after_edit(&base, SettingsEdit::Theme(Theme::Dark));
    assert_eq!(edited.theme, Theme::Dark);
    assert_eq!(edited.max_concurrent_jobs, base.max_concurrent_jobs);
    assert_eq!(edited.language, base.language);

    let edited = settings_after_edit(&base, SettingsEdit::Language(Language::PtBr));
    assert_eq!(edited.language, Language::PtBr);
    assert_eq!(edited.theme, base.theme);

    let edited = settings_after_edit(&base, SettingsEdit::DuplicatePolicy(DuplicatePolicy::Skip));
    assert_eq!(edited.duplicate_policy, DuplicatePolicy::Skip);
    assert_eq!(edited.output_dir, base.output_dir);

    let edited = settings_after_edit(&base, SettingsEdit::Notifications(true));
    assert!(edited.notifications);
    assert_eq!(edited.keep_temp_on_failure, base.keep_temp_on_failure);

    let edited = settings_after_edit(&base, SettingsEdit::KeepTempOnFailure(true));
    assert!(edited.keep_temp_on_failure);
    assert!(!edited.notifications);

    let edited = settings_after_edit(
        &base,
        SettingsEdit::OutputDir("/home/wilkin/Vídeos/otra".to_owned()),
    );
    assert_eq!(edited.output_dir, "/home/wilkin/Vídeos/otra");
    assert_eq!(edited.max_concurrent_jobs, base.max_concurrent_jobs);
}

#[test]
fn settings_edit_rejects_an_out_of_range_concurrency() {
    let base = settings();
    for rejected in [0, 9, u8::MAX] {
        let result = settings_after_edit(&base, SettingsEdit::MaxConcurrentJobs(rejected));
        assert_eq!(
            result, base,
            "{rejected} queda fuera de 1..=8: el resultado es el estado previo"
        );
    }
}
