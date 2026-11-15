//! Contract of the Presets, Diagnóstico and Ajustes page cores
//! (`docs/ui-mvp.md` §6-§11, `docs/contracts.md` §2.3, §2.6 and §6).
//!
//! The three pages decide in plain functions with no GTK widget, so the grouping
//! of the catalog, the detail rows, the diagnostic report, the theme mapping and
//! the settings edit are all tested without a display. The widgets are only a
//! rendering of these answers. Every `Preset`, `Capabilities` and `Settings`
//! value comes from `cauce-core` and is built literally, never from a fixture.

use cauce_core::{
    Capabilities, CustomAudio, CustomBitrate, CustomContainer, CustomSubtitles, CustomVideo,
    DeviceFamily, DuplicatePolicy, ErrorCode, HdrPolicy, Language, Preset, Settings,
    SubtitlePolicy, Theme, system_presets,
};
use cauce_gui::pages::convert::flows::preset_choices;
use cauce_gui::pages::diagnostics::{capabilities_report, human_bytes};
use cauce_gui::pages::presets::{
    CUSTOM_CATEGORY, CustomDraft, PresetFilter, category_label, custom_editor_heading,
    custom_headers, custom_row_label, filter_presets, filters_in_order, group_presets,
    preset_detail, preset_essentials,
};
use cauce_gui::pages::settings::{
    SettingsEdit, edit_params, output_dir_to_show, settings_after_edit, theme_from_str,
    theme_to_str,
};

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

/// The compact editor's default draft: a legal MP4 with H.264 and AAC.
fn custom_draft() -> CustomDraft {
    CustomDraft::defaults()
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
            "Descripción",
            "Contenedor",
            "Códec de video",
            "Códec de audio",
            "Calidad (CRF)",
            "Resolución máxima",
            "HDR",
            "Subtítulos",
            "Faststart",
            "Metadatos",
        ],
        "el título del panel ya muestra el nombre: sin Nombre duplicado, sin bitrate, FPS \
         ni bits no hay fila, y las cinco esenciales siempre están"
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

// The navigator above the list: six chips, one per group, in the documented
// order, and each one filters the catalog to its own group.

#[test]
fn filters_are_in_the_documented_order() {
    assert_eq!(
        filters_in_order(),
        [
            PresetFilter::All,
            PresetFilter::Tv,
            PresetFilter::Web,
            PresetFilter::Archive,
            PresetFilter::Audio,
            PresetFilter::Custom,
        ],
        "Todas, Smart TV, Web, Archivo, Audio, Personalizado: `Personalizado` existe \
         desde la primera pantalla y no al final de un scroll"
    );

    let labels: Vec<String> = filters_in_order()
        .iter()
        .map(|filter| filter.label())
        .collect();
    assert_eq!(
        labels,
        [
            "Todas",
            "Smart TV",
            "Web",
            "Archivo",
            "Audio",
            "Personalizado"
        ],
        "cada chip se lee en el idioma vigente"
    );
}

#[test]
fn custom_filter_selects_only_the_custom_preset() {
    let mut presets: Vec<Preset> = system_presets().to_vec();
    presets.push(custom_draft().to_custom().to_preset().expect("legal"));

    let custom = filter_presets(&presets, PresetFilter::Custom);
    assert_eq!(custom.len(), 1, "el grupo propio tiene un solo preset");
    assert_eq!(custom[0].id, "custom-1");
    assert_eq!(custom[0].category, CUSTOM_CATEGORY);

    // Ninguno de los trece del sistema cae en «Personalizado».
    assert!(
        custom
            .iter()
            .all(|preset| !preset.id.starts_with("system-")),
        "el sistema no se mezcla con el preset propio: {custom:?}"
    );
}

#[test]
fn all_filter_shows_thirteen_plus_custom() {
    let system = system_presets().to_vec();
    assert_eq!(system.len(), 13);
    let mut presets = system.clone();
    presets.push(custom_draft().to_custom().to_preset().expect("legal"));

    let all = filter_presets(&presets, PresetFilter::All);
    assert_eq!(all.len(), 14, "trece del sistema y el propio");
    assert_eq!(all.len(), system.len() + 1);

    // Cada grupo aporta su parte y entre todos suman el catálogo completo.
    let groups = [
        PresetFilter::Tv,
        PresetFilter::Web,
        PresetFilter::Archive,
        PresetFilter::Audio,
        PresetFilter::Custom,
    ];
    let total: usize = groups
        .iter()
        .map(|filter| filter_presets(&presets, *filter).len())
        .sum();
    assert_eq!(total, all.len(), "un chip por grupo, sin presets perdidos");
    for filter in groups {
        assert!(
            filter_presets(&presets, filter)
                .iter()
                .all(|preset| all.iter().any(|shown| shown.id == preset.id)),
            "«{}» filtra el catálogo, nunca lo inventa",
            filter.label()
        );
    }
}

