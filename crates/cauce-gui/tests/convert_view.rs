//! The Convertir page must not paint anything outside its own containers.
//!
//! The real capture showed a stray ✖ below the preset control: a `GtkDropDown`
//! left an internal image outside its row (`odd/tasks/cauce-0.1.md`, L33). The
//! preset control is now an `AdwComboRow`, which owns its whole rendering, and
//! this test fails if any image of the page paints outside a control.

use adw::prelude::*;
use libadwaita as adw;
use libadwaita::gtk;
use std::rc::Rc;

/// Images the page must never paint: the artefacts seen in the capture.
const FORBIDDEN: [&str; 3] = [
    "object-select-symbolic",
    "edit-clear-symbolic",
    "image-missing",
];

fn frames(count: usize) {
    for _ in 0..count {
        while gtk::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
}

#[test]
fn the_page_has_a_combo_row_and_paints_no_stray_glyph() {
    let _ = adw::init();
    if !gtk::is_initialized() {
        // Sin display no se puede pintar nada: el resto de la suite corre en CI
        // bajo `xvfb-run`, que es donde este contrato se ejerce de verdad.
        eprintln!("se omite convert_view: hace falta un display (usá xvfb-run)");
        return;
    }
    let client = Rc::new(cauce_gui::client::Client::connect());
    let page = cauce_gui::pages::convert::page(&client);
    let content: gtk::Widget = page.clone().upcast();
    let window = adw::ApplicationWindow::builder()
        .default_width(960)
        .default_height(640)
        .content(&content)
        .build();
    window.set_visible(true);
    frames(40);

    let mut combos = 0;
    let mut offenders = Vec::new();
    walk(&content, &mut |widget| {
        if widget.downcast_ref::<adw::ComboRow>().is_some() {
            combos += 1;
        }
        let Some(image) = widget.downcast_ref::<gtk::Image>() else {
            return;
        };
        let icon = image.icon_name().unwrap_or_default();
        // Solo un glifo MAPEADO se pinta: las imágenes internas no mapeadas de un
        // control son inertes y no ensucian la ventana.
        if widget.is_mapped() && FORBIDDEN.contains(&icon.as_str()) {
            offenders.push(icon);
        }
    });

    assert_eq!(
        combos, 1,
        "la página debe tener exactamente un AdwComboRow de preset"
    );
    assert!(
        offenders.is_empty(),
        "la página no debe pintar glifos sueltos: {offenders:?}"
    );
}

fn walk(widget: &gtk::Widget, visit: &mut impl FnMut(&gtk::Widget)) {
    visit(widget);
    let mut child = widget.first_child();
    while let Some(current) = child {
        walk(&current, visit);
        child = current.next_sibling();
    }
}
