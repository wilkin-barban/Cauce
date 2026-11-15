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
use crate::i18n::{localized_preset_description, localized_preset_name, t};
use crate::pages::{self, ErrorSurface, error_surface};

/// Catalog key of the drop zone (`docs/ui-mvp.md` §4). The key, not the sentence:
/// `pages::convert` re-exports it and the view translates it at build time.
pub const DROP_HINT: &str = "convert.drop_hint";

/// Catalog key of the card headline and its hint.
pub const DROP_TITLE: &str = "convert.drop_title";
pub const DROP_SUBTITLE: &str = "convert.drop_subtitle";

/// Widget name of the drop card, so a test finds it without a handle.
pub const DROP_CARD: &str = "drop-card";

/// Symbolic icon of the card: the icon theme ships it, so nothing is vendored.
const DROP_ICON: &str = "document-send-symbolic";

/// Every widget of the page, plus the page itself.
#[derive(Clone)]
pub struct Ui {
    pub page: adw::NavigationPage,
    pub toolbar: adw::ToolbarView,
    pub drop_card: gtk::Button,
    pub add_files: gtk::Button,
    pub add_folder: gtk::Button,
    pub files_list: gtk::ListBox,
    pub rejected_list: gtk::ListBox,
    pub rejected_box: gtk::Box,
    pub spinner: gtk::Spinner,
    pub inspecting: gtk::Box,
    pub phase_label: gtk::Label,
    pub failure: ErrorSurface,
    pub output_row: adw::ActionRow,
    pub output_button: gtk::Button,
    pub preset_combo: adw::ComboRow,
    pub preset_group: adw::PreferencesGroup,
    pub plan_box: gtk::Box,
    pub plan_summary_line: gtk::Label,
    pub plan_details: gtk::Expander,
    pub plan_details_line: gtk::Label,
    pub plan_reasons: gtk::Box,
    pub confirm_label: gtk::Label,
    pub queue_button: gtk::Button,
    pub stack: gtk::Stack,
    pub blocked: adw::StatusPage,
}

impl Ui {
    /// Builds every widget of the page. Public so the widget tests can drive one
    /// surface — the failure disclosure — without a whole shell around it.
    pub fn build() -> Self {
        // The card's dashed border and its `drag-over` state are the only CSS
        // Cauce ships; installing it here keeps a page built on its own styled
        // exactly like the one inside the window.
        crate::window::install_styles();

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(t("section.convert_title"), "")));

        let drop_card = drop_card();
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
        // (`docs/ui-mvp.md` §4). The empty state has no sentence of its own: the
        // drop card already says what to do, and saying it twice (`L54`) made the
        // page look like two different instructions.
        let phase_label = text_line("dim-label");
        phase_label.set_label("");
        phase_label.set_visible(false);

        let failure = error_surface("convert-error");

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
        // The description lives with the preset, once: the capture showed it as a
        // detached grey line that also repeated the plan summary (`L43`).
        let preset_group = adw::PreferencesGroup::new();
        preset_group.set_title(t("convert.preset_group"));
        preset_group.add(&preset_combo);

        // One visible line, and the technical detail behind a closed disclosure: the
        // long summary pushed "Añadir a la cola" below the fold at 960×640 (`L43`).
        let plan_summary_line = gtk::Label::new(None);
        plan_summary_line.set_xalign(0.0);
        plan_summary_line.set_lines(1);
        plan_summary_line.set_ellipsize(gtk::pango::EllipsizeMode::End);
        plan_summary_line.set_widget_name("plan-headline");
        let plan_details_line = text_line("dim-label");
        let plan_reasons = gtk::Box::new(gtk::Orientation::Vertical, 2);
        plan_reasons.set_widget_name("plan-reasons");
        let details_body = gtk::Box::new(gtk::Orientation::Vertical, 4);
        details_body.append(&plan_details_line);
        details_body.append(&plan_reasons);
        let plan_details = gtk::Expander::new(Some(t("convert.details")));
        plan_details.set_widget_name("plan-details");
        plan_details.set_expanded(false);
        plan_details.set_child(Some(&details_body));
        let plan_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        plan_box.append(&heading(t("convert.plan_heading")));
        plan_box.append(&plan_summary_line);
        plan_box.append(&plan_details);
        plan_box.set_widget_name("plan-box");
        plan_box.set_visible(false);

        let confirm_label = text_line("success");
        confirm_label.set_visible(false);

        let queue_button = gtk::Button::with_label(t("convert.queue_button"));
        queue_button.set_widget_name("queue-cta");
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
        body.append(&drop_card);
        body.append(&actions);
        body.append(&inspecting);
        body.append(&files_list);
        body.append(&phase_label);
        body.append(&rejected_box);
        body.append(&failure.area);
        body.append(&output_group);
        body.append(&preset_group);
        body.append(&plan_box);
        body.append(&confirm_label);
        body.append(&queue_button);

        // Same card width as Ajustes and Diagnóstico, whose `AdwPreferencesPage`
        // clamps its content, so the shell reads as one interface (`L43`).
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(600);
        clamp.set_tightening_threshold(400);
        clamp.set_child(Some(&body));

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&clamp));
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
            drop_card,
            add_files,
            add_folder,
            files_list,
            rejected_list,
            rejected_box,
            spinner,
            inspecting,
            phase_label,
            failure,
            output_row,
            output_button,
            preset_combo,
            preset_group,
            plan_box,
            plan_summary_line,
            plan_details,
            plan_details_line,
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

