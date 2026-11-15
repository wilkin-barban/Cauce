//! Widgets of the Cola page (`docs/ui-mvp.md` §5, §9 and §10).
//!
//! [`Ui`] owns every handle the page needs after construction, so the flows in
//! [`super::flows`] can clone it instead of juggling dozens of widgets. Nothing
//! here decides anything: the phase machine lives in [`super::state`].
//!
//! The list is a `GtkListBox` (its own accessible role is `list`), every row is
//! activatable-free so Tab reaches its buttons directly, and no widget carries a
//! fixed pixel height, so text scaling is respected.
//!
//! The confirmation dialog and the "open output" handler live here too: they are
//! pure widget work, they speak no IPC, and they need the [`Ui`] handles.

/// Opens the output folder (`true`) or the output file (`false`) of a job.
pub(super) type Open = Rc<dyn Fn(&Job, bool)>;

use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use adw::{gio, gtk};
use cauce_core::Job;
use libadwaita as adw;

use super::state::{JobAction, QueueState, progress_label, state_label};
use crate::client::Client;
use crate::i18n::t;
use crate::pages::convert::base_name;

/// Catalog key of the empty headline (`docs/ui-mvp.md` §5).
pub const EMPTY_TITLE: &str = "queue.empty_title";

/// Catalog key of how to put something in the queue, so the empty page is never
/// an error page.
pub const EMPTY_DESCRIPTION: &str = "queue.empty_description";

/// Every widget of the page, plus the page itself.
#[derive(Clone)]
pub struct Ui {
    pub page: adw::NavigationPage,
    pub toolbar: adw::ToolbarView,
    pub header: adw::HeaderBar,
    pub queue_toggle: gtk::Button,
    pub jobs_list: gtk::ListBox,
    pub stack: gtk::Stack,
    pub hint: gtk::Label,
    pub notice: gtk::Label,
    pub footer: gtk::Label,
    pub spinner: gtk::Spinner,
    pub error_page: adw::StatusPage,
    pub retry_button: gtk::Button,
}

impl Ui {
    fn build() -> Self {
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(t("section.queue_title"), "")));
        let queue_toggle = gtk::Button::with_label(t("queue.pause"));
        queue_toggle.add_css_class("flat");
        queue_toggle.set_tooltip_text(Some(t("queue.toggle_tooltip")));
        header.pack_end(&queue_toggle);

        let jobs_list = gtk::ListBox::new();
        jobs_list.add_css_class("boxed-list");
        jobs_list.set_selection_mode(gtk::SelectionMode::None);
        jobs_list.set_valign(gtk::Align::Start);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&jobs_list));
        scroll.set_vexpand(true);

        // The empty page is a quiet invitation, never the error page.
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 6);
        empty.set_valign(gtk::Align::Center);
        empty.set_halign(gtk::Align::Center);
        empty.set_margin_top(36);
        empty.set_margin_bottom(36);
        empty.append(&gtk::Image::from_icon_name("view-list-symbolic"));
        empty.append(&headline(t(EMPTY_TITLE)));
        let empty_text = text_line("dim-label");
        empty_text.set_label(t(EMPTY_DESCRIPTION));
        empty_text.set_justify(gtk::Justification::Center);
        empty_text.set_max_width_chars(48);
        empty.append(&empty_text);

        let spinner = gtk::Spinner::new();
        let loading = gtk::Box::new(gtk::Orientation::Vertical, 6);
        loading.set_valign(gtk::Align::Center);
        loading.set_halign(gtk::Align::Center);
        loading.append(&spinner);
        loading.append(&gtk::Label::new(Some(t("queue.loading"))));

        let error_page = adw::StatusPage::new();
        error_page.set_icon_name(Some("dialog-error-symbolic"));
        error_page.set_title(t("queue.error_title"));
        let retry_button = gtk::Button::with_label(t("queue.retry"));
        retry_button.add_css_class("pill");
        retry_button.set_halign(gtk::Align::Center);
        error_page.set_child(Some(&retry_button));

        let stack = gtk::Stack::new();
        stack.add_named(&scroll, Some("list"));
        stack.add_named(&empty, Some("empty"));
        stack.add_named(&loading, Some("loading"));
        stack.add_named(&error_page, Some("error"));
        stack.set_vexpand(true);

        let hint = text_line("dim-label");
        hint.set_label("");
        hint.set_visible(false);
        let notice = text_line("error");
        notice.set_visible(false);
        let footer = text_line("caption");
        footer.add_css_class("dim-label");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        body.append(&notice);
        body.append(&hint);
        body.append(&stack);
        body.append(&footer);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&body));
        let page = adw::NavigationPage::new(&toolbar, t("section.queue_title"));
        page.set_tag(Some("queue"));

        Self {
            page,
            toolbar,
            header,
            queue_toggle,
            jobs_list,
            stack,
            hint,
            notice,
            footer,
            spinner,
            error_page,
            retry_button,
        }
    }
}

/// Builds the page and hands every interaction to [`super::flows`].
pub fn page(client: &std::rc::Rc<Client>) -> adw::NavigationPage {
    let ui = Ui::build();
    super::flows::wire(client, &ui);
    ui.page
}

/// A row of the list: the mutable widgets [`super::flows`] refines in place.
pub struct Row {
    pub row: gtk::ListBoxRow,
    pub state: gtk::Label,
    pub bar: gtk::ProgressBar,
    pub detail: gtk::Label,
    pub actions: gtk::Box,
}