// The five essentials of the detail panel, always in the same order.

#[test]
fn preset_essentials_lists_the_five_fields_in_order() {
    let preset = system_presets()[0].clone();
    let rows = preset_essentials(&preset);

    let labels: Vec<&str> = rows.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Contenedor",
            "Códec de video",
            "Códec de audio",
            "Calidad (CRF)",
            "Resolución máxima",
        ],
        "las cinco filas esenciales, en el orden en que se leen de un vistazo"
    );
    assert!(
        rows.iter().all(|(_, value)| !value.trim().is_empty()),
        "ninguna fila esencial queda vacía: {rows:?}"
    );
    // El contenedor y la resolución son valores, no claves del catálogo.
    assert_eq!(rows[0].1, preset.container);
    assert_eq!(rows[3].1, preset.video_quality.expect("CRF").to_string());
}

#[test]
fn preset_essentials_omits_missing_values() {
    let mut draft = custom_draft();
    draft.video = CustomVideo::None;
    draft.subtitles = CustomSubtitles::Remove;
    let custom = draft.to_custom().to_preset().expect("legal");
    assert_eq!(custom.video_codec, "none");
    assert_eq!(custom.video_quality, None);

    let rows = preset_essentials(&custom);
    let find = |label: &str| {
        rows.iter()
            .find(|(name, _)| name == label)
            .map(|(_, value)| value.clone())
    };
    // Un valor que falta no deja la fila vacía ni la esconde: la dice en palabras.
    assert_eq!(rows.len(), 5, "las cinco filas siguen ahí: {rows:?}");
    assert!(
        rows.iter().all(|(_, value)| !value.trim().is_empty()),
        "nada se muestra como una fila vacía: {rows:?}"
    );
    assert_eq!(find("Códec de video").as_deref(), Some("Sin video"));
    assert_eq!(find("Calidad (CRF)").as_deref(), Some("Sin CRF"));
    assert_eq!(find("Resolución máxima").as_deref(), Some("Original"));
}

