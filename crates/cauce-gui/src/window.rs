//! The `AdwApplicationWindow`: sidebar, content stack and daemon banner
//! (`docs/ui-mvp.md` §2, §9 and §12, `docs/architecture.md` §13).
//!
//! The window owns the client. Every daemon interaction goes through
//! `glib::spawn_future_local`, so the main loop never blocks on a socket, and the
//! banner is the only error surface of the shell: a missing daemon is a
//! non-blocking notice with a "Reconectar" action, never a crash and never the
//! same thing as an empty page.
//!
//! Since T19 every visible sentence is a catalog key (`S8`). A locale change is
//! applied by **rebuilding the page stack**: the pages are cheap, each one re-reads
//! its state from the daemon, and dropping them is the only way to guarantee that
//! no header, row, banner or tooltip keeps a stale language. The selected sidebar
//! section is preserved across the rebuild.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;

use adw::glib;
use adw::gtk;
use adw::prelude::*;
use cauce_core::ErrorCode;
use libadwaita as adw;
use serde_json::json;

use crate::client::{self, Client};
use crate::i18n;
use crate::pages;

/// Application id, shared with the future desktop file (`C-050`).
pub const APPLICATION_ID: &str = "io.github.wilkin.Cauce";

/// Icon name the theme may already provide for Cauce.
const APP_ICON: &str = "io.github.wilkin.Cauce";

/// Symbolic icon every Adwaita-based theme ships; used when `APP_ICON` is absent.
const FALLBACK_ICON: &str = "video-x-generic-symbolic";

/// What the shell believes about the daemon, so the chrome can be repainted
/// without asking again (`docs/ui-mvp.md` §9 and §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Online,
    Offline,
    Error,
}

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

/// The widgets a locale change has to repaint, plus the page stack it rebuilds.
#[derive(Clone)]
struct Chrome {
    content: Rc<RefCell<Vec<adw::NavigationPage>>>,
    list: gtk::ListBox,
    title: adw::WindowTitle,
    banner: adw::Banner,
    daemon_state: gtk::Label,
    link: Rc<Cell<Link>>,
}

impl Chrome {
    /// Repaints banner and footer from the last known daemon state.
    fn paint(&self) {
        self.banner
            .set_button_label(Some(i18n::t("banner.reconnect")));
        match self.link.get() {
            Link::Online => {
                self.banner.set_revealed(false);
                self.daemon_state
                    .set_label(i18n::t("window.daemon_connected"));
            }
            Link::Offline => {
                self.banner.set_title(i18n::t("banner.daemon_down"));
                self.banner.set_revealed(true);
                self.daemon_state
                    .set_label(i18n::t("window.daemon_offline_state"));
            }
            // The banner keeps the daemon's own message: it is data, not copy.
            Link::Error => {
                self.banner.set_revealed(true);
                self.daemon_state.set_label(&format!(
                    "{}: {}",
                    i18n::t("diagnostics.report_daemon"),
                    i18n::t("window.daemon_error")
                ));
            }
        }
    }

    /// Drops and re-creates every page, then repaints the chrome around them.
    fn relocalize(&self, client: &Rc<Client>) {
        let selected = self.list.selected_row().map(|row| row.index()).unwrap_or(0);
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        *self.content.borrow_mut() = pages::all(client);
        for section in pages::SECTIONS.iter() {
            self.list
                .append(&sidebar_row(i18n::t(section.title_key), section.icon));
        }
        // The sidebar header carries the title only: the subtitle would
        // ellipsize inside the 200-320px column.
        self.title.set_title(i18n::t("app.title"));
        self.paint();
        // Re-selecting the same index restores the section the user was reading.
        let last = i32::try_from(pages::SECTIONS.len()).unwrap_or(1) - 1;
        if let Some(row) = self.list.row_at_index(selected.clamp(0, last)) {
            self.list.select_row(Some(&row));
        }
    }
}

/// Builds the shell. Public so the chrome contract can be checked with a display.
pub fn build(application: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(i18n::t("app.title"))
        .default_width(960)
        .default_height(640)
        .build();

    let client = Rc::new(Client::connect());
    let content: Rc<RefCell<Vec<adw::NavigationPage>>> = Rc::new(RefCell::new(pages::all(&client)));

    let banner = adw::Banner::new(i18n::t("banner.daemon_down"));
    banner.set_button_label(Some(i18n::t("banner.reconnect")));
    banner.set_revealed(false);

    let daemon_state = gtk::Label::new(Some(i18n::t("window.daemon_offline_state")));
    daemon_state.add_css_class("caption");
    daemon_state.add_css_class("dim-label");

    let title = adw::WindowTitle::new(i18n::t("app.title"), "");
    let list = gtk::ListBox::new();
    let split = adw::NavigationSplitView::new();
    split.set_min_sidebar_width(200.0);
    split.set_max_sidebar_width(320.0);
    split.set_sidebar(Some(&sidebar(
        &content,
        &split,
        &list,
        &daemon_state,
        &title,
    )));
    if let Some(first) = content.borrow().first() {
        split.set_content(Some(first));
    }

    // The window draws no header bar of its own: every page owns one and
    // Libadwaita hides the window controls everywhere but at the window edges.
    // A window-level `AdwHeaderBar` here would draw a second set of them, which
    // is exactly the duplicate title bar of the first screenshot.
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&banner);
    toolbar.set_content(Some(&split));
    window.set_content(Some(&toolbar));

    let chrome = Chrome {
        content,
        list,
        title,
        banner,
        daemon_state,
        link: Rc::new(Cell::new(Link::Offline)),
    };

    wire_events(&chrome, &client);
    wire_connection(&chrome, &client);
    wire_retry(&chrome, &client);
    wire_smoke(&window);
    wire_locale(&chrome, &client);

    window
}

