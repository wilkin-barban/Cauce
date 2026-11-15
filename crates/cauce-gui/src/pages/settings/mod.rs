//! The Ajustes page: every mutable setting of `docs/contracts.md` §2.6, written
//! through `settings.set` and never written directly (`docs/ui-mvp.md` §8).
//!
//! The page is split so each part stays under the size rule and single-purpose:
//!
//! - this file is the decision core: one [`SettingsEdit`] per mutable field, the
//!   validation that can reject it, the resulting [`Settings`] and the theme
//!   mapping. It has no GTK widget, so `tests/pages_contract.rs` covers the edit
//!   semantics without a display.
//! - [`view`] builds the widgets and performs every write. It is re-rendered from
//!   the daemon's answer after each change, so a rejected value reverts and the
//!   page never drifts from the daemon.
//!
//! Since `T19` the language is not only persisted: [`view`] applies it live, so
//! choosing a locale repaints the whole shell without a restart.

use cauce_core::{DuplicatePolicy, Language, Settings, Theme};
use cauce_i18n::Locale;
use libadwaita as adw;
use serde_json::json;

use crate::i18n::t;

pub mod view;

pub use view::page;

/// Documented order of the three themes (`docs/ui-mvp.md` §8).
pub const THEMES: [&str; 3] = ["system", "light", "dark"];
/// Wire values of the three locales; `pt-BR` keeps its exact tag.
pub const LANGUAGES: [&str; 3] = ["es", "en", "pt-BR"];
/// Documented order of the duplicate policies (`docs/contracts.md` §2.6).
pub const POLICIES: [&str; 3] = ["suffix", "skip", "overwrite"];

/// One change of one field. A rejected edit answers the previous settings, so
/// the caller can put the widget back where the daemon expects it.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEdit {
    OutputDir(String),
    MaxConcurrentJobs(u8),
    Theme(Theme),
    Language(Language),
    Notifications(bool),
    DuplicatePolicy(DuplicatePolicy),
    KeepTempOnFailure(bool),
}

/// Why the daemon would reject this edit, or nothing when it is acceptable.
pub fn edit_error(edit: &SettingsEdit) -> Option<String> {
    match edit {
        SettingsEdit::MaxConcurrentJobs(jobs) if !(1..=8).contains(jobs) => {
            Some(t("settings.invalid_concurrency").replace("{jobs}", &jobs.to_string()))
        }
        SettingsEdit::OutputDir(dir) if dir.trim().is_empty() => {
            Some(t("settings.invalid_output").to_owned())
        }
        _ => None,
    }
}

/// The settings after one accepted edit; a rejected edit returns `current`
/// untouched, never a half-applied struct.
pub fn settings_after_edit(current: &Settings, edit: SettingsEdit) -> Settings {
    if edit_error(&edit).is_some() {
        return current.clone();
    }
    let mut next = current.clone();
    match edit {
        SettingsEdit::OutputDir(dir) => next.output_dir = dir,
        SettingsEdit::MaxConcurrentJobs(jobs) => next.max_concurrent_jobs = jobs,
        SettingsEdit::Theme(theme) => next.theme = theme,
        SettingsEdit::Language(language) => next.language = language,
        SettingsEdit::Notifications(on) => next.notifications = on,
        SettingsEdit::DuplicatePolicy(policy) => next.duplicate_policy = policy,
        SettingsEdit::KeepTempOnFailure(on) => next.keep_temp_on_failure = on,
    }
    next
}

/// The `Partial<Settings>` body of `settings.set` for one edit.
pub fn edit_params(edit: &SettingsEdit) -> serde_json::Value {
    match edit {
        SettingsEdit::OutputDir(dir) => json!({ "output_dir": dir }),
        SettingsEdit::MaxConcurrentJobs(jobs) => json!({ "max_concurrent_jobs": jobs }),
        SettingsEdit::Theme(theme) => json!({ "theme": theme }),
        SettingsEdit::Language(language) => json!({ "language": language }),
        SettingsEdit::Notifications(on) => json!({ "notifications": on }),
        SettingsEdit::DuplicatePolicy(policy) => json!({ "duplicate_policy": policy }),
        SettingsEdit::KeepTempOnFailure(on) => json!({ "keep_temp_on_failure": on }),
    }
}

/// `system`/`light`/`dark` as the scheme `AdwStyleManager` understands.
pub fn theme_from_str(value: &str) -> adw::ColorScheme {
    match value {
        "light" => adw::ColorScheme::ForceLight,
        "dark" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    }
}

/// The inverse of [`theme_from_str`]; a preference for light or dark is saved as
/// that same value so the round trip is exact.
pub fn theme_to_str(scheme: adw::ColorScheme) -> &'static str {
    match scheme {
        adw::ColorScheme::ForceLight | adw::ColorScheme::PreferLight => "light",
        adw::ColorScheme::ForceDark | adw::ColorScheme::PreferDark => "dark",
        _ => "system",
    }
}

/// Applies the theme to the running application, without restarting it.
pub fn apply_scheme(theme: Theme) {
    let scheme = theme_from_str(wire(&theme).as_str());
    adw::StyleManager::default().set_color_scheme(scheme);
}

pub fn theme_at(index: u32) -> Theme {
    match index {
        1 => Theme::Light,
        2 => Theme::Dark,
        _ => Theme::System,
    }
}

pub fn language_at(index: u32) -> Language {
    match index {
        1 => Language::En,
        2 => Language::PtBr,
        _ => Language::Es,
    }
}

/// Catalog keys of the three themes, in the order of [`THEMES`].
pub const THEME_LABELS: [&str; 3] = [
    "settings.theme_system",
    "settings.theme_light",
    "settings.theme_dark",
];
/// The three language names, always shown in their own language.
pub const LANGUAGE_LABELS: [&str; 3] = [
    "settings.language_es",
    "settings.language_en",
    "settings.language_pt_br",
];
/// Catalog keys of the three duplicate policies, in the order of [`POLICIES`].
pub const POLICY_LABELS: [&str; 3] = [
    "settings.policy_suffix",
    "settings.policy_skip",
    "settings.policy_overwrite",
];

/// The `Locale` of the settings contract, for the live switch.
pub fn locale_of(language: Language) -> Locale {
    match language {
        Language::Es => Locale::Es,
        Language::En => Locale::En,
        Language::PtBr => Locale::PtBr,
    }
}

pub fn policy_at(index: u32) -> DuplicatePolicy {
    match index {
        1 => DuplicatePolicy::Skip,
        2 => DuplicatePolicy::Overwrite,
        _ => DuplicatePolicy::Suffix,
    }
}

/// The wire spelling of a domain enum (`system`, `light`, `dark`, `pt-BR`, …).
pub fn wire<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        _ => String::new(),
    }
}

/// Position of one wire value in its documented table.
pub fn index_of(table: &[&str], value: &str) -> u32 {
    table
        .iter()
        .position(|candidate| *candidate == value)
        .unwrap_or(0) as u32
}
