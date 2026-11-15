//! The Diagnóstico page: what the system offers and what is missing, with two
//! concrete actions (`docs/ui-mvp.md` §7 and §9, `docs/contracts.md` §6).
//!
//! Everything comes from `system.capabilities`; the page never runs FFmpeg
//! itself (`docs/architecture.md` §5). The pure core ([`human_bytes`] and
//! [`capabilities_report`]) has no GTK type, so `tests/pages_contract.rs` covers
//! the units and the exact text of the copy button without a display.
//!
//! `Copiar informe` deliberately reports only what the payload carries: no file
//! path and no socket path ever reaches the clipboard.

use std::cell::RefCell;
use std::rc::Rc;

use adw::glib;
use adw::gtk;
use adw::prelude::*;
use cauce_core::Capabilities;
use libadwaita as adw;
use serde_json::json;

use crate::client::Client;
use crate::pages::escaped;

/// The command `docs/ui-mvp.md` §9 asks the page to offer when FFmpeg is absent.
pub const FFMPEG_INSTALL: &str = "sudo apt install ffmpeg";

/// Offline copy of the error page; never the same screen as an empty report.
const OFFLINE: &str = "No se pudo conectar con el daemon.";

/// Bytes in the unit a person reads: GiB and MiB with one decimal, else bytes.
pub fn human_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    match bytes {
        b if b >= GIB => format!("{:.1} GiB", b as f64 / GIB as f64),
        b if b >= MIB => format!("{:.1} MiB", b as f64 / MIB as f64),
        b => format!("{b} B"),
    }
}

/// The exact text `Copiar informe` puts on the clipboard: one fact per line, in
/// a stable order, plus the gaps only when there are any.
pub fn capabilities_report(caps: &Capabilities) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(9);
    lines.push(format!("FFmpeg: {}", or_unknown(&caps.ffmpeg_version)));
    lines.push(format!("FFprobe: {}", or_unknown(&caps.ffprobe_version)));
    lines.push(format!("Encoders: {}", caps.encoders.len()));
    lines.push(format!("Filtros: {}", caps.filters.len()));
    lines.push(format!("Núcleos de CPU: {}", caps.cpu_cores));
    lines.push(format!(
        "Disco libre: {}",
        human_bytes(caps.disk_free_bytes)
    ));
    lines.push(format!("Daemon: {}", or_unknown(&caps.daemon_version)));
    for missing in caps.ffmpeg_missing.iter() {
        lines.push(format!("Falta {missing}"));
    }
    for warning in caps.warnings.iter() {
        lines.push(format!("Aviso: {warning}"));
    }
    let mut report = lines.join("\n");
    report.push('\n');
    report
}

fn or_unknown(value: &str) -> String {
    match value.is_empty() {
        true => "desconocida".to_owned(),
        false => value.to_owned(),
    }
}

/// Every widget of the page.
#[derive(Clone)]
struct Ui {
    page: adw::NavigationPage,
    stack: gtk::Stack,
    error: adw::StatusPage,
    retry: gtk::Button,
    refresh: gtk::Button,
    copy: gtk::Button,
    report: Rc<RefCell<String>>,
}

/// Builds the page, wires its three actions and asks for the capabilities.
pub fn page(client: &Rc<Client>) -> adw::NavigationPage {
    let ui = build();

    for button in [&ui.retry, &ui.refresh] {
        let (client, ui) = (Rc::clone(client), ui.clone());
        button.connect_clicked(move |_| load(&client, &ui));
    }

    let report = Rc::clone(&ui.report);
    ui.copy.connect_clicked(move |button| {
        let text = report.borrow().clone();
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&text);
        }
        button.set_label("Informe copiado");
    });

    load(client, &ui);
    ui.page
}

fn build() -> Ui {
    let refresh = gtk::Button::with_label("Actualizar");
    refresh.add_css_class("flat");
    let copy = gtk::Button::with_label("Copiar informe");
    copy.add_css_class("suggested-action");

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Diagnóstico", "")));
    header.pack_start(&refresh);
    header.pack_end(&copy);

    let loading = adw::StatusPage::new();
    loading.set_icon_name(Some("utilities-system-monitor-symbolic"));
    loading.set_title("Revisando el entorno");
    loading.set_description(Some("Consultando las capacidades del daemon…"));

    let error = adw::StatusPage::new();
    error.set_icon_name(Some("dialog-warning-symbolic"));
    error.set_title("No se pudo leer el diagnóstico");
    error.set_description(Some(OFFLINE));
    let retry = gtk::Button::with_label("Reintentar");
    retry.add_css_class("pill");
    error.set_child(Some(&retry));

    let stack = gtk::Stack::new();
    stack.add_named(&loading, Some("loading"));
    stack.add_named(&error, Some("error"));
    stack.set_visible_child_name("loading");

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));

    let page = adw::NavigationPage::new(&toolbar, "Diagnóstico");
    page.set_tag(Some("diagnostics"));

    Ui {
        page,
        stack,
        error,
        retry,
        refresh,
        copy,
        report: Rc::new(RefCell::new(String::new())),
    }
}

