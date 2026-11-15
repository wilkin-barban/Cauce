//! The process-wide locale of the GUI and the two ways the interface reads it
//! (`S8`, `docs/contracts.md` §2.6).
//!
//! The catalogs themselves live in `cauce-i18n`; this module only remembers which
//! one is current and lets the shell rebuild itself when it changes. Three rules:
//!
//! - [`t`] is the only way a widget gets text, so no Spanish sentence is compiled
//!   into the GUI and a miss shows the key instead of a blank label.
//! - [`set_locale`] is the only writer. It notifies the registered listener only
//!   when the value really changed, which is what keeps a settings re-render from
//!   rebuilding the pages in a loop.
//! - Preset names and descriptions come from the ratified columns of `Preset`
//!   (`docs/presets.md`), never from the catalogs.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use cauce_core::{HdrPolicy, Preset, SubtitlePolicy};
use cauce_i18n::Locale;

/// Spanish is the default until the daemon says otherwise.
static LOCALE: OnceLock<Mutex<Locale>> = OnceLock::new();

thread_local! {
    /// The GTK thread owns the shell, so the rebuild hook never needs `Send`.
    static LISTENER: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

fn slot() -> &'static Mutex<Locale> {
    LOCALE.get_or_init(|| Mutex::new(Locale::Es))
}

/// The locale every widget renders with.
pub fn locale() -> Locale {
    match slot().lock() {
        Ok(guard) => *guard,
        // A poisoned lock still holds a valid `Locale`: the copy is `Copy`, so a
        // panic elsewhere never turns the interface into a crash here.
        Err(poisoned) => *poisoned.into_inner(),
    }
}

/// Changes the locale and, when it really changed, asks the shell to repaint.
pub fn set_locale(locale: Locale) {
    let changed = {
        let mut guard = match slot().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let changed = *guard != locale;
        *guard = locale;
        changed
    };
    if changed {
        notify();
    }
}

/// The text of `key` in the current locale (`cauce-i18n` owns the fallback).
pub fn t(key: &'static str) -> &'static str {
    cauce_i18n::translate(locale(), key)
}

/// Replaces `{name}` with `value` in the text of `key`, for the few messages that
/// name a job, a path or a count.
pub fn t_with(key: &'static str, name: &str, value: &str) -> String {
    t(key).replace(name, value)
}

/// Registers the shell's repaint hook; the last registration wins.
pub fn on_locale_change(listener: Rc<dyn Fn()>) {
    LISTENER.with(|slot| *slot.borrow_mut() = Some(listener));
}

fn notify() {
    let listener = LISTENER.with(|slot| slot.borrow().clone());
    if let Some(listener) = listener {
        listener();
    }
}

/// The ratified Spanish, English or Brazilian name of `preset`.
pub fn preset_name(preset: &Preset, locale: Locale) -> &str {
    match locale {
        Locale::Es => preset.name_es.as_str(),
        Locale::En => preset.name_en.as_str(),
        Locale::PtBr => preset.name_pt_br.as_str(),
    }
}

/// The ratified description that goes with [`preset_name`].
pub fn preset_description(preset: &Preset, locale: Locale) -> &str {
    match locale {
        Locale::Es => preset.description_es.as_str(),
        Locale::En => preset.description_en.as_str(),
        Locale::PtBr => preset.description_pt_br.as_str(),
    }
}

/// [`preset_name`] in the current locale.
pub fn localized_preset_name(preset: &Preset) -> &str {
    preset_name(preset, locale())
}

/// [`preset_description`] in the current locale.
pub fn localized_preset_description(preset: &Preset) -> &str {
    preset_description(preset, locale())
}

/// A `yes`/`no` answer of a detail row.
pub fn yes_no(value: bool) -> &'static str {
    match value {
        true => t("presets.value_yes"),
        false => t("presets.value_no"),
    }
}

/// `none` is a real `video_codec`: the preset removes the video stream.
pub fn codec_label(codec: &str) -> String {
    match codec {
        "none" => t("presets.value_no_video").to_owned(),
        other => other.to_owned(),
    }
}

/// The HDR policy in the words of the current locale.
pub fn hdr_label(policy: HdrPolicy) -> &'static str {
    match policy {
        HdrPolicy::Preserve => t("presets.value_hdr_preserve"),
        HdrPolicy::ToneMapSdr => t("presets.value_hdr_tonemap"),
    }
}

/// The subtitle policy in the words of the current locale.
pub fn subtitle_label(policy: SubtitlePolicy) -> &'static str {
    match policy {
        SubtitlePolicy::PreserveCompatible => t("presets.value_subtitles_preserve"),
        SubtitlePolicy::ConvertCompatible => t("presets.value_subtitles_convert"),
        SubtitlePolicy::Remove => t("presets.value_subtitles_remove"),
        SubtitlePolicy::BurnIn => t("presets.value_subtitles_burn_in"),
    }
}

/// The resolution limit of a preset, or nothing when it declares none.
pub fn resolution(width: Option<u32>, height: Option<u32>) -> Option<String> {
    match (width, height) {
        (Some(width), Some(height)) => Some(format!("{width}×{height}")),
        (Some(width), None) => {
            Some(t("presets.value_width_limit").replace("{n}", &width.to_string()))
        }
        (None, Some(height)) => {
            Some(t("presets.value_height_limit").replace("{n}", &height.to_string()))
        }
        (None, None) => None,
    }
}