/// `daemon.status` and `daemon.disconnected` keep banner and footer truthful
/// between probes (`docs/contracts.md` §7, review findings R3-002 and R4-003).
fn wire_events(chrome: &Chrome, client: &Rc<Client>) {
    let events = client.events();
    let chrome = Rc::new(chrome.clone());
    glib::spawn_future_local(async move {
        while let Ok(event) = events.recv().await {
            match event.get("method").and_then(serde_json::Value::as_str) {
                Some(client::DISCONNECTED_EVENT) => {
                    chrome.link.set(Link::Offline);
                    chrome.paint();
                }
                // Any event from a live daemon proves the connection works.
                Some(_) => {
                    chrome.link.set(Link::Online);
                    chrome.paint();
                }
                None => {}
            }
        }
    });
}

/// A successful reply is the strongest proof that IPC works: this watches the
/// client's own state so the footer and the banner follow the request/response
/// path and never wait for a `daemon.status` event.
fn wire_connection(chrome: &Chrome, client: &Rc<Client>) {
    let states = client.connection();
    let chrome = Rc::new(chrome.clone());
    glib::spawn_future_local(async move {
        while let Ok(connected) = states.recv().await {
            chrome.link.set(if connected {
                Link::Online
            } else {
                Link::Offline
            });
            chrome.paint();
        }
    });
}

/// The banner button asks `initialize` again, so a returning daemon is noticed
/// without restarting Cauce.
fn wire_retry(chrome: &Chrome, client: &Rc<Client>) {
    let client = Rc::clone(client);
    let chrome = Rc::new(chrome.clone());
    chrome.banner.clone().connect_button_clicked(move |_| {
        probe_daemon(&client, &chrome);
    });
}

/// Rebuilds every page when the locale changes: registered once, fired by
/// `i18n::set_locale` from the Ajustes page.
fn wire_locale(chrome: &Chrome, client: &Rc<Client>) {
    let client = Rc::clone(client);
    let chrome = Rc::new(chrome.clone());
    i18n::on_locale_change(Rc::new(move || {
        // An idle hop lets the current signal handler return before its own page
        // is dropped, so no callback runs on a widget that is already unparented.
        let (client, chrome) = (Rc::clone(&client), Rc::clone(&chrome));
        glib::idle_add_local_once(move || chrome.relocalize(&client));
    }));
}

/// Sidebar list paired with the content stack: one row per documented section.
fn sidebar(
    content: &Rc<RefCell<Vec<adw::NavigationPage>>>,
    split: &adw::NavigationSplitView,
    list: &gtk::ListBox,
    daemon_state: &gtk::Label,
    title: &adw::WindowTitle,
) -> adw::NavigationPage {
    // The sidebar keeps the application title, never the window controls: the
    // editable buttons belong to the page at the window edge (`AdwNavigationSplitView`
    // hides them elsewhere), so the sidebar hides both sides explicitly.
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(title));
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);

    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    list.set_vexpand(true);
    for section in pages::SECTIONS.iter() {
        list.append(&sidebar_row(i18n::t(section.title_key), section.icon));
    }

    let footer = gtk::Box::new(gtk::Orientation::Vertical, 2);
    footer.set_margin_top(6);
    footer.set_margin_bottom(12);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    footer.append(daemon_state);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(list);
    body.append(&footer);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&body));

    let pages = Rc::clone(content);
    let split = split.clone();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        let index = row.index();
        if let Some(page) = usize::try_from(index)
            .ok()
            .and_then(|index| pages.borrow().get(index).cloned())
        {
            split.set_content(Some(&page));
        }
    });
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }

    adw::NavigationPage::new(&toolbar, i18n::t("app.title"))
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
    label.set_hexpand(true);
    // One line, truncated: at the 200 px minimum sidebar a wrapped label would
    // grow over the row and collide with its icon (`L43`).
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&label);
    row.set_child(Some(&content));
    row
}

/// Asks the daemon for `initialize` and reflects the answer in banner and footer.
fn probe_daemon(client: &Rc<Client>, chrome: &Rc<Chrome>) {
    let (client, chrome) = (Rc::clone(client), Rc::clone(chrome));
    glib::spawn_future_local(async move {
        let params = json!({
            "client_name": "cauce-gui",
            "client_version": env!("CARGO_PKG_VERSION"),
            "protocol_version": 1,
        });
        match client.call("initialize", params).await {
            Ok(_) => {
                chrome.link.set(Link::Online);
                chrome.paint();
            }
            Err(error) => {
                let state = match error.code {
                    ErrorCode::DaemonUnavailable => Link::Offline,
                    _ => Link::Error,
                };
                chrome.link.set(state);
                chrome.paint();
                if state == Link::Error {
                    // A protocol failure speaks with the daemon's own words.
                    chrome.banner.set_title(&error.message);
                }
            }
        }
    });
}

/// `CAUCE_SMOKE=1` prints `window-ready` on the first map and quits by itself, so
/// the smoke test proves a window really opened (`T24`).
fn wire_smoke(window: &adw::ApplicationWindow) {
    let smoke = std::env::var_os("CAUCE_SMOKE").is_some_and(|value| value == "1");
    let announced = Rc::new(Cell::new(false));
    window.connect_map(move |window| {
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
}