/// Builds one row: base name, localized preset name, state, bar and actions.
pub fn row(job: &Job, preset_name: &str) -> Row {
    let title = gtk::Label::new(Some(&base_name(&job.input_path)));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    title.add_css_class("heading");
    let preset = gtk::Label::new(Some(preset_name));
    preset.set_xalign(0.0);
    preset.add_css_class("dim-label");
    preset.add_css_class("caption");

    let state = gtk::Label::new(Some(state_label(job.state)));
    state.add_css_class("caption");
    state.add_css_class("accent");

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    top.append(&title);
    top.append(&preset);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    top.append(&spacer);
    top.append(&state);

    let bar = gtk::ProgressBar::new();
    bar.set_hexpand(true);
    bar.set_show_text(false);
    bar.set_valign(gtk::Align::Center);

    let detail = text_line("caption");
    detail.add_css_class("dim-label");

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions.set_halign(gtk::Align::Start);
    actions.set_margin_top(2);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.append(&top);
    body.append(&bar);
    body.append(&detail);
    body.append(&actions);

    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_focusable(false);
    row.set_child(Some(&body));
    row.set_tooltip_text(Some(&job.input_path));

    let widgets = Row {
        row,
        state,
        bar,
        detail,
        actions,
    };
    update_row(&widgets, job);
    widgets
}

/// Refreshes the parts of a row that an event can change, keeping its widgets
/// (and therefore the keyboard focus) alive.
pub fn update_row(widgets: &Row, job: &Job) {
    widgets.state.set_label(state_label(job.state));
    let progress = &job.progress;
    match progress.percent {
        Some(percent) => {
            widgets.bar.set_fraction((percent / 100.0).clamp(0.0, 1.0));
        }
        None => widgets.bar.pulse(),
    }
    widgets.detail.set_label(&progress_label(
        progress.percent,
        progress.fps,
        progress.speed,
        progress.eta_seconds,
    ));
    widgets
        .detail
        .set_visible(job.state == cauce_core::JobState::Running);
}

/// Empties the list just before [`super::flows`] appends the rebuilt rows.
pub fn clear_list(ui: &Ui) {
    while let Some(child) = ui.jobs_list.first_child() {
        ui.jobs_list.remove(&child);
    }
}

/// Copies the parts of the state that are not a row: the phase, the toggle, the
/// pause hint and the footer counters.
pub fn show_phase(ui: &Ui, state: &QueueState) {
    ui.stack.set_visible_child_name(match state.phase() {
        super::state::QueuePhase::Loading => "loading",
        super::state::QueuePhase::Empty => "empty",
        super::state::QueuePhase::Ready => "list",
        super::state::QueuePhase::Error => "error",
    });
    ui.spinner
        .set_spinning(state.phase() == super::state::QueuePhase::Loading);
    if let Some(message) = state.last_error() {
        ui.error_page.set_description(Some(message));
    }
    ui.queue_toggle.set_label(if state.queue_paused() {
        t("queue.resume")
    } else {
        t("queue.pause")
    });
    let hint = state.pause_hint();
    ui.hint.set_visible(hint.is_some());
    if let Some(hint) = hint {
        ui.hint.set_label(hint);
    }
    ui.footer.set_label(&state.summary().footer());
}

/// `AdwAlertDialog` with the job base name, as `docs/ui-mvp.md` §9 asks.
pub(super) fn confirm(ui: &Ui, action: JobAction, name: &str, run: Rc<dyn Fn()>) {
    let (heading, body, accept) = match action {
        JobAction::Cancel => (
            t("queue.confirm_cancel_title"),
            t("queue.confirm_cancel_body").replace("{name}", name),
            t("queue.confirm_cancel_accept"),
        ),
        _ => (
            t("queue.confirm_remove_title"),
            t("queue.confirm_remove_body").replace("{name}", name),
            t("queue.confirm_remove_accept"),
        ),
    };
    let dialog = adw::AlertDialog::new(Some(heading), Some(&body));
    dialog.add_response("back", t("queue.confirm_back"));
    dialog.add_response("confirm", accept);
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("back"));
    dialog.set_close_response("back");
    dialog.connect_response(None, move |_, response| {
        if response == "confirm" {
            run();
        }
    });
    dialog.present(Some(&ui.toolbar));
}

/// Opens the output with the desktop's default handler, never through a shell,
/// and says out loud when the file is gone instead of failing silently.
pub(super) fn build_open(ui: &Ui) -> Open {
    let ui = ui.clone();
    Rc::new(move |job: &Job, folder: bool| {
        let path = if folder {
            parent_of(&job.output_path)
        } else {
            job.output_path.clone()
        };
        if !Path::new(&path).exists() {
            show_notice(
                &ui,
                Some(t("queue.notice_missing").replace("{path}", &path)),
            );
            return;
        }
        let uri = gio::File::for_path(&path).uri();
        if let Err(error) =
            gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
        {
            show_notice(
                &ui,
                Some(
                    t("queue.notice_open_failed")
                        .replace("{path}", &path)
                        .replace("{error}", &error.to_string()),
                ),
            );
        }
    })
}

fn parent_of(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

/// A concrete failure inside the page (`docs/ui-mvp.md` §9).
pub fn show_notice(ui: &Ui, message: Option<String>) {
    match message {
        Some(message) => {
            ui.notice.set_label(&message);
            ui.notice.set_visible(true);
        }
        None => ui.notice.set_visible(false),
    }
}

pub fn headline(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    label
}

pub fn text_line(css: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class(css);
    label
}
