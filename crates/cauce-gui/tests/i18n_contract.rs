//! Contract of the ES/EN/PT-BR catalogs (`S8`, `docs/contracts.md` §2.6 and
//! `docs/presets.md`).
//!
//! Every assertion here is about data, not about GTK: the three catalogs must
//! cover the same keys, a miss must fall back to English instead of panicking, a
//! miss in English must show the key itself, and the ratified preset name columns
//! must be the only source of a preset name. The process-wide locale is never
//! touched, so the tests stay independent of each other and of the GUI.

use cauce_core::ErrorCode;
use cauce_gui::i18n::{
    ERROR_GENERIC, error_code, error_key, human_error, human_error_from_key, human_error_in,
    preset_description, preset_name, technical_details,
};
use cauce_gui::pages::SECTIONS;
use cauce_gui::pages::queue::state::{STATES, state_key};
use cauce_i18n::{Catalog, Locale, catalog, keys, missing_in, translate};

/// The thirteen codes of `docs/contracts.md` §8, in the document's order.
const CODES: [ErrorCode; 13] = [
    ErrorCode::InvalidInput,
    ErrorCode::PathDenied,
    ErrorCode::FfmpegMissing,
    ErrorCode::FfmpegFeatureMissing,
    ErrorCode::ProbeFailed,
    ErrorCode::PlanInvalid,
    ErrorCode::OutputCollision,
    ErrorCode::OutputValidationFailed,
    ErrorCode::ProcessFailed,
    ErrorCode::Cancelled,
    ErrorCode::DaemonUnavailable,
    ErrorCode::DatabaseBusy,
    ErrorCode::Internal,
];

/// The raw message a daemon would attach to one of these codes.
const TECHNICAL: &str = "ffmpeg exited with status 1: /home/wilkin/in.mkv";

/// Every token that shouts, as `INVALID_INPUT` or a bare `PROCESS_FAILED`.
fn screaming_tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .filter(|token| {
            token.contains('_')
                || (token.len() >= 3
                    && token
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        })
        .collect()
}

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

#[test]
fn every_error_code_has_a_human_message() {
    assert_eq!(
        CODES.len(),
        13,
        "los trece códigos de `docs/contracts.md` §8"
    );

    let generic = translate(Locale::Es, ERROR_GENERIC);
    let mut keys_seen: Vec<&'static str> = Vec::new();
    let mut sentences: Vec<String> = Vec::new();
    for code in CODES {
        let key = error_key(code);
        assert!(
            !keys_seen.contains(&key),
            "«{key}» repite la clave de otro código"
        );
        keys_seen.push(key);

        for locale in Locale::ALL {
            let sentence = human_error_in(locale, code, TECHNICAL);
            assert!(
                !sentence.trim().is_empty(),
                "{code:?} no tiene oración en {locale:?}"
            );
            assert!(
                !sentence.contains(TECHNICAL),
                "el mensaje técnico no es la oración visible de {code:?} en {locale:?}: {sentence:?}"
            );
            assert!(
                sentence.chars().count() >= 20,
                "la oración de {code:?} en {locale:?} tiene que ser una frase: {sentence:?}"
            );
            if locale == Locale::Es && code != ErrorCode::Internal {
                assert_ne!(
                    sentence, generic,
                    "{code:?} necesita su propia oración y no la genérica"
                );
            }
            if locale == Locale::Es {
                sentences.push(sentence);
            }
        }
    }
    sentences.dedup();
}

#[test]
fn unknown_code_falls_back() {
    // Un código que ningún catálogo conoce: la oración genérica, nunca la clave
    // cruda y nunca el texto del daemon.
    let fallback = human_error_from_key("error.no_such_code", TECHNICAL);
    assert_eq!(
        fallback,
        translate(Locale::Es, ERROR_GENERIC),
        "un código sin mensaje cae en la oración genérica"
    );
    assert!(
        !fallback.contains("no_such_code") && !fallback.contains(TECHNICAL),
        "el respaldo tampoco muestra la clave ni el detalle técnico: {fallback:?}"
    );

    // Sin detalle técnico la oración genérica sigue siendo una frase completa.
    let bare = human_error_from_key("error.no_such_code", "");
    assert_eq!(bare, fallback, "el respaldo no depende del detalle técnico");
    assert!(!bare.trim().is_empty(), "el respaldo nunca queda vacío");

    // `human_error` usa el mismo camino para los trece códigos reales.
    for code in CODES {
        assert_eq!(
            human_error(code, TECHNICAL),
            human_error_in(Locale::Es, code, TECHNICAL),
            "`human_error` y `human_error_in(Es, …)` coinciden para {code:?}"
        );
    }
}

#[test]
fn no_screaming_snake_case_reaches_the_ui() {
    for code in CODES {
        for locale in Locale::ALL {
            let sentence = human_error_in(locale, code, TECHNICAL);
            assert!(
                screaming_tokens(&sentence).is_empty(),
                "{code:?} en {locale:?} muestra un código en mayúsculas: {sentence:?}"
            );
            assert!(
                !sentence.contains('_'),
                "la oración de {code:?} en {locale:?} no lleva guiones bajos: {sentence:?}"
            );
            // El código sigue existiendo, pero solo dentro del desplegable.
            assert!(
                technical_details(code, TECHNICAL).contains(error_code(code)),
                "el detalle técnico conserva el código de {code:?}"
            );
        }
    }
}

#[test]
fn the_technical_detail_is_only_in_the_disclosure() {
    for code in CODES {
        let details = technical_details(code, TECHNICAL);
        assert!(
            details.contains(TECHNICAL),
            "el desplegable conserva el mensaje del daemon de {code:?}: {details:?}"
        );
        assert!(
            screaming_tokens(&details).len() == 1,
            "el desplegable lleva el código y nada más gritado: {details:?}"
        );
        assert_ne!(
            technical_details(code, ""),
            TECHNICAL,
            "sin mensaje del daemon el desplegable igual nombra el código de {code:?}"
        );
    }

    // La etiqueta del desplegable existe en los tres idiomas.
    for locale in Locale::ALL {
        let label = translate(locale, "error.technical_details");
        assert_ne!(
            label, "error.technical_details",
            "«Detalles técnicos» está traducido en {locale:?}"
        );
        assert!(
            !label.trim().is_empty(),
            "la etiqueta del desplegable no está vacía en {locale:?}"
        );
    }
}

#[test]
fn the_spanish_messages_say_what_to_do() {
    for (code, expected) in [
        (
            ErrorCode::FfmpegMissing,
            "No encontramos FFmpeg en este equipo. Instalalo con «sudo apt install ffmpeg» y reintentá.",
        ),
        (
            ErrorCode::PathDenied,
            "Esa ruta está fuera de las carpetas que Cauce puede usar. Elegí un archivo o una carpeta dentro de tus documentos.",
        ),
        (
            ErrorCode::OutputCollision,
            "Ya existe un archivo con ese nombre en la carpeta de destino. Elegí otra política de duplicados o cambiá la carpeta de salida.",
        ),
        (
            ErrorCode::DaemonUnavailable,
            "No podemos hablar con el servicio de Cauce. Verificá que esté activo y volvé a intentarlo.",
        ),
        (
            ErrorCode::Cancelled,
            "El trabajo se canceló. Si todavía querés el archivo, volvé a encolarlo.",
        ),
    ] {
        assert_eq!(
            human_error_in(Locale::Es, code, TECHNICAL),
            expected,
            "el texto en español de {code:?} es el ratificado"
        );
    }
}
