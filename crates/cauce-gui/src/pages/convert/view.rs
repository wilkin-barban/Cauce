//! Widgets of the Convertir page (`docs/ui-mvp.md` §4 and §10).
//!
//! [`Ui`] owns every handle the page needs after construction, so the flows in
//! [`super::flows`] can clone it instead of juggling dozens of individual
//! widgets. Nothing here decides anything: the state machine lives in
//! [`super::state`]. Every icon-only affordance is avoided, the lists are
//! `GtkListBox` widgets (their own accessible role is `list`) and no widget has a
//! fixed pixel height, so the page follows the text scale.

use std::cell::RefCell;
use std::rc::Rc;

use adw::gtk;
use adw::prelude::*;
use libadwaita as adw;

use super::flows;
use cauce_core::{MediaSnapshot, Preset};

use super::state::{
    ConvertPhase, ConvertState, FFMPEG_COMMAND, RejectedInput, base_name, summarize,
};
use crate::client::Client;
use crate::i18n::{localized_preset_description, t};

/// Catalog key of the drop zone (`docs/ui-mvp.md` §4). The key, not the sentence:
/// `pages::convert` re-exports it and the view translates it at build time.
pub const DROP_HINT: &str = "convert.drop_hint";

/// Every widget of the page, plus the page itself.
#[derive(Clone)]
pub struct Ui {
    pub page: adw::NavigationPage,
    pub toolbar: adw::ToolbarView,
    pub drop_zone: gtk::Button,
    pub add_files: gtk::Button,
    pub add_folder: gtk::Button,
    pub files_list: gtk::ListBox,
    pub rejected_list: gtk::ListBox,
    pub rejected_box: gtk::Box,
    pub spinner: gtk::Spinner,
    pub inspecting: gtk::Box,
    pub phase_label: gtk::Label,
    pub error_label: gtk::Label,
    pub output_row: adw::ActionRow,
    pub output_button: gtk::Button,
    pub preset_combo: adw::ComboRow,
    pub preset_description: gtk::Label,
    pub plan_box: gtk::Box,
    pub plan_summary_line: gtk::Label,
    pub plan_reasons: gtk::Box,
    pub confirm_label: gtk::Label,
    pub queue_button: gtk::Button,
    pub stack: gtk::Stack,
    pub blocked: adw::StatusPage,
}

impl Ui {
    fn build() -> Self {
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(t("section.convert_title"), "")));

        let drop_zone = gtk::Button::with_label(t(DROP_HINT));
        drop_zone.add_css_class("pill");
        drop_zone.set_margin_top(18);
        drop_zone.set_margin_bottom(18);
        let add_files = gtk::Button::with_label(t("convert.add_files"));
        let add_folder = gtk::Button::with_label(t("convert.add_folder"));
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&add_files);
        actions.append(&add_folder);

        let files_list = readable_list();
        let rejected_list = readable_list();
        let rejected_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        rejected_box.append(&heading(t("convert.rejected")));
        rejected_box.append(&rejected_list);
        rejected_box.set_visible(false);

        let spinner = gtk::Spinner::new();
        let inspecting = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        inspecting.append(&spinner);
        inspecting.append(&gtk::Label::new(Some(t("convert.inspecting"))));
        inspecting.set_visible(false);

        // Empty, ready and partial are sentences, never only the spinner state
        // (`docs/ui-mvp.md` §4).
        let phase_label = text_line("dim-label");
        phase_label.set_label(t("convert.phase_empty"));

        let error_label = text_line("error");
        error_label.set_visible(false);

        let output_row = adw::ActionRow::new();
        output_row.set_title(t("convert.output_folder"));
        output_row.set_subtitle(t("convert.not_chosen"));
        let output_button = gtk::Button::with_label(t("convert.choose"));
        output_button.add_css_class("flat");
        output_row.add_suffix(&output_button);
        let output_group = adw::PreferencesGroup::new();
        output_group.set_title(t("convert.output_group"));
        output_group.add(&output_row);

        // `AdwComboRow` owns its own list and selected-state rendering, so the page
        // never nests a `GtkDropDown` whose internal images escaped their container
        // (recorded in `odd/tasks/cauce-0.1.md`, L33).
        let preset_combo = adw::ComboRow::new();
        preset_combo.set_title(t("convert.output_format"));
        preset_combo.set_model(Some(&gtk::StringList::new(&[t("convert.loading_presets")])));
        let preset_description = text_line("dim-label");
        preset_description.set_visible(false);
        let preset_group = adw::PreferencesGroup::new();
        preset_group.set_title(t("convert.preset_group"));
        preset_group.add(&preset_combo);

        let plan_summary_line = text_line("body");
        let plan_reasons = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let plan_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        plan_box.append(&heading(t("convert.plan_heading")));
        plan_box.append(&plan_summary_line);
        plan_box.append(&plan_reasons);
        plan_box.set_visible(false);

        let confirm_label = text_line("success");
        confirm_label.set_visible(false);

        let queue_button = gtk::Button::with_label(t("convert.queue_button"));
        queue_button.add_css_class("suggested-action");
        queue_button.add_css_class("pill");
        queue_button.set_halign(gtk::Align::Start);
        queue_button.set_sensitive(false);

        let blocked = adw::StatusPage::new();
        blocked.set_icon_name(Some("dialog-warning-symbolic"));
        blocked.set_title(t("convert.blocked_title"));
        let copy_button = gtk::Button::with_label(
            &t("convert.copy_command").replace("{command}", FFMPEG_COMMAND),
        );
        copy_button.add_css_class("pill");
        blocked.set_child(Some(&copy_button));
        copy_button.connect_clicked(|button| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(FFMPEG_COMMAND);
            }
            button.set_label(t("convert.command_copied"));
        });

        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.set_margin_top(12);
        body.set_margin_bottom(12);
        body.set_margin_start(12);
        body.set_margin_end(12);
        body.append(&drop_zone);
        body.append(&actions);
        body.append(&inspecting);
        body.append(&files_list);
        body.append(&phase_label);
        body.append(&rejected_box);
        body.append(&error_label);
        body.append(&output_group);
        body.append(&preset_group);
        body.append(&preset_description);
        body.append(&plan_box);
        body.append(&confirm_label);
        body.append(&queue_button);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&body));
        scroll.set_vexpand(true);
        let stack = gtk::Stack::new();
        stack.add_named(&scroll, Some("main"));
        stack.add_named(&blocked, Some("blocked"));

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&stack));
        let page = adw::NavigationPage::new(&toolbar, t("section.convert_title"));
        page.set_tag(Some("convert"));

        Self {
            page,
            toolbar,
            drop_zone,
            add_files,
            add_folder,
            files_list,
            rejected_list,
            rejected_box,
            spinner,
            inspecting,
            phase_label,
            error_label,
            output_row,
            output_button,
            preset_combo,
            preset_description,
            plan_box,
            plan_summary_line,
            plan_reasons,
            confirm_label,
            queue_button,
            stack,
            blocked,
        }
    }
}

