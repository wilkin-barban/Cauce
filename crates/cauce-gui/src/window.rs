//! The `AdwApplicationWindow`: sidebar, content stack and daemon banner
//! (`docs/ui-mvp.md` §2, §9 and §12, `docs/architecture.md` §13).
//!
//! The window owns the client. Every daemon interaction goes through
//! `glib::spawn_future_local`, so the main loop never blocks on a socket, and the
//! banner is the only error surface of the shell: a missing daemon is a
//! non-blocking notice with a "Reconectar" action, never a crash and never the
//! same thing as an empty page.

use std::io::Write;
use std::rc::Rc;

use adw::glib;
use adw::gtk;
use adw::prelude::*;
use cauce_core::ErrorCode;
use libadwaita as adw;
use serde_json::json;

use crate::client::{self, Client};
use crate::pages;

/// Application id, shared with the future desktop file (`C-050`).
pub const APPLICATION_ID: &str = "io.github.wilkin.Cauce";

/// Icon name the theme may already provide for Cauce.
const APP_ICON: &str = "io.github.wilkin.Cauce";

/// Symbolic icon every Adwaita-based theme ships; used when `APP_ICON` is absent.
const FALLBACK_ICON: &str = "video-x-generic-symbolic";

/// Text of the banner when the socket cannot be reached (`docs/ui-mvp.md` §9).
const OFFLINE_BANNER: &str = "No se pudo conectar con el daemon.";

/// Shows the window, creating it on the first activation.
pub fn present(application: &adw::Application) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }
    let window = build(application);
    window.present();
}

/// Picks `io.github.wilkin.Cauce` when the theme ships it, else a symbolic icon.
pub fn app_icon() -> &'static str {
    let Some(display) = gtk::gdk::Display::default() else {
        return FALLBACK_ICON;
    };
    if gtk::IconTheme::for_display(&display).has_icon(APP_ICON) {
        return APP_ICON;
    }
    FALLBACK_ICON
}

fn build(application: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Cauce")
        .default_width(960)
        .default_height(640)
        .build();

    let client = Rc::new(Client::connect());
    let content = Rc::new(pages::all(&client));

    let banner = adw::Banner::new(OFFLINE_BANNER);
    banner.set_button_label(Some("Reconectar"));
    banner.set_revealed(false);

    let daemon_state = gtk::Label::new(Some(OFFLINE_DAEMON_STATE));
    daemon_state.add_css_class("caption");
    daemon_state.add_css_class("dim-label");

    let split = adw::NavigationSplitView::new();
    split.set_min_sidebar_width(200.0);
    split.set_max_sidebar_width(320.0);
    split.set_sidebar(Some(&sidebar(&content, &split, &daemon_state)));
    if let Some(first) = content.first() {
        split.set_content(Some(first));
    }

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Cauce",
        "Conversor multimedia nativo",
    )));
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&banner);
    toolbar.set_content(Some(&split));
    window.set_content(Some(&toolbar));

    // `daemon.status` notifications keep the footer truthful between probes, and
    // the client-side `daemon.disconnected` event means the socket closed, so the
    // banner appears immediately instead of waiting for the next probe
    // (`docs/contracts.md` §7, review findings R3-002 and R4-003).
    let events = client.events();
    let banner_for_events = banner.clone();
    let state_for_events = daemon_state.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = events.recv().await {
            match event.get("method").and_then(serde_json::Value::as_str) {
                Some(client::DISCONNECTED_EVENT) => {
                    banner_for_events.set_title(OFFLINE_BANNER);
                    banner_for_events.set_revealed(true);
                    state_for_events.set_label(OFFLINE_DAEMON_STATE);
                }
                // Any event from a live daemon proves the connection works, so the
                // banner hides again as soon as the daemon answers.
                Some(_) => {
                    banner_for_events.set_revealed(false);
                    state_for_events.set_label("Daemon: conectado");
                }
                None => {}
            }
        }
    });

    let banner_for_retry = banner.clone();
    let state_for_retry = daemon_state.clone();
    let client_for_retry = Rc::clone(&client);
    let retry: Rc<dyn Fn()> = Rc::new(move || {
        probe_daemon(&client_for_retry, &banner_for_retry, &state_for_retry);
    });
    let retry_on_click = Rc::clone(&retry);
    banner.connect_button_clicked(move |_| retry_on_click());

    let retry_on_map = Rc::clone(&retry);
    let smoke = std::env::var_os("CAUCE_SMOKE").is_some_and(|value| value == "1");
    let announced = Rc::new(std::cell::Cell::new(false));
    window.connect_map(move |window| {
        // Retrying on every activation keeps the banner honest after the daemon
        // comes back without restarting Cauce.
        retry_on_map();
        if !smoke || announced.replace(true) {
            return;
        }
        println!("window-ready");
        let _ = std::io::stdout().flush();
        let application = window.application();
        glib::timeout_add_seconds_local(1, move || {
            if let Some(application) = application.as_ref() {
                application.quit();
            }
            glib::ControlFlow::Break
        });
    });

    window
}

