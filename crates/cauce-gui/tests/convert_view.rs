//! The Convertir page must not paint anything outside its own containers.
//!
//! The real capture showed a stray ✖ below the preset control: a `GtkDropDown`
//! left an internal image outside its row (`odd/tasks/cauce-0.1.md`, L33). The
//! preset control is now an `AdwComboRow`, which owns its whole rendering, and
//! this test fails if any image of the page paints outside a control.

use adw::prelude::*;
use cauce_core::{
    AudioPlan, ConversionPlan, HdrAction, MetadataPolicy, StreamAction, SubtitleAction,
    SubtitlePlan, VideoPlan,
};
use cauce_gui::pages::convert::state::{plan_details, plan_headline};
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

/// The page inside a 960×640 window, or nothing without a display.
fn shown_page() -> Option<(adw::ApplicationWindow, gtk::Widget)> {
    let _ = adw::init();
    if !gtk::is_initialized() {
        // Sin display no se puede pintar nada: el resto de la suite corre en CI
        // bajo `xvfb-run`, que es donde este contrato se ejerce de verdad.
        eprintln!("se omite convert_view: hace falta un display (usá xvfb-run)");
        return None;
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
    Some((window, content))
}

/// A literal plan: the three stream families, a container and two reasons.
fn plan() -> ConversionPlan {
    ConversionPlan {
        container: "mp4".to_owned(),
        video: VideoPlan {
            action: StreamAction::Copy,
            encoder: None,
            quality: None,
            max_width: None,
            max_height: None,
            fps: None,
            pixel_format: None,
            profile: None,
            level: None,
            hdr_action: HdrAction::Preserve,
            stream_index: Some(0),
        },
        audio: AudioPlan {
            action: StreamAction::Copy,
            encoder: None,
            bitrate: None,
            channels: None,
            stream_index: Some(1),
            normalize: false,
        },
        subtitles: SubtitlePlan {
            action: SubtitleAction::Remove,
            encoder: None,
            stream_index: None,
            source_index: None,
        },
        faststart: true,
        metadata: MetadataPolicy::Preserve,
        reasoning: vec![
            "video: copy — h264 compatible con mp4".to_owned(),
            "subtítulos: remove — la entrada no declara pistas".to_owned(),
        ],
    }
}

#[test]
fn the_plan_needs_one_visible_line_and_keeps_the_rest_in_the_details() {
    let plan = plan();
    let headline = plan_headline(&plan);
    assert!(
        !headline.contains('\n'),
        "la línea visible del plan no puede romper en varias: {headline:?}"
    );

    let details = plan_details(&plan);
    assert_eq!(
        details.first().map(String::as_str),
        Some(headline.as_str()),
        "el resumen completo sigue disponible dentro de «Detalles»"
    );
    assert_eq!(
        &details[1..],
        plan.reasoning.as_slice(),
        "cada línea de `reasoning[]` aparece en el desplegable"
    );
}

#[test]
fn the_page_shows_a_collapsed_disclosure_and_paints_no_stray_glyph() {
    // Un solo test toca el display: Libadwaita exige el hilo principal para
    // construir sus widgets, y cada test de Cargo corre en su propio hilo.
    let Some((window, content)) = shown_page() else {
        return;
    };

    let mut combos = 0;
    let mut expanders: Vec<gtk::Expander> = Vec::new();
    let mut headline: Option<gtk::Label> = None;
    let mut plan_box: Option<gtk::Box> = None;
    let mut cta: Option<gtk::Button> = None;
    let mut offenders = Vec::new();
    walk(&content, &mut |widget| {
        if widget.downcast_ref::<adw::ComboRow>().is_some() {
            combos += 1;
        }
        if let Some(expander) = widget.downcast_ref::<gtk::Expander>() {
            expanders.push(expander.clone());
        }
        match widget.widget_name().as_str() {
            "plan-headline" => headline = widget.downcast_ref::<gtk::Label>().cloned(),
            "plan-box" => plan_box = widget.downcast_ref::<gtk::Box>().cloned(),
            "queue-cta" => cta = widget.downcast_ref::<gtk::Button>().cloned(),
            _ => {}
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

    assert_eq!(
        expanders.len(),
        1,
        "la página necesita un único desplegable «Detalles»: hay {}",
        expanders.len()
    );
    assert!(
        !expanders[0].is_expanded(),
        "«Detalles» arranca cerrado: el resumen técnico no puede empujar la CTA bajo el pliegue"
    );

    let headline = headline.expect("la línea del plan se llama `plan-headline`");
    assert_eq!(
        headline.lines(),
        1,
        "el resumen visible ocupa una sola línea"
    );
    assert_eq!(
        headline.ellipsize(),
        gtk::pango::EllipsizeMode::End,
        "una línea larga se recorta con puntos suspensivos, no crece en alto"
    );

    // El peor caso de la captura: el plan visible y una línea larga. Con el
    // desplegable cerrado sus `reasoning[]` no ocupan alto y la CTA sigue
    // entrando completa a 960×640.
    headline.set_label(&plan_headline(&plan()));
    plan_box
        .expect("la caja del plan se llama `plan-box`")
        .set_visible(true);
    frames(20);

    let cta = cta.expect("la CTA se llama `queue-cta`");
    assert!(cta.is_mapped(), "la CTA está en pantalla");
    let origin = cta
        .compute_point(&window, &gtk::graphene::Point::new(0.0, 0.0))
        .expect("la CTA se puede situar dentro de la ventana");
    let bottom = origin.y() + cta.height() as f32;
    assert!(
        bottom <= window.height() as f32,
        "«Añadir a la cola» tiene que entrar completa a 960×640: termina en {bottom:.0} de {}",
        window.height()
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