/// Builds the page and hands every interaction to [`super::flows`].
pub fn page(client: &Rc<Client>) -> adw::NavigationPage {
    let ui = Ui::build();
    let state = Rc::new(RefCell::new(ConvertState::new()));
    flows::wire(client, &ui, &state);
    ui.page
}

/// The toplevel a file dialog should be modal to.
pub fn parent_window(widget: &gtk::Button) -> Option<gtk::Window> {
    widget.root().and_then(|root| root.downcast().ok())
}

pub fn heading(text: &str) -> gtk::Label {
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

/// A list `GtkListBox` announces as a list on its own (`docs/ui-mvp.md` §10).
fn readable_list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list
}

pub fn clear(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

pub fn clear_plan(ui: &Ui) {
    while let Some(child) = ui.plan_reasons.first_child() {
        ui.plan_reasons.remove(&child);
    }
    ui.plan_box.set_visible(false);
}

pub fn file_row(snapshot: &MediaSnapshot) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&base_name(&snapshot.path));
    row.set_subtitle(&summarize(snapshot));
    row
}

pub fn rejected_row(rejected: &RejectedInput) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&base_name(&rejected.path));
    row.set_subtitle(&rejected.message);
    row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    row
}

/// Renders the state into the widgets: lists, spinner, messages and the button.
pub fn refresh(
    ui: &Ui,
    state: &Rc<RefCell<ConvertState>>,
    presets: &Rc<RefCell<Vec<Preset>>>,
) -> Rc<dyn Fn()> {
    let (ui, state, presets) = (ui.clone(), Rc::clone(state), Rc::clone(presets));
    Rc::new(move || {
        let state = state.borrow();
        clear(&ui.files_list);
        for snapshot in state.accepted() {
            ui.files_list.append(&file_row(snapshot));
        }
        clear(&ui.rejected_list);
        for rejected in state.rejected() {
            ui.rejected_list.append(&rejected_row(rejected));
        }
        ui.rejected_box.set_visible(!state.rejected().is_empty());
        let busy = state.phase() == ConvertPhase::Inspecting;
        ui.spinner.set_spinning(busy);
        ui.inspecting.set_visible(busy);
        ui.error_label.set_label(state.error().unwrap_or_default());
        ui.error_label.set_visible(state.error().is_some());
        ui.output_row
            .set_subtitle(state.output_dir().unwrap_or(t("convert.not_chosen")));
        // The ratified column of the current locale, never a catalog key.
        let description = state.preset_id().and_then(|id| {
            presets
                .borrow()
                .iter()
                .find(|preset| preset.id == id)
                .map(|preset| localized_preset_description(preset).to_owned())
        });
        ui.phase_label.set_visible(state.error().is_none());
        ui.phase_label.set_label(match state.phase() {
            ConvertPhase::Empty => t("convert.phase_empty"),
            ConvertPhase::Ready => t("convert.phase_ready"),
            ConvertPhase::Partial => t("convert.phase_partial"),
            _ => "",
        });
        ui.preset_description.set_visible(description.is_some());
        if let Some(text) = description {
            ui.preset_description.set_label(&text);
        }
        ui.queue_button.set_sensitive(state.can_queue());
        match state.blocking() {
            Some(message) => {
                ui.blocked.set_description(Some(message));
                ui.stack.set_visible_child_name("blocked");
            }
            None => ui.stack.set_visible_child_name("main"),
        }
    })
}