/// The drop zone of `docs/ui-mvp.md` §4: a full-width card with a dashed border,
/// a large symbolic icon and two lines of text, instead of a pill that only said
/// "arrastrá".
///
/// It is a `GtkButton` on purpose: the button role is what makes it focusable,
/// what makes `Enter` and `Space` activate it, and what routes the press to the
/// same file chooser as the two explicit buttons.
fn drop_card() -> gtk::Button {
    let card = gtk::Button::new();
    card.set_widget_name(DROP_CARD);
    card.add_css_class("drop-card");
    card.add_css_class("flat");
    card.set_hexpand(true);
    card.set_focusable(true);
    card.set_tooltip_text(Some(t(DROP_HINT)));

    let icon = gtk::Image::from_icon_name(DROP_ICON);
    icon.set_pixel_size(32);
    icon.add_css_class("dim-label");

    let title = gtk::Label::new(Some(t(DROP_TITLE)));
    title.add_css_class("heading");
    let subtitle = text_line("dim-label");
    subtitle.set_label(t(DROP_SUBTITLE));
    subtitle.add_css_class("caption");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_halign(gtk::Align::Center);
    content.append(&icon);
    content.append(&title);
    content.append(&subtitle);

    card.set_child(Some(&content));
    card
}

/// Paints the `drag-over` state of the card: the class exists only while a drag
/// hovers the drop zone, and the stylesheet in [`crate::window`] makes it visible.
pub fn set_drag_over(card: &gtk::Button, active: bool) {
    match active {
        true => card.add_css_class("drag-over"),
        false => card.remove_css_class("drag-over"),
    }
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
    ui.plan_summary_line.set_label("");
    ui.plan_details_line.set_label("");
    // A new plan opens closed, so the page never inherits the height of the last
    // expansion (`L43`).
    ui.plan_details.set_expanded(false);
    ui.plan_box.set_visible(false);
}

/// Truncates the value `AdwComboRow` builds internally and puts the full name in
/// the row's tooltip: the control exposes no accessor for that `GtkLabel`, and a
/// long preset name used to be cut with no way to read it (`L43`).
fn ellipsize_value(row: &adw::ComboRow, selected: &str) {
    row.set_tooltip_text(Some(selected));
    let widget: gtk::Widget = row.clone().upcast();
    ellipsize_labels(&widget, selected);
}

fn ellipsize_labels(widget: &gtk::Widget, selected: &str) {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if label.text() == selected {
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        ellipsize_labels(&current, selected);
        child = current.next_sibling();
    }
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
        match state.error() {
            Some(message) => pages::show_parts(&ui.failure, message, state.error_details()),
            None => pages::clear_failure(&ui.failure),
        }
        ui.output_row
            .set_subtitle(state.output_dir().unwrap_or(t("convert.not_chosen")));
        // The ratified column of the current locale, never a catalog key.
        let preset = state.preset_id().and_then(|id| {
            presets
                .borrow()
                .iter()
                .find(|preset| preset.id == id)
                .cloned()
        });
        ui.phase_label.set_label(match state.phase() {
            ConvertPhase::Ready => t("convert.phase_ready"),
            ConvertPhase::Partial => t("convert.phase_partial"),
            _ => "",
        });
        // Only a non-empty list gets a phase sentence: the empty state is the drop
        // card alone (`L54`).
        ui.phase_label
            .set_visible(state.error().is_none() && !ui.phase_label.text().trim().is_empty());
        ui.preset_group
            .set_description(preset.as_ref().map(localized_preset_description));
        if let Some(preset) = preset {
            ellipsize_value(&ui.preset_combo, localized_preset_name(&preset));
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
