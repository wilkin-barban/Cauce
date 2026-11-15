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
use cauce_core::{Job, JobState};
use libadwaita as adw;

use super::state::{JobAction, QueueState, progress_label, state_label};
use crate::client::Client;
use crate::i18n::{t, t_with};
use crate::pages::convert::base_name;
use crate::pages::{self, ErrorSurface, error_surface};

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
    pub clear_button: gtk::Button,
    pub summary: gtk::Label,
    pub cleared: gtk::Label,
    pub jobs_list: gtk::ListBox,
    pub stack: gtk::Stack,
    pub hint: gtk::Label,
    pub notice: ErrorSurface,
    pub footer: gtk::Label,
    pub spinner: gtk::Spinner,
    pub error_page: adw::StatusPage,
    pub error_expander: gtk::Expander,
    pub error_details: gtk::Label,
    pub retry_button: gtk::Button,
}

impl Ui {
    fn build() -> Self {
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(t("section.queue_title"), "")));
        // A real button, not grey text: the capture showed "Pausar cola" as if it
        // were a label and nothing suggested it could be pressed (`L43`).
        let queue_toggle = gtk::Button::with_label(t("queue.pause"));
        queue_toggle.add_css_class("pill");
        queue_toggle.set_tooltip_text(Some(t("queue.toggle_tooltip")));
        // «Limpiar lista» removes only the finished jobs, asks first, and is
        // disabled while there is nothing finished (`docs/ui-mvp.md` §5).
        let clear_button = gtk::Button::with_label(t("queue.clear_finished"));
        clear_button.set_widget_name("queue-clear-finished");
        clear_button.add_css_class("pill");
        clear_button.set_tooltip_text(Some(t("queue.clear_tooltip")));
        clear_button.set_sensitive(false);
        header.pack_end(&clear_button);
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

        // The error page says what happened in one sentence and keeps the code
        // and the daemon's wording behind the closed disclosure (`L53`, §8).
        let error_page = adw::StatusPage::new();
        error_page.set_icon_name(Some("dialog-error-symbolic"));
        error_page.set_title(t("queue.error_title"));
        let retry_button = gtk::Button::with_label(t("queue.retry"));
        retry_button.add_css_class("pill");
        retry_button.set_halign(gtk::Align::Center);

        let error_details = text_line("caption");
        error_details.add_css_class("dim-label");
        let error_expander = gtk::Expander::new(Some(t("error.technical_details")));
        error_expander.set_expanded(false);
        error_expander.set_child(Some(&error_details));
        let error_body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        error_body.set_halign(gtk::Align::Center);
        error_body.append(&error_expander);
        error_body.append(&retry_button);
        error_page.set_child(Some(&error_body));

        let stack = gtk::Stack::new();
        stack.add_named(&scroll, Some("list"));
        stack.add_named(&empty, Some("empty"));
        stack.add_named(&loading, Some("loading"));
        stack.add_named(&error_page, Some("error"));
        stack.set_vexpand(true);

        let hint = text_line("dim-label");
        hint.set_label("");
        hint.set_visible(false);
        let notice = error_surface("queue-notice");
        // The four counters above the list: total, active, waiting and finished.
        let summary = text_line("caption");
        summary.set_widget_name("queue-summary");
        summary.add_css_class("dim-label");
        // What the last bulk clear did, so the rows never vanish in silence.
        let cleared = text_line("success");
        cleared.set_widget_name("queue-cleared");
        cleared.set_visible(false);
        let footer = text_line("caption");
        footer.add_css_class("dim-label");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        body.append(&notice.area);
        body.append(&hint);
        body.append(&summary);
        body.append(&cleared);
        body.append(&stack);
        body.append(&footer);

        // Same card width as Ajustes and Diagnóstico, whose `AdwPreferencesPage`
        // clamps its content (`L43`).
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(600);
        clamp.set_tightening_threshold(400);
        clamp.set_child(Some(&body));

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&clamp));
        let page = adw::NavigationPage::new(&toolbar, t("section.queue_title"));
        page.set_tag(Some("queue"));

        Self {
            page,
            toolbar,
            header,
            queue_toggle,
            clear_button,
            summary,
            cleared,
            jobs_list,
            stack,
            hint,
            notice,
            footer,
            spinner,
            error_page,
            error_expander,
            error_details,
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
    preset.set_ellipsize(gtk::pango::EllipsizeMode::End);
    preset.set_tooltip_text(Some(preset_name));
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

/// The fraction the row's bar paints (`None` = indeterminate).
///
/// A completed job paints full whatever the last sample said: the capture showed
/// "Completada" next to a bar stuck at 96 % (`odd/tasks/cauce-0.1.md`, L43).
/// It is GTK-free and lives here, next to its only caller, to keep the decision
/// core inside the 400-line rule.
pub fn bar_fraction(state: JobState, percent: Option<f64>) -> Option<f64> {
    match (state, percent) {
        (JobState::Completed, _) => Some(1.0),
        (_, Some(percent)) => Some((percent / 100.0).clamp(0.0, 1.0)),
        (_, None) => None,
    }
}

/// Refreshes the parts of a row that an event can change, keeping its widgets
/// (and therefore the keyboard focus) alive.
pub fn update_row(widgets: &Row, job: &Job) {
    widgets.state.set_label(state_label(job.state));
    let progress = &job.progress;
    // A completed job paints full: the bar never shows a stale 96 % (`L43`).
    match bar_fraction(job.state, progress.percent) {
        Some(fraction) => widgets.bar.set_fraction(fraction),
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
    ui.error_expander
        .set_visible(state.last_details().is_some());
    ui.error_details
        .set_label(state.last_details().unwrap_or_default());
    ui.queue_toggle.set_label(t(state.toggle()));
    let hint = state.pause_hint();
    ui.hint.set_visible(hint.is_some());
    if let Some(hint) = hint {
        ui.hint.set_label(hint);
    }
    ui.footer.set_label(&state.summary().footer());
    let summary = state.summary();
    ui.summary.set_label(&summary.headline());
    ui.clear_button.set_sensitive(state.can_clear());
    let notice = state.notice();
    ui.cleared.set_visible(notice.is_some());
    ui.cleared.set_label(notice.unwrap_or_default());
}

/// `AdwAlertDialog` with the number of finished jobs a bulk clear would remove,
/// so the count is never a surprise (`docs/ui-mvp.md` §9).
pub(super) fn confirm_clear(ui: &Ui, count: usize, run: Rc<dyn Fn()>) {
    let body = match count {
        1 => t("queue.confirm_clear_body_one").to_owned(),
        n => t_with("queue.confirm_clear_body_many", "{n}", &n.to_string()),
    };
    let dialog = adw::AlertDialog::new(Some(t("queue.confirm_clear_title")), Some(&body));
    dialog.add_response("back", t("queue.confirm_back"));
    dialog.add_response("confirm", t("queue.confirm_clear_accept"));
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

/// A concrete failure inside the page (`docs/ui-mvp.md` §9). `message` is a
/// sentence Cauce already wrote, so there is nothing to hide.
pub fn show_notice(ui: &Ui, message: Option<String>) {
    match message {
        Some(message) => pages::show_parts(&ui.notice, &message, None),
        None => pages::clear_failure(&ui.notice),
    }
}

/// A daemon failure: the human sentence is visible and the raw text waits inside
/// the disclosure (`docs/contracts.md` §8).
pub fn show_failure(ui: &Ui, error: &cauce_core::JobError) {
    pages::show_failure(&ui.notice, error);
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
