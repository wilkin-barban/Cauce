//! Widgets and flows of the Ajustes page (`docs/ui-mvp.md` §8 and §9,
//! `docs/contracts.md` §2.6 and §6).
//!
//! Every change is one [`SettingsEdit`] applied through `settings.set`; nothing
//! writes the configuration directly. Each call is awaited with
//! `glib::spawn_future_local`, so the GTK thread never waits on the socket, and
//! the widgets are re-rendered from the daemon's answer, so a rejected value
//! reverts and the page cannot drift from the daemon.
//!
//! The theme is applied live through `AdwStyleManager::set_color_scheme` and, since
//! `T19`, so is the language: the daemon is still the owner of the value (the write
//! goes through `settings.set`), and the answer is what switches the catalogs and
//! asks the shell to rebuild its pages.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::glib;
use adw::gtk;
use adw::prelude::*;
use cauce_core::{JobError, Settings};
use libadwaita as adw;
use serde_json::json;

use super::{
    LANGUAGE_LABELS, LANGUAGES, POLICIES, POLICY_LABELS, SettingsEdit, THEME_LABELS, THEMES,
    apply_scheme, edit_error, edit_params, index_of, language_at, locale_of, output_dir_to_show,
    policy_at, theme_at, wire,
};
use crate::client::Client;
use crate::i18n::t;
use crate::pages::{self, ErrorSurface, error_surface, escaped};

/// Every widget the page needs after construction.
#[derive(Clone)]
struct Ui {
    page: adw::NavigationPage,
    stack: gtk::Stack,
    error: adw::StatusPage,
    error_expander: gtk::Expander,
    error_details: gtk::Label,
    failure: ErrorSurface,
    retry: gtk::Button,
    banner: adw::Banner,
    output: adw::ActionRow,
    concurrency: adw::SpinRow,
    theme: adw::ComboRow,
    language: adw::ComboRow,
    notifications: adw::SwitchRow,
    duplicates: adw::ComboRow,
    keep_temp: adw::SwitchRow,
    choose: gtk::Button,
}

type Commit = Rc<dyn Fn(SettingsEdit)>;

/// Builds the page, loads `settings.get` and connects every write.
pub fn page(client: &Rc<Client>) -> adw::NavigationPage {
    let ui = build();
    let current: Rc<RefCell<Option<Settings>>> = Rc::new(RefCell::new(None));
    let guard = Rc::new(Cell::new(false));
    let commit = build_commit(client, &ui, &current, &guard);

    ui.choose.connect_clicked({
        let (ui, commit) = (ui.clone(), Rc::clone(&commit));
        move |_| pick_folder(&ui, &commit)
    });
    ui.concurrency.connect_value_notify({
        let (guard, commit, row) = (
            Rc::clone(&guard),
            Rc::clone(&commit),
            ui.concurrency.clone(),
        );
        move |_| {
            if guard.get() {
                return;
            }
            let value = row.value();
            commit(SettingsEdit::MaxConcurrentJobs(value.round() as u8));
        }
    });
    ui.theme.connect_selected_notify({
        let (guard, commit, row) = (Rc::clone(&guard), Rc::clone(&commit), ui.theme.clone());
        move |_| {
            if guard.get() {
                return;
            }
            commit(SettingsEdit::Theme(theme_at(row.selected())));
        }
    });
    ui.language.connect_selected_notify({
        let (guard, commit, row) = (Rc::clone(&guard), Rc::clone(&commit), ui.language.clone());
        move |_| {
            if guard.get() {
                return;
            }
            commit(SettingsEdit::Language(language_at(row.selected())));
        }
    });
    ui.duplicates.connect_selected_notify({
        let (guard, commit, row) = (Rc::clone(&guard), Rc::clone(&commit), ui.duplicates.clone());
        move |_| {
            if guard.get() {
                return;
            }
            commit(SettingsEdit::DuplicatePolicy(policy_at(row.selected())));
        }
    });
    ui.notifications.connect_active_notify({
        let (guard, commit, row) = (
            Rc::clone(&guard),
            Rc::clone(&commit),
            ui.notifications.clone(),
        );
        move |_| {
            if !guard.get() {
                commit(SettingsEdit::Notifications(row.is_active()));
            }
        }
    });
    ui.keep_temp.connect_active_notify({
        let (guard, commit, row) = (Rc::clone(&guard), Rc::clone(&commit), ui.keep_temp.clone());
        move |_| {
            if !guard.get() {
                commit(SettingsEdit::KeepTempOnFailure(row.is_active()));
            }
        }
    });
    ui.retry.connect_clicked({
        let (client, ui, current, guard) = (
            Rc::clone(client),
            ui.clone(),
            Rc::clone(&current),
            Rc::clone(&guard),
        );
        move |_| load(&client, &ui, &current, &guard)
    });
    // A folder chosen in Convertir repaints this row (`L43`).
    watch_output_dir(&ui, &current);
    load(client, &ui, &current, &guard);
    ui.page
}

