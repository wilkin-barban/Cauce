//! The five sections of the MVP shell: Convertir, Cola, Presets, Diagnóstico and
//! Ajustes (`docs/ui-mvp.md` §2 and `docs/architecture.md` §13).
//!
//! Every section is a real page: Convertir since T16, Cola since T17 and the
//! other three since T18. Each page owns its own file, so the shell only decides
//! the sidebar order and the icon of each entry. Icons are symbolic names the
//! Adwaita theme already ships: nothing here borrows an asset from another
//! project. Since T19 an entry carries the *keys* of its names, never the names
//! themselves, so switching the locale re-renders the sidebar.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::{glib, gtk};
use cauce_core::JobError;
use libadwaita as adw;

use crate::client::Client;
use crate::i18n;

pub mod convert;
pub mod diagnostics;
pub mod presets;
pub mod queue;
pub mod settings;

/// One entry of the sidebar and its page.
pub struct Section {
    /// Stable identifier, also the widget name used by tests.
    pub id: &'static str,
    /// Catalog key of the sidebar label and the page title.
    pub title_key: &'static str,
    /// Catalog key of the one-line description of the section.
    pub description_key: &'static str,
    /// Symbolic icon shipped by the icon theme.
    pub icon: &'static str,
}

/// The five, and only five, sections of `docs/ui-mvp.md`.
pub const SECTIONS: [Section; 5] = [
    Section {
        id: "convert",
        title_key: "section.convert_title",
        description_key: "section.convert_description",
        icon: "video-x-generic-symbolic",
    },
    Section {
        id: "queue",
        title_key: "section.queue_title",
        description_key: "section.queue_description",
        icon: "view-list-symbolic",
    },
    Section {
        id: "presets",
        title_key: "section.presets_title",
        description_key: "section.presets_description",
        icon: "starred-symbolic",
    },
    Section {
        id: "diagnostics",
        title_key: "section.diagnostics_title",
        description_key: "section.diagnostics_description",
        // `utilities-system-monitor-symbolic` is absent from the Adwaita theme
        // installed here, so the sidebar painted a faint missing-image box (`L43`).
        icon: "applications-system-symbolic",
    },
    Section {
        id: "settings",
        title_key: "section.settings_title",
        description_key: "section.settings_description",
        icon: "preferences-system-symbolic",
    },
];

/// Escapes text that comes from the daemon before it reaches an `AdwActionRow`:
/// those rows read title and subtitle as Pango markup, and the canonical preset
/// names legitimately contain `&` (`docs/presets.md`).
pub fn escaped(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

/// The one failure surface of every page (`docs/ui-mvp.md` §9): a human sentence
/// on top and, behind a closed `GtkExpander`, the code and the daemon's own
/// message.
///
/// The rule the widget enforces is the one `docs/contracts.md` §8 asks for: the
/// user reads what happened and what to do, and the raw text is one deliberate
/// click away instead of being the headline.
#[derive(Clone)]
pub struct ErrorSurface {
    /// What the page appends to its body; hidden until there is a failure.
    pub area: gtk::Box,
    /// The human sentence.
    pub message: gtk::Label,
    /// «Detalles técnicos», closed by default.
    pub expander: gtk::Expander,
    /// The code and the daemon's message.
    pub details: gtk::Label,
}

/// Builds a hidden [`ErrorSurface`]; `name` is the widget-name prefix the widget
/// tests look for.
pub fn error_surface(name: &str) -> ErrorSurface {
    let message = gtk::Label::new(None);
    message.set_wrap(true);
    message.set_xalign(0.0);
    message.add_css_class("error");
    message.set_widget_name(&format!("{name}-message"));

    let details = error_details_label();
    details.set_widget_name(&format!("{name}-details"));

    let expander = gtk::Expander::new(Some(i18n::t("error.technical_details")));
    expander.set_expanded(false);
    expander.set_child(Some(&details));
    expander.set_widget_name(&format!("{name}-expander"));

    let area = gtk::Box::new(gtk::Orientation::Vertical, 4);
    area.append(&message);
    area.append(&expander);
    area.set_widget_name(name);
    area.set_visible(false);

    ErrorSurface {
        area,
        message,
        expander,
        details,
    }
}

/// The label that holds the technical detail inside a disclosure: small, dim and
/// wrapped, never the headline.
pub fn error_details_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label
}

/// Shows a daemon failure: the human sentence on top, the raw text inside the
/// disclosure, and the disclosure reopened closed.
pub fn show_failure(surface: &ErrorSurface, error: &JobError) {
    show_parts(
        surface,
        &i18n::human_error(error.code, &error.message),
        Some(&i18n::technical_details(error.code, &error.message)),
    );
}