/// `system.capabilities`, awaited off the GTK thread.
fn load(client: &Rc<Client>, ui: &Ui) {
    ui.stack.set_visible_child_name("loading");
    let (client, ui) = (Rc::clone(client), ui.clone());
    glib::spawn_future_local(async move {
        let caps = match client.call("system.capabilities", json!({})).await {
            Ok(value) => match serde_json::from_value::<Capabilities>(value) {
                Ok(caps) => caps,
                Err(error) => {
                    fail(&ui, &format!("El diagnóstico llegó ilegible: {error}"));
                    return;
                }
            },
            Err(error) => {
                fail(&ui, &error.message);
                return;
            }
        };
        render(&ui, &caps);
    });
}

fn fail(ui: &Ui, message: &str) {
    ui.error.set_description(Some(message));
    ui.stack.set_visible_child_name("error");
}

/// Replaces the ready page with a freshly built `AdwPreferencesPage`.
fn render(ui: &Ui, caps: &Capabilities) {
    *ui.report.borrow_mut() = capabilities_report(caps);

    let page = adw::PreferencesPage::new();
    page.add(&tools_group(caps));
    page.add(&machine_group(caps));
    if !caps.ffmpeg_missing.is_empty() {
        page.add(&missing_group(caps));
    }
    if !caps.warnings.is_empty() {
        page.add(&warnings_group(caps));
    }

    if let Some(previous) = ui.stack.child_by_name("ready") {
        ui.stack.remove(&previous);
    }
    ui.stack.add_named(&page, Some("ready"));
    ui.stack.set_visible_child_name("ready");
}

fn tools_group(caps: &Capabilities) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Herramientas");
    group.set_description(Some(
        "Cauce usa el FFmpeg del sistema: no embebe ningún binario.",
    ));
    group.add(&row("FFmpeg", &or_unknown(&caps.ffmpeg_version)));
    group.add(&row("FFprobe", &or_unknown(&caps.ffprobe_version)));
    group.add(&row(
        "Encoders",
        &format!("{} reportados por FFmpeg", caps.encoders.len()),
    ));
    group.add(&row(
        "Filtros",
        &format!("{} reportados por FFmpeg", caps.filters.len()),
    ));
    group
}

fn machine_group(caps: &Capabilities) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Equipo y daemon");
    group.add(&row(
        "Núcleos de CPU",
        &format!("{} disponibles", caps.cpu_cores),
    ));
    group.add(&row(
        "Espacio libre",
        &format!(
            "{} en la carpeta de salida",
            human_bytes(caps.disk_free_bytes)
        ),
    ));
    group.add(&row("Daemon", &or_unknown(&caps.daemon_version)));
    group
}

/// Every missing item is named, with the one command that fixes all of them.
fn missing_group(caps: &Capabilities) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Falta instalar");
    group.set_description(Some(
        "Estos encoders y filtros son obligatorios para los presets del sistema.",
    ));
    for item in caps.ffmpeg_missing.iter() {
        group.add(&row(item, "Obligatorio para los presets del sistema"));
    }

    let command = adw::ActionRow::new();
    command.set_title("Instalación");
    command.set_subtitle(FFMPEG_INSTALL);
    let copy = gtk::Button::with_label("Copiar comando");
    copy.add_css_class("flat");
    copy.connect_clicked(|button| {
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(FFMPEG_INSTALL);
        }
        button.set_label("Comando copiado");
    });
    command.add_suffix(&copy);
    group.add(&command);
    group
}

fn warnings_group(caps: &Capabilities) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Avisos");
    for warning in caps.warnings.iter() {
        group.add(&row(warning, ""));
    }
    group
}

fn row(title: &str, subtitle: &str) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&escaped(title));
    if !subtitle.is_empty() {
        row.set_subtitle(&escaped(subtitle));
    }
    row
}
