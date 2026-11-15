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

use libadwaita as adw;

use crate::client::Client;

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
    adw::glib::markup_escape_text(text).to_string()
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

thread_local! {
    /// The output folder the whole shell agrees on in this session.
    static OUTPUT_DIR: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The page that repaints when [`set_output_dir`] lands; the last one wins,
    /// exactly like the locale hook, so a rebuilt page never leaves a stale copy.
    static OUTPUT_DIR_LISTENER: RefCell<Option<OutputDirListener>> = const { RefCell::new(None) };
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
