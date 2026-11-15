//! Widgets and flows of the Ajustes page (`docs/ui-mvp.md` §8 and §9,
//! `docs/contracts.md` §2.6 and §6).
//!
//! Every change is one [`SettingsEdit`] applied through `settings.set`; nothing
//! writes the configuration directly. Each call is awaited with
//! `glib::spawn_future_local`, so the GTK thread never waits on the socket, and
//! the widgets are re-rendered from the daemon's answer, so a rejected value
//! reverts and the page cannot drift from the daemon.
//!
//! The theme is applied live through `AdwStyleManager::set_color_scheme`; the
//! language is only persisted until `T19` wires the catalogs.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::glib;
use adw::gtk;
use adw::prelude::*;
use cauce_core::Settings;
use libadwaita as adw;
use serde_json::json;

use super::{
    LANGUAGES, POLICIES, SettingsEdit, THEMES, apply_scheme, edit_error, edit_params, index_of,
    language_at, policy_at, theme_at, wire,
};
use crate::client::Client;
use crate::pages::escaped;

const THEME_LABELS: [&str; 3] = ["Sistema", "Claro", "Oscuro"];
const LANGUAGE_LABELS: [&str; 3] = ["Español", "English", "Português (Brasil)"];
const POLICY_LABELS: [&str; 3] = ["Añadir un sufijo", "Omitir el archivo", "Sobrescribir"];

const OFFLINE: &str = "No se pudo leer la configuración del daemon.";

/// Every widget the page needs after construction.
#[derive(Clone)]
struct Ui {
    page: adw::NavigationPage,
    stack: gtk::Stack,
    error: adw::StatusPage,
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
    load(client, &ui, &current, &guard);
    ui.page
}

fn build() -> Ui {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Ajustes", "")));

    let banner = adw::Banner::new("");
    banner.set_revealed(false);

    let loading = adw::StatusPage::new();
    loading.set_icon_name(Some("preferences-system-symbolic"));
    loading.set_title("Cargando la configuración");
    let error = adw::StatusPage::new();
    error.set_icon_name(Some("dialog-warning-symbolic"));
    error.set_title("No se pudo leer la configuración");
    error.set_description(Some(OFFLINE));
    let retry = gtk::Button::with_label("Reintentar");
    retry.add_css_class("pill");
    error.set_child(Some(&retry));

    let output = adw::ActionRow::new();
    output.set_title("Carpeta de salida");
    output.set_subtitle("Sin elegir");
    let choose = gtk::Button::with_label("Elegir…");
    choose.add_css_class("flat");
    output.add_suffix(&choose);

    let concurrency = adw::SpinRow::with_range(1.0, 8.0, 1.0);
    concurrency.set_title("Trabajos simultáneos");
    concurrency.set_subtitle("Entre 1 y 8 conversiones a la vez.");

    let theme = combo_row("Tema", &THEME_LABELS);
    let language = combo_row("Idioma", &LANGUAGE_LABELS);
    let notifications = adw::SwitchRow::new();
    notifications.set_title("Notificaciones de escritorio");
    notifications.set_subtitle("El daemon avisa una sola vez por trabajo terminado.");
    let duplicates = combo_row("Archivos duplicados", &POLICY_LABELS);
    duplicates.set_subtitle("Qué hacer cuando el destino ya existe.");
    let keep_temp = adw::SwitchRow::new();
    keep_temp.set_title("Conservar el temporal al fallar");
    keep_temp.set_subtitle("Sirve para revisar una conversión fallida.");

    let general = adw::PreferencesGroup::new();
    general.set_title("General");
    general.add(&output);
    general.add(&concurrency);
    general.add(&theme);
    general.add(&language);

    let behavior = adw::PreferencesGroup::new();
    behavior.set_title("Comportamiento");
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

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&banner);
    toolbar.set_content(Some(&stack));

    let page = adw::NavigationPage::new(&toolbar, "Ajustes");
    page.set_tag(Some("settings"));
    Ui {
        page,
        stack,
        error,
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

fn combo_row(title: &str, labels: &[&str]) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title(title);
    row.set_model(Some(&gtk::StringList::new(labels)));
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
                show_error(&ui, "");
                ui.stack.set_visible_child_name("ready");
            }
            Err(error) => {
                ui.error.set_description(Some(&error.message));
                ui.stack.set_visible_child_name("error");
            }
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
                    *current.borrow_mut() = Some(saved.clone());
                    render(&ui, &saved, &guard);
                    show_error(&ui, "");
                }
                Err(error) => {
                    if let Some(base) = current.borrow().clone() {
                        render(&ui, &base, &guard);
                    }
                    show_error(&ui, &error.message);
                }
            }
        });
    })
}

/// Reflects one settings value in every widget, without re-triggering writes.
fn render(ui: &Ui, settings: &Settings, guard: &Rc<Cell<bool>>) {
    guard.set(true);
    ui.output.set_subtitle(&escaped(&settings.output_dir));
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

/// The folder dialog of the output directory.
fn pick_folder(ui: &Ui, commit: &Commit) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Elegir carpeta de salida");
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