fn build() -> Ui {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        t("section.settings_title"),
        "",
    )));

    let banner = adw::Banner::new("");
    banner.set_revealed(false);

    let loading = adw::StatusPage::new();
    loading.set_icon_name(Some("preferences-system-symbolic"));
    loading.set_title(t("settings.loading"));
    // The configuration page says what happened in one sentence and keeps the
    // code plus the daemon's message behind the closed disclosure (§8).
    let error = adw::StatusPage::new();
    error.set_icon_name(Some("dialog-warning-symbolic"));
    error.set_title(t("settings.error_title"));
    error.set_description(Some(t("settings.offline")));
    let retry = gtk::Button::with_label(t("settings.retry"));
    retry.add_css_class("pill");
    let error_details = pages::error_details_label();
    let error_expander = gtk::Expander::new(Some(t("error.technical_details")));
    error_expander.set_expanded(false);
    error_expander.set_child(Some(&error_details));
    let error_body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    error_body.set_halign(gtk::Align::Center);
    error_body.append(&error_expander);
    error_body.append(&retry);
    error.set_child(Some(&error_body));

    // A failed write does not replace the whole page: one line and a closed
    // disclosure above the preferences.
    let failure = error_surface("settings-failure");

    let output = adw::ActionRow::new();
    output.set_title(t("settings.output_folder"));
    output.set_subtitle(t("settings.not_chosen"));
    let choose = gtk::Button::with_label(t("settings.choose"));
    choose.add_css_class("flat");
    output.add_suffix(&choose);

    let concurrency = adw::SpinRow::with_range(1.0, 8.0, 1.0);
    concurrency.set_title(t("settings.concurrency"));
    concurrency.set_subtitle(t("settings.concurrency_hint"));

    let theme = combo_row(t("settings.theme"), &THEME_LABELS);
    let language = combo_row(t("settings.language"), &LANGUAGE_LABELS);
    let notifications = adw::SwitchRow::new();
    notifications.set_title(t("settings.notifications"));
    notifications.set_subtitle(t("settings.notifications_hint"));
    let duplicates = combo_row(t("settings.duplicates"), &POLICY_LABELS);
    duplicates.set_subtitle(t("settings.duplicates_hint"));
    let keep_temp = adw::SwitchRow::new();
    keep_temp.set_title(t("settings.keep_temp"));
    keep_temp.set_subtitle(t("settings.keep_temp_hint"));

    let general = adw::PreferencesGroup::new();
    general.set_title(t("settings.general_group"));
    general.add(&output);
    general.add(&concurrency);
    general.add(&theme);
    general.add(&language);

    let behavior = adw::PreferencesGroup::new();
    behavior.set_title(t("settings.behavior_group"));
    behavior.add(&notifications);
    behavior.add(&duplicates);
    behavior.add(&keep_temp);

    let preferences = adw::PreferencesPage::new();
    preferences.add(&general);
    preferences.add(&behavior);

    let stack = gtk::Stack::new();
    stack.add_named(&loading, Some("loading"));
    stack.add_named(&error, Some("error"));
    stack.add_named(&preferences, Some("ready"));
    stack.set_visible_child_name("loading");

    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.append(&failure.area);
    body.append(&stack);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&banner);
    toolbar.set_content(Some(&body));

    let page = adw::NavigationPage::new(&toolbar, t("section.settings_title"));
    page.set_tag(Some("settings"));
    Ui {
        page,
        stack,
        error,
        error_expander,
        error_details,
        failure,
        retry,
        banner,
        output,
        concurrency,
        theme,
        language,
        notifications,
        duplicates,
        keep_temp,
        choose,
    }
}

