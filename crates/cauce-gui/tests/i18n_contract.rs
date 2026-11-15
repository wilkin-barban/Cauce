//! Contract of the ES/EN/PT-BR catalogs (`S8`, `docs/contracts.md` §2.6 and
//! `docs/presets.md`).
//!
//! Every assertion here is about data, not about GTK: the three catalogs must
//! cover the same keys, a miss must fall back to English instead of panicking, a
//! miss in English must show the key itself, and the ratified preset name columns
//! must be the only source of a preset name. The process-wide locale is never
//! touched, so the tests stay independent of each other and of the GUI.

use cauce_gui::i18n::{preset_description, preset_name};
use cauce_gui::pages::SECTIONS;
use cauce_gui::pages::queue::state::{STATES, state_key};
use cauce_i18n::{Catalog, Locale, catalog, keys, missing_in, translate};

#[test]
fn catalogs_cover_the_same_keys() {
    let all = keys();
    assert!(!all.is_empty(), "los catálogos no están vacíos");

    let mut union: Vec<&'static str> = Vec::new();
    for locale in Locale::ALL {
        for (key, _) in catalog(locale).entries() {
            union.push(key);
        }
    }
    union.sort_unstable();
    union.dedup();
    assert_eq!(
        all,
        union.as_slice(),
        "`keys()` es exactamente la unión ordenada de las tres tablas"
    );

    for locale in Locale::ALL {
        assert!(
            missing_in(locale).is_empty(),
            "{locale:?} tiene claves sin traducir: {:?}",
            missing_in(locale)
        );
    }
}

#[test]
fn pt_br_uses_the_brazilian_tag() {
    assert_eq!(Locale::PtBr.tag(), "pt-BR");
    assert_eq!(Locale::from_tag("pt-BR"), Some(Locale::PtBr));
    assert_eq!(
        Locale::from_tag("pt-br"),
        None,
        "la etiqueta del contrato lleva la región en mayúsculas"
    );
    for locale in Locale::ALL {
        assert_eq!(
            Locale::from_tag(locale.tag()),
            Some(locale),
            "ida y vuelta de {:?}",
            locale.tag()
        );
        assert_ne!(locale.tag(), "pt-br");
    }
    assert_eq!(
        Locale::ALL.map(Locale::tag),
        ["es", "en", "pt-BR"],
        "los tres tags del contrato, en orden"
    );
}

#[test]
fn unknown_key_falls_back_to_english() {
    // Two ad-hoc catalogs: the shipped ones are complete by construction, so the
    // fallback rule is proven here instead of by a hole in a real locale.
    let spanish = Catalog::new(&[("convert.title", "Convertir")]);
    let english = Catalog::new(&[("convert.title", "Convert"), ("queue.title", "Queue")]);

    assert_eq!(
        spanish.resolve(&english, "queue.title"),
        "Queue",
        "una clave que solo existe en inglés devuelve el texto inglés"
    );
    assert_eq!(
        spanish.resolve(&english, "convert.title"),
        "Convertir",
        "el acierto exacto gana sobre el respaldo"
    );
}

#[test]
fn missing_key_returns_the_key() {
    const ABSENT: &str = "no.such.key.exists";
    assert_eq!(translate(Locale::Es, ABSENT), ABSENT);
    assert_eq!(translate(Locale::En, ABSENT), ABSENT);
    assert_eq!(translate(Locale::PtBr, ABSENT), ABSENT);
}

#[test]
fn every_sidebar_section_is_translated() {
    let ids: Vec<&str> = SECTIONS.iter().map(|section| section.id).collect();
    assert_eq!(
        ids,
        ["convert", "queue", "presets", "diagnostics", "settings"],
        "las cinco secciones de `docs/ui-mvp.md` §2"
    );

    for section in SECTIONS.iter() {
        for locale in Locale::ALL {
            let title = translate(locale, section.title_key);
            assert_ne!(
                title, section.title_key,
                "«{}» tiene título en {locale:?}",
                section.id
            );
            assert!(!title.trim().is_empty(), "«{}» no está vacío", section.id);

            let description = translate(locale, section.description_key);
            assert_ne!(
                description, section.description_key,
                "«{}» tiene descripción en {locale:?}",
                section.id
            );
        }
    }
}

#[test]
fn every_job_state_has_a_name() {
    assert_eq!(STATES.len(), 10, "los diez estados de `contracts.md` §4");
    for state in STATES {
        let key = state_key(state);
        for locale in Locale::ALL {
            let name = translate(locale, key);
            assert_ne!(name, key, "«{}» se traduce en {locale:?}", state.as_str());
            assert!(
                !name.trim().is_empty(),
                "«{}» no está vacío",
                state.as_str()
            );
        }
    }
}

#[test]
fn preset_names_follow_the_locale() {
    let presets = cauce_core::system_presets();
    let preset = presets
        .iter()
        .find(|preset| preset.id == "system-smarttv-compat")
        .expect("el preset está en el catálogo del sistema");

    // The three ratified columns of `docs/presets.md`, literally.
    assert_eq!(
        preset_name(preset, Locale::Es),
        "Smart TV — Máxima Compatibilidad"
    );
    assert_eq!(preset_name(preset, Locale::En), "Maximum Compatibility");
    assert_eq!(
        preset_name(preset, Locale::PtBr),
        "Smart TV — Compatibilidade Máxima"
    );

    assert_eq!(
        preset_description(preset, Locale::Es),
        preset.description_es
    );
    assert_eq!(
        preset_description(preset, Locale::En),
        preset.description_en
    );
    assert_eq!(
        preset_description(preset, Locale::PtBr),
        preset.description_pt_br
    );

    // No preset borrows another locale's column.
    for preset in presets.iter() {
        assert_eq!(preset_name(preset, Locale::PtBr), preset.name_pt_br);
        assert_eq!(preset_name(preset, Locale::En), preset.name_en);
        assert_eq!(preset_name(preset, Locale::Es), preset.name_es);
    }
}