/// Shows a message Cauce itself wrote, with or without a technical part.
pub fn show_parts(surface: &ErrorSurface, message: &str, details: Option<&str>) {
    surface.message.set_label(message);
    match details.filter(|details| !details.trim().is_empty()) {
        Some(details) => {
            surface.details.set_label(details);
            surface.expander.set_visible(true);
            surface.expander.set_expanded(false);
        }
        None => {
            surface.details.set_label("");
            surface.expander.set_visible(false);
        }
    }
    surface.area.set_visible(true);
}

/// Hides the surface again, so a recovered page carries no stale failure.
pub fn clear_failure(surface: &ErrorSurface) {
    surface.area.set_visible(false);
}

/// Builds every section, in sidebar order.
pub fn all(client: &Rc<Client>) -> Vec<adw::NavigationPage> {
    vec![
        convert::page(client),
        queue::page(client),
        presets::page(client),
        diagnostics::page(client),
        settings::page(client),
    ]
}

/// The hook the Ajustes page registers to follow [`set_output_dir`].
type OutputDirListener = Rc<dyn Fn(&str)>;

/// The hook Convertir registers to follow [`presets_changed`].
type PresetsListener = Rc<dyn Fn()>;

thread_local! {
    /// The output folder the whole shell agrees on in this session.
    static OUTPUT_DIR: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The page that repaints when [`set_output_dir`] lands; the last one wins,
    /// exactly like the locale hook, so a rebuilt page never leaves a stale copy.
    static OUTPUT_DIR_LISTENER: RefCell<Option<OutputDirListener>> = const { RefCell::new(None) };
    /// The page that repaints when the preset catalog changes: saving or deleting
    /// the custom preset has to reach the Convertir combo without a restart.
    static PRESETS_LISTENER: RefCell<Option<PresetsListener>> = const { RefCell::new(None) };
}

/// The output folder Convertir chose in this session, if it chose one.
///
/// Convertir keeps the folder in its own state and Ajustes reads `settings.get`,
/// so before this value existed the two pages disagreed whenever the daemon had
/// not persisted the choice yet (`odd/tasks/cauce-0.1.md`, L43).
pub fn output_dir() -> Option<String> {
    OUTPUT_DIR.with(|slot| slot.borrow().clone())
}

/// Records the folder Convertir just chose and asks Ajustes to render it. An
/// empty value never replaces a usable folder.
pub fn set_output_dir(dir: &str) {
    let dir = dir.trim();
    if dir.is_empty() {
        return;
    }
    OUTPUT_DIR.with(|slot| *slot.borrow_mut() = Some(dir.to_owned()));
    let listener = OUTPUT_DIR_LISTENER.with(|slot| slot.borrow().clone());
    if let Some(listener) = listener {
        listener(dir);
    }
}

/// Registers the Ajustes renderer; the last registration wins.
pub fn on_output_dir_change(listener: OutputDirListener) {
    OUTPUT_DIR_LISTENER.with(|slot| *slot.borrow_mut() = Some(listener));
}

/// Announced after `presets.set_custom` or `presets.clear_custom` succeeded, so
/// Convertir re-reads `presets.list` and the saved preset appears in its combo.
pub fn presets_changed() {
    let listener = PRESETS_LISTENER.with(|slot| slot.borrow().clone());
    if let Some(listener) = listener {
        listener();
    }
}

/// Registers the Convertir reload; the last registration wins, like the other
/// shell hooks, so a rebuilt page never leaves a stale copy.
pub fn on_presets_change(listener: PresetsListener) {
    PRESETS_LISTENER.with(|slot| *slot.borrow_mut() = Some(listener));
}

/// Moves the shell to `id` from a widget inside a page, without reaching back to
/// the window object: the sidebar row is selected, which is the same signal a
/// person's click sends, so the highlight and the content can never disagree.
///
/// Outside a shell — a page built on its own by a test — there is nothing to
/// select and the call is a no-op.
pub fn open_section(from: &impl IsA<gtk::Widget>, id: &str) {
    let Some(index) = SECTIONS.iter().position(|section| section.id == id) else {
        return;
    };
    let widget = from.clone().upcast::<gtk::Widget>();
    let Some(split) = widget
        .ancestor(adw::NavigationSplitView::static_type())
        .and_then(|ancestor| ancestor.downcast::<adw::NavigationSplitView>().ok())
    else {
        return;
    };
    let Some(sidebar) = split.sidebar() else {
        return;
    };
    let sidebar: gtk::Widget = sidebar.upcast();
    let Some(list) = find_sidebar_list(&sidebar) else {
        return;
    };
    if let Some(row) = list.row_at_index(i32::try_from(index).unwrap_or(0)) {
        list.select_row(Some(&row));
    }
}

/// The `GtkListBox` of the sidebar: the first list that carries one row per
/// section, so a list inside a page can never be mistaken for it.
fn find_sidebar_list(widget: &gtk::Widget) -> Option<gtk::ListBox> {
    if let Some(list) = widget.downcast_ref::<gtk::ListBox>()
        && list.observe_children().n_items() == SECTIONS.len() as u32
    {
        return Some(list.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_sidebar_list(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}