/// A combo whose entries are catalog keys: the visible text is translated here,
/// the selection index still comes from the documented table
/// (`THEMES`/`LANGUAGES`/`POLICIES`).
fn combo_row(title: &str, labels: &[&'static str]) -> adw::ComboRow {
    let texts: Vec<&str> = labels.iter().map(|key| t(key)).collect();
    let row = adw::ComboRow::new();
    row.set_title(title);
    row.set_model(Some(&gtk::StringList::new(&texts)));
    row
}

/// `settings.get`, awaited off the GTK thread.
fn load(
    client: &Rc<Client>,
    ui: &Ui,
    current: &Rc<RefCell<Option<Settings>>>,
    guard: &Rc<Cell<bool>>,
) {
    ui.stack.set_visible_child_name("loading");
    let (client, ui, current, guard) = (
        Rc::clone(client),
        ui.clone(),
        Rc::clone(current),
        Rc::clone(guard),
    );
    glib::spawn_future_local(async move {
        let answer = client
            .call("settings.get", json!({}))
            .await
            .and_then(|value| {
                serde_json::from_value::<Settings>(value).map_err(|error| {
                    cauce_core::JobError::new(
                        cauce_core::ErrorCode::Internal,
                        error.to_string(),
                        false,
                    )
                })
            });
        match answer {
            Ok(settings) => {
                *current.borrow_mut() = Some(settings.clone());
                render(&ui, &settings, &guard);
                pages::clear_failure(&ui.failure);
                show_error(&ui, "");
                ui.stack.set_visible_child_name("ready");
            }
            Err(error) => show_load_failure(&ui, &error),
        }
    });
}

/// Writes one edit through `settings.set`; the daemon's answer is the truth.
fn build_commit(
    client: &Rc<Client>,
    ui: &Ui,
    current: &Rc<RefCell<Option<Settings>>>,
    guard: &Rc<Cell<bool>>,
) -> Commit {
    let (client, ui) = (Rc::clone(client), ui.clone());
    let (current, guard) = (Rc::clone(current), Rc::clone(guard));
    Rc::new(move |edit: SettingsEdit| {
        let base = match current.borrow().clone() {
            Some(base) => base,
            None => return,
        };
        if let Some(message) = edit_error(&edit) {
            render(&ui, &base, &guard);
            show_error(&ui, &message);
            return;
        }
        if let SettingsEdit::Theme(theme) = edit {
            // Live: the window changes before the daemon answers.
            apply_scheme(theme);
        }
        let params = edit_params(&edit);
        let (client, ui) = (Rc::clone(&client), ui.clone());
        let (current, guard) = (Rc::clone(&current), Rc::clone(&guard));
        glib::spawn_future_local(async move {
            let answer = client.call("settings.set", params).await.and_then(|value| {
                serde_json::from_value::<Settings>(value).map_err(|error| {
                    cauce_core::JobError::new(
                        cauce_core::ErrorCode::Internal,
                        error.to_string(),
                        false,
                    )
                })
            });
            match answer {
                Ok(saved) => {
                    // Ajustes saves the folder Convertir shows too (`L43`).
                    crate::pages::set_output_dir(&saved.output_dir);
                    *current.borrow_mut() = Some(saved.clone());
                    render(&ui, &saved, &guard);
                    pages::clear_failure(&ui.failure);
                    show_error(&ui, "");
                }
                Err(error) => {
                    if let Some(base) = current.borrow().clone() {
                        render(&ui, &base, &guard);
                    }
                    show_write_failure(&ui, &error);
                }
            }
        });
    })
}

/// Reflects one settings value in every widget, without re-triggering writes.
fn render(ui: &Ui, settings: &Settings, guard: &Rc<Cell<bool>>) {
    guard.set(true);
    ui.output
        .set_subtitle(&escaped(&output_dir_to_show(settings)));
    ui.concurrency
        .set_value(f64::from(settings.max_concurrent_jobs));
    ui.theme
        .set_selected(index_of(&THEMES, &wire(&settings.theme)));
    ui.language
        .set_selected(index_of(&LANGUAGES, &wire(&settings.language)));
    ui.notifications.set_active(settings.notifications);
    ui.duplicates
        .set_selected(index_of(&POLICIES, &wire(&settings.duplicate_policy)));
    ui.keep_temp.set_active(settings.keep_temp_on_failure);
    guard.set(false);
    apply_scheme(settings.theme);
    // The daemon's answer is what switches the language, so the interface always
    // follows the persisted value instead of a local guess.
    crate::i18n::set_locale(locale_of(settings.language));
}

/// Inline error inside the page, never only a toast (`docs/ui-mvp.md` §9).
fn show_error(ui: &Ui, message: &str) {
    if message.is_empty() {
        ui.banner.set_revealed(false);
        return;
    }
    ui.banner.set_title(message);
    ui.banner.set_revealed(true);
}

/// `settings.get` failed: the page shows the human sentence and hides the code
/// and the daemon's message behind the disclosure.
fn show_load_failure(ui: &Ui, error: &JobError) {
    ui.error
        .set_description(Some(&crate::i18n::human_error(error.code, &error.message)));
    ui.error_details
        .set_label(&crate::i18n::technical_details(error.code, &error.message));
    ui.error_expander.set_visible(true);
    ui.error_expander.set_expanded(false);
    ui.stack.set_visible_child_name("error");
}

/// `settings.set` failed: the preferences stay in place and the human sentence
/// replaces the banner, with the raw text one click away.
fn show_write_failure(ui: &Ui, error: &JobError) {
    ui.banner
        .set_title(&crate::i18n::human_error(error.code, &error.message));
    ui.banner.set_revealed(true);
    pages::show_failure(&ui.failure, error);
}

/// Follows the folder Convertir chooses, without a second `settings.get`.
fn watch_output_dir(ui: &Ui, current: &Rc<RefCell<Option<Settings>>>) {
    let (ui, current) = (ui.clone(), Rc::clone(current));
    crate::pages::on_output_dir_change(Rc::new(move |dir: &str| {
        ui.output.set_subtitle(&escaped(dir));
        if let Some(settings) = current.borrow_mut().as_mut() {
            settings.output_dir = dir.to_owned();
        }
    }));
}

/// The folder dialog of the output directory.
fn pick_folder(ui: &Ui, commit: &Commit) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title(t("settings.folder_dialog"));
    let parent = ui
        .page
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let commit = Rc::clone(commit);
    glib::spawn_future_local(async move {
        let Ok(folder) = dialog.select_folder_future(parent.as_ref()).await else {
            return;
        };
        let Some(path) = folder.path() else {
            return;
        };
        commit(SettingsEdit::OutputDir(path.to_string_lossy().into_owned()));
    });
}