const OFFLINE_DAEMON_STATE: &str = "Daemon: sin conexión";

/// Sidebar: app identity, the five sections and the summarized daemon state.
fn sidebar(
    content: &Rc<Vec<adw::NavigationPage>>,
    split: &adw::NavigationSplitView,
    daemon_state: &gtk::Label,
) -> adw::NavigationPage {
    let identity = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    identity.set_margin_top(12);
    identity.set_margin_bottom(6);
    identity.set_margin_start(12);
    identity.set_margin_end(12);
    let icon = gtk::Image::from_icon_name(app_icon());
    icon.set_pixel_size(32);
    identity.append(&icon);
    identity.append(&gtk::Label::new(Some("Cauce")));

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    list.set_vexpand(true);
    for section in pages::SECTIONS.iter() {
        list.append(&sidebar_row(section.title, section.icon));
    }

    let footer = gtk::Box::new(gtk::Orientation::Vertical, 2);
    footer.set_margin_top(6);
    footer.set_margin_bottom(12);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    footer.append(daemon_state);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&identity);
    body.append(&list);
    body.append(&footer);

    let pages = Rc::clone(content);
    let split = split.clone();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        let index = row.index();
        if let Some(page) = usize::try_from(index).ok().and_then(|i| pages.get(i)) {
            split.set_content(Some(page));
        }
    });
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }

    adw::NavigationPage::new(&body, "Cauce")
}

/// One sidebar entry: a symbolic icon plus its label, so no control is icon-only.
fn sidebar_row(title: &str, icon: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&gtk::Image::from_icon_name(icon));
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    content.append(&label);
    row.set_child(Some(&content));
    row
}

/// Asks the daemon for `initialize` and reflects the answer in banner and footer.
fn probe_daemon(client: &Rc<Client>, banner: &adw::Banner, daemon_state: &gtk::Label) {
    let client = Rc::clone(client);
    let banner = banner.clone();
    let daemon_state = daemon_state.clone();
    glib::spawn_future_local(async move {
        let params = json!({
            "client_name": "cauce-gui",
            "client_version": env!("CARGO_PKG_VERSION"),
            "protocol_version": 1,
        });
        match client.call("initialize", params).await {
            Ok(_) => {
                banner.set_revealed(false);
                daemon_state.set_label("Daemon: conectado");
            }
            Err(error) => {
                banner.set_title(&banner_text(&error));
                banner.set_revealed(true);
                daemon_state.set_label(&format!("Daemon: {}", state_text(&error)));
            }
        }
    });
}

/// Banner copy: the fixed sentence for an absent daemon, the daemon's own text
/// for a protocol-level failure.
fn banner_text(error: &cauce_core::JobError) -> String {
    if error.code == ErrorCode::DaemonUnavailable {
        return OFFLINE_BANNER.to_owned();
    }
    error.message.clone()
}

fn state_text(error: &cauce_core::JobError) -> &'static str {
    match error.code {
        ErrorCode::DaemonUnavailable => "sin conexión",
        _ => "error",
    }
}