#[test]
fn settings_after_convertir_choice_is_the_one_ajustes_shows() {
    let base = settings();
    let chosen = "/home/wilkin/Vídeos/elegida";

    // Sin una elección en esta sesión, Ajustes enseña lo persistido.
    assert_eq!(
        output_dir_to_show(&base),
        base.output_dir,
        "sin elección, Ajustes muestra la carpeta guardada"
    );

    // Convertir fija la carpeta en el valor que ambas páginas leen (`L43`).
    cauce_gui::pages::set_output_dir(chosen);
    assert_eq!(
        cauce_gui::pages::output_dir().as_deref(),
        Some(chosen),
        "la carpeta elegida queda en el valor compartido"
    );
    assert_eq!(
        output_dir_to_show(&base),
        chosen,
        "Ajustes muestra la carpeta que fijó Convertir, no la instantánea vieja del daemon"
    );

    // La elección se persiste con `settings.set` y sin arrastrar otros campos.
    let after = settings_after_edit(&base, SettingsEdit::OutputDir(chosen.to_owned()));
    assert_eq!(after.output_dir, chosen);
    assert_eq!(after.max_concurrent_jobs, base.max_concurrent_jobs);
    assert_eq!(
        edit_params(&SettingsEdit::OutputDir(chosen.to_owned())),
        serde_json::json!({ "output_dir": chosen }),
        "`settings.set` recibe solo la carpeta"
    );

    // Un valor vacío nunca borra la carpeta que ya se conoce.
    cauce_gui::pages::set_output_dir("   ");
    assert_eq!(
        cauce_gui::pages::output_dir().as_deref(),
        Some(chosen),
        "un valor vacío no sustituye la carpeta elegida"
    );
    assert_eq!(output_dir_to_show(&after), chosen);
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

// The explicit categories of the Presets page and the one custom preset
// (`docs/ui-mvp.md` §6, `docs/contracts.md` §2.3 and §6).

#[test]
fn categories_are_grouped_and_navigable() {
    let presets = system_presets();
    let groups = group_presets(presets);

    // Four explicit groups, each with a translated header, and the custom slot.
    let categories: Vec<&str> = groups.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(categories, ["tv", "social", "archive", "audio"]);
    for (category, items) in &groups {
        assert!(!items.is_empty(), "«{category}» no puede estar vacío");
        let label = category_label(category);
        assert!(!label.trim().is_empty(), "«{category}» necesita cabecera");
        assert_ne!(label, *category, "«{category}» se traduce");
    }
    assert_eq!(category_label(CUSTOM_CATEGORY), "Personalizado");

    // Navigable: every preset is reachable in exactly one group, and each group
    // keeps the catalog order it was given.
    let reached: Vec<&str> = groups
        .iter()
        .flat_map(|(_, items)| items.iter().map(|preset| preset.id.as_str()))
        .collect();
    assert_eq!(reached.len(), presets.len(), "ningún preset se duplica");
    for (category, items) in &groups {
        let expected: Vec<&str> = presets
            .iter()
            .filter(|preset| preset.category == *category)
            .map(|preset| preset.id.as_str())
            .collect();
        let actual: Vec<&str> = items.iter().map(|preset| preset.id.as_str()).collect();
        assert_eq!(
            actual, expected,
            "«{category}» conserva el orden del catálogo"
        );
    }
    assert!(
        reached.contains(&"system-smarttv-compat"),
        "un preset concreto es alcanzable en su grupo"
    );
}

#[test]
fn custom_group_is_last() {
    let mut presets: Vec<Preset> = system_presets().to_vec();
    presets.push(custom_draft().to_custom().to_preset().expect("legal"));

    let groups = group_presets(&presets);
    let (last_category, last_items) = groups.last().expect("hay grupos");
    assert_eq!(
        last_category, CUSTOM_CATEGORY,
        "«Personalizado» va al final"
    );
    assert_eq!(category_label(last_category), "Personalizado");
    assert_eq!(last_items.len(), 1, "un solo preset personalizado");
    assert_eq!(last_items[0].id, "custom-1");
    assert_eq!(
        groups
            .iter()
            .flat_map(|(_, items)| items.iter())
            .filter(|preset| preset.id == "custom-1")
            .count(),
        1,
        "el personalizado no se repite en otro grupo"
    );
}

#[test]
fn custom_detail_has_one_title_and_one_description() {
    // The panel of the custom group is fed by exactly two heading sources: its own
    // header and the editor's preferences group. Only the first one may speak.
    let (panel_title, panel_description) = custom_headers();
    let (editor_title, editor_description) = custom_editor_heading();
    let titles: Vec<&str> = [panel_title, editor_title].into_iter().flatten().collect();
    let descriptions: Vec<&str> = [panel_description, editor_description]
        .into_iter()
        .flatten()
        .collect();

    assert_eq!(
        titles,
        ["Mi preset"],
        "el panel del preset propio tiene un solo título"
    );
    assert_eq!(
        descriptions,
        ["Un solo preset propio: al guardar reemplaza al anterior."],
        "y una sola descripción"
    );
    assert_ne!(titles[0], descriptions[0], "el título no repite la frase");

    // The group name lives on the chip and on the category header; the row and
    // the panel title say the preset is the user's own.
    let chip = PresetFilter::Custom.label();
    let header = category_label(CUSTOM_CATEGORY);
    let row = custom_row_label();
    assert_eq!(chip, header, "el chip conserva el nombre del grupo");
    assert_eq!(row, titles[0], "la fila y el título dicen lo mismo");
    assert_ne!(row, header, "la fila no repite la cabecera");
}

#[test]
fn saving_a_custom_preset_lists_it_in_convert() {
    // The editor's draft is what `presets.set_custom` receives.
    let mut draft = custom_draft();
    draft.container = CustomContainer::Mp4;
    draft.video = CustomVideo::H264;
    draft.audio = CustomAudio::Aac;
    draft.bitrate = CustomBitrate::K192;
    let custom = draft.to_custom();
    assert_eq!(
        custom.validate(),
        Ok(()),
        "el borrador por defecto es legal"
    );

    // The saved preset is what `presets.list` answers and Convertir renders.
    let mut listed: Vec<Preset> = system_presets().to_vec();
    listed.push(custom.to_preset().expect("legal"));
    let choices = preset_choices(&listed);
    assert_eq!(choices.len(), 14, "trece del sistema y el propio");
    assert_eq!(
        choices.last().map(String::as_str),
        Some("Personalizado"),
        "el combo de Convertir muestra el preset guardado"
    );
    assert_eq!(
        listed[choices.len() - 1].id,
        "custom-1",
        "el índice del combo apunta al preset personalizado"
    );

    // A draft read back from the saved preset reproduces the same choices.
    let reopened = CustomDraft::from_preset(&listed[13]);
    assert_eq!(reopened, draft, "el editor reabre con lo que se guardó");
}

#[test]
fn an_invalid_custom_draft_names_the_reason() {
    let mut draft = custom_draft();
    draft.container = CustomContainer::Webm;
    draft.video = CustomVideo::H264;
    let error = draft
        .to_custom()
        .validate()
        .expect_err("WebM no transporta H.264");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(
        error.message.contains("webm"),
        "el motivo nombra el contenedor: {}",
        error.message
    );
}
