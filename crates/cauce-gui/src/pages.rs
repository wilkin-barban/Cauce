//! The five sections of the MVP shell: Convertir, Cola, Presets, Diagnóstico and
//! Ajustes (`docs/ui-mvp.md` §2 and `docs/architecture.md` §13).
//!
//! Convertir is a real page since T16 and lives in [`convert`]. The other four are
//! still an `AdwNavigationPage` holding an `AdwToolbarView` with a short
//! `AdwStatusPage` that says what the section will do, so T17-T18 can replace the
//! body without touching the window. Icons are symbolic names the Adwaita theme
//! already ships: nothing here borrows an asset from another project.

use std::rc::Rc;

use adw::prelude::*;
use libadwaita as adw;

use crate::client::Client;

pub mod convert;

/// One entry of the sidebar and its placeholder page.
pub struct Section {
    /// Stable identifier, also the widget name used by tests.
    pub id: &'static str,
    /// Sidebar label and page title.
    pub title: &'static str,
    /// Symbolic icon shipped by the icon theme.
    pub icon: &'static str,
    /// Headline of the placeholder.
    pub headline: &'static str,
    /// One short paragraph: what the page will do.
    pub description: &'static str,
}

/// The five, and only five, sections of `docs/ui-mvp.md`.
pub const SECTIONS: [Section; 5] = [
    Section {
        id: "convert",
        title: "Convertir",
        icon: "video-x-generic-symbolic",
        headline: "Convertir archivos y carpetas",
        description: "Arrastrá videos, audios o una carpeta para inspeccionarlos con FFprobe, \
                      elegir uno de los 13 presets del sistema y añadirlos a la cola.",
    },
    Section {
        id: "queue",
        title: "Cola",
        icon: "view-list-symbolic",
        headline: "Seguir la cola de conversiones",
        description: "Acá vas a ver el progreso real de cada trabajo, con FPS, velocidad y tiempo \
                      restante, y podrás pausar, reanudar, cancelar o reintentar.",
    },
    Section {
        id: "presets",
        title: "Presets",
        icon: "starred-symbolic",
        headline: "Los trece presets del sistema",
        description: "Un catálogo de solo lectura que explica qué copia y qué recodifica cada \
                      preset. En v0.1 no se pueden crear ni editar presets.",
    },
    Section {
        id: "diagnostics",
        title: "Diagnóstico",
        icon: "utilities-system-monitor-symbolic",
        headline: "Revisar el entorno de conversión",
        description: "Versiones de Cauce, del daemon y de FFmpeg, encoders disponibles, CPU, \
                      disco y estado del socket, con acciones cuando algo falta.",
    },
    Section {
        id: "settings",
        title: "Ajustes",
        icon: "preferences-system-symbolic",
        headline: "Ajustar Cauce a tu equipo",
        description: "Carpeta de salida, concurrencia, tema, idioma y notificaciones. Cada cambio \
                      se valida y se guarda a través del daemon.",
    },
];

/// Builds the placeholder page for one section that T17-T18 will fill in.
pub fn placeholder(section: &Section) -> adw::NavigationPage {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(section.title, "")));

    let status = adw::StatusPage::new();
    status.set_icon_name(Some(section.icon));
    status.set_title(section.headline);
    status.set_description(Some(section.description));
    status.set_hexpand(true);
    status.set_vexpand(true);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&status));

    let page = adw::NavigationPage::new(&toolbar, section.title);
    page.set_tag(Some(section.id));
    page
}

/// Builds every section, in sidebar order: Convertir first, then the placeholders.
pub fn all(client: &Rc<Client>) -> Vec<adw::NavigationPage> {
    let mut pages = Vec::with_capacity(SECTIONS.len());
    for section in SECTIONS.iter() {
        if section.id == "convert" {
            pages.push(convert::page(client));
        } else {
            pages.push(placeholder(section));
        }
    }
    pages
}

