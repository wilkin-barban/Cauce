//! The Convertir page must not paint anything outside its own containers, and its
//! drop zone must be a real target, not a pill. `gtk4` only accepts widget work
//! from the thread that initialised it, so [`on_gtk_thread`] keeps one GTK thread.

use adw::prelude::*;
use cauce_core::{
    AudioPlan, ConversionPlan, ErrorCode, HdrAction, JobError, MetadataPolicy, StreamAction,
    SubtitleAction, SubtitlePlan, VideoPlan,
};
use cauce_gui::i18n::{human_error, t, technical_details};
use cauce_gui::pages::convert::state::{
    ConvertState, base_name, expand_dropped_paths, plan_details, plan_headline, summarize,
};
use cauce_gui::pages::convert::view;
use cauce_gui::pages::convert::view::Ui;
use cauce_gui::window::STYLE_SHEET;
use libadwaita as adw;
use libadwaita::gtk;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{OnceLock, mpsc};

/// Images the page must never paint: the artefacts seen in the capture.
const FORBIDDEN: [&str; 3] = [
    "object-select-symbolic",
    "edit-clear-symbolic",
    "image-missing",
];

/// Widget name of the drop card, so a test can find it without a handle.
const DROP_CARD: &str = "drop-card";

/// The real name that rendered as an empty row — the `&` opens a Pango entity
/// the row never closes — and a hostile one with every markup character.
const MARKUP_NAMES: [&str; 2] = [
    "They Call Me Fáfnir | Cinematic Viking Neofolk | Female Vocals & Nordic Atmosphere.mp4",
    "a <b>bold</b> \"quote\" 'apos' & nombre.mkv",
];

/// Something ran on the GTK thread: `None` means there was no display.
type Outcome = Result<(), Box<dyn std::any::Any + Send + 'static>>;
type Task = (
    Box<dyn FnOnce() + Send + 'static>,
    mpsc::Sender<Option<Outcome>>,
);

/// Runs `task` on the GTK thread; `false` means there is no display to test with.
fn on_gtk_thread(task: impl FnOnce() + Send + 'static) -> bool {
    static SENDER: OnceLock<mpsc::Sender<Task>> = OnceLock::new();
    let sender = SENDER.get_or_init(|| {
        let (sender, tasks) = mpsc::channel::<Task>();
        std::thread::Builder::new()
            .name("cauce-gtk-tests".to_owned())
            .spawn(move || {
                let ready = adw::init().is_ok() && gtk::gdk::Display::default().is_some();
                while let Ok((task, reply)) = tasks.recv() {
                    if !ready {
                        let _ = reply.send(None);
                        continue;
                    }
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
                    let _ = reply.send(Some(outcome));
                }
            })
            .expect("el hilo de GTK debe arrancar");
        sender
    });
    let (reply, outcome) = mpsc::channel();
    sender
        .send((Box::new(task), reply))
        .expect("el hilo de GTK debe aceptar tareas");
    match outcome.recv().expect("el hilo de GTK debe responder") {
        Some(Ok(())) => true,
        Some(Err(payload)) => std::panic::resume_unwind(payload),
        None => false,
    }
}

fn skip(test: &str) {
    eprintln!("sin display: {test} se omite; corré `xvfb-run -a cargo test -p cauce-gui`");
}

fn frames(count: usize) {
    for _ in 0..count {
        while gtk::glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
}

/// Builds the page inside a 960×640 window and hands it to `visit`.
fn with_page(visit: impl FnOnce(&gtk::Widget, &adw::ApplicationWindow) + Send + 'static) -> bool {
    on_gtk_thread(move || {
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
        visit(&content, &window);
        window.close();
    })
}

fn walk(widget: &gtk::Widget, visit: &mut impl FnMut(&gtk::Widget)) {
    visit(widget);
    let mut child = widget.first_child();
    while let Some(current) = child {
        walk(&current, visit);
        child = current.next_sibling();
    }
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
            tone_mapping: None,
        },
        audio: AudioPlan {
            action: StreamAction::Copy,
            encoder: None,
            bitrate: None,
            channels: None,
            stream_index: Some(1),
            normalize: false,
            filter: None,
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
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
    }
}

#[test]
fn the_empty_state_has_a_single_instruction() {
    let ran = with_page(|content, _window| {
        let mut titles = 0;
        let mut duplicated = 0;
        walk(content, &mut |widget| {
            let Some(label) = widget.downcast_ref::<gtk::Label>() else {
                return;
            };
            if label.text() == t("convert.drop_title") {
                titles += 1;
            }
            // La segunda instrucción repetida: la tarjeta ya lo dice.
            if label.text() == t("convert.phase_empty") {
                duplicated += 1;
            }
        });

        assert_eq!(
            titles, 1,
            "el estado vacío se apoya en la tarjeta de drop, una sola vez"
        );
        assert_eq!(
            duplicated, 0,
            "no hay una segunda instrucción debajo de la tarjeta"
        );
    });
    if !ran {
        skip("the_empty_state_has_a_single_instruction");
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

/// A directory of its own under the system temp dir, so no test shares state.
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::UNIX_EPOCH
        .elapsed()
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("cauce-drop-{}-{nanos}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("el directorio temporal se crea");
    dir
}

fn find_card(content: &gtk::Widget) -> Option<gtk::Button> {
    let mut card = None;
    walk(content, &mut |widget| {
        if widget.widget_name() == DROP_CARD {
            card = widget.downcast_ref::<gtk::Button>().cloned();
        }
    });
    card
}

#[test]
fn dropping_several_files_and_folders_adds_them_all() {
    let root = unique_dir("muchos");
    let (uno, dos) = (root.join("uno"), root.join("dos"));
    let files = [
        root.join("suelto.mp4"),
        root.join("otro.mp3"),
        uno.join("a.mkv"),
        uno.join("b.webm"),
        dos.join("c.wav"),
    ];
    for file in &files {
        std::fs::create_dir_all(file.parent().expect("cada archivo tiene carpeta")).unwrap();
        std::fs::write(file, b"x").expect("el archivo se escribe");
    }
    // Un symlink no es un archivo de la carpeta: se ignora al expandir.
    let _ = std::os::unix::fs::symlink(&files[0], uno.join("enlace.mp4"));

    // Un único drop: dos archivos sueltos y dos carpetas.
    let paths = expand_dropped_paths(vec![
        files[0].clone(),
        uno.clone(),
        dos.clone(),
        files[1].clone(),
    ]);
    let mut found = paths.clone();
    found.sort();
    let mut expected: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "el drop expande carpetas una vez y no sigue symlinks"
    );

    let mut state = ConvertState::new();
    state.add_inputs(paths.clone());
    assert_eq!(
        state.pending_paths().len(),
        5,
        "el mismo drop encola los cinco archivos de una vez"
    );
    state.add_inputs(paths);
    assert_eq!(
        state.pending_paths().len(),
        5,
        "un drop repetido no duplica entradas"
    );
    std::fs::remove_dir_all(&root).expect("el directorio temporal se limpia");
}

#[test]
fn the_drop_area_is_full_width_and_visible() {
    let ran = with_page(|content, _window| {
        let mut largest_icon = 0;
        let mut has_title = false;
        let mut has_subtitle = false;
        walk(content, &mut |widget| {
            if let Some(image) = widget.downcast_ref::<gtk::Image>() {
                if image
                    .icon_name()
                    .is_some_and(|name| name.ends_with("-symbolic"))
                {
                    largest_icon = largest_icon.max(image.pixel_size());
                }
            }
            if let Some(label) = widget.downcast_ref::<gtk::Label>() {
                has_title |= label.text() == t("convert.drop_title");
                has_subtitle |= label.text() == t("convert.drop_subtitle");
            }
        });

        let card = find_card(content).expect("la tarjeta de drop se llama `drop-card`");
        assert!(
            card.is_visible() && card.is_mapped(),
            "la tarjeta está visible"
        );
        assert!(
            card.is_focusable(),
            "la tarjeta se alcanza con el teclado: Enter y Space activan el botón"
        );
        assert!(
            has_title,
            "la tarjeta muestra «{}»",
            t("convert.drop_title")
        );
        assert!(
            has_subtitle,
            "la tarjeta muestra «{}»",
            t("convert.drop_subtitle")
        );
        assert!(
            largest_icon >= 32,
            "el icono simbólico es grande: {largest_icon}"
        );
        assert!(
            STYLE_SHEET.contains("dashed") && STYLE_SHEET.contains("drag-over"),
            "el borde punteado y el estado de arrastre están en la hoja de estilos"
        );

        let parent = card
            .parent()
            .expect("la tarjeta vive en el cuerpo de la página");
        assert!(
            parent.width() > 300,
            "el contenido de la página no es un botón pill: {} px",
            parent.width()
        );
        // `compute_bounds` mide la caja completa (borde y padding incluidos).
        let bounds = card
            .compute_bounds(&parent)
            .expect("la tarjeta se puede situar dentro del cuerpo");
        assert_eq!(bounds.x(), 0.0, "la tarjeta arranca en el borde izquierdo");
        assert_eq!(
            bounds.width().round() as i32,
            parent.width(),
            "la tarjeta ocupa todo el ancho del contenido"
        );
    });
    if !ran {
        skip("the_drop_area_is_full_width_and_visible");
    }
}

#[test]
fn drag_over_state_toggles() {
    let ran = with_page(|content, _window| {
        let card = find_card(content).expect("la tarjeta de drop se llama `drop-card`");

        // El arrastre llega por un `GtkDropTarget` instalado en la tarjeta.
        let controllers = card.observe_controllers();
        let is_target = |index: u32| {
            controllers
                .item(index)
                .is_some_and(|item| item.is::<gtk::DropTarget>())
        };
        let targets = (0..controllers.n_items())
            .filter(|index| is_target(*index))
            .count();
        assert_eq!(targets, 1, "la tarjeta acepta un drop real");

        assert!(
            !card.has_css_class("drag-over"),
            "sin arrastre encima no hay estado resaltado"
        );
        view::set_drag_over(&card, true);
        assert!(
            card.has_css_class("drag-over"),
            "el arrastre pinta `drag-over`"
        );
        view::set_drag_over(&card, false);
        assert!(
            !card.has_css_class("drag-over"),
            "al salir se quita la clase"
        );
    });
    if !ran {
        skip("the_drop_area_is_full_width_and_visible");
    }
}

#[test]
fn the_technical_detail_is_reachable_through_the_disclosure() {
    let ran = on_gtk_thread(|| {
        let ui = Ui::build();
        let page: gtk::Widget = ui.page.clone().upcast();
        let window = adw::ApplicationWindow::builder()
            .default_width(960)
            .default_height(640)
            .content(&page)
            .build();
        window.set_visible(true);
        frames(30);

        assert!(!ui.failure.area.is_visible(), "sin error no hay superficie");

        let error = JobError::new(ErrorCode::PathDenied, "EACCES: Permission denied", false);
        cauce_gui::pages::show_failure(&ui.failure, &error);

        let human = ui.failure.message.text().to_string();
        assert_eq!(
            human,
            human_error(ErrorCode::PathDenied, &error.message),
            "la oración visible es la humana, no la del daemon"
        );
        assert!(
            !human.contains("Permission denied") && !human.contains("PATH_DENIED"),
            "ni el código ni el texto técnico son el principal: {human:?}"
        );
        assert!(
            !ui.failure.expander.is_expanded(),
            "«Detalles técnicos» arranca cerrado"
        );
        assert!(!ui.failure.details.is_mapped(), "cerrado no ocupa lugar");
        assert_eq!(
            ui.failure.expander.label().as_deref(),
            Some(t("error.technical_details")),
            "el desplegable se llama «Detalles técnicos»"
        );

        let details = ui.failure.details.text().to_string();
        assert_eq!(
            details,
            technical_details(ErrorCode::PathDenied, &error.message),
            "el desplegable conserva el código y el mensaje del daemon"
        );

        ui.failure.expander.set_expanded(true);
        frames(20);
        assert!(ui.failure.details.is_mapped(), "abierto, el detalle se lee");
        window.close();
    });
    if !ran {
        skip("the_drop_area_is_full_width_and_visible");
    }
}

#[test]
fn the_page_shows_a_collapsed_disclosure_and_paints_no_stray_glyph() {
    let ran = with_page(|content, window| {
        let mut combos = 0;
        let mut expanders: Vec<gtk::Expander> = Vec::new();
        let mut headline: Option<gtk::Label> = None;
        let mut plan_box: Option<gtk::Box> = None;
        let mut cta: Option<gtk::Button> = None;
        let mut offenders = Vec::new();
        walk(content, &mut |widget| {
            if widget.downcast_ref::<adw::ComboRow>().is_some() {
                combos += 1;
            }
            if let Some(expander) = widget.downcast_ref::<gtk::Expander>() {
                if expander.widget_name() == "plan-details" {
                    expanders.push(expander.clone());
                }
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
            "la página necesita un único desplegable «Detalles» del plan: hay {}",
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
            .compute_point(window, &gtk::graphene::Point::new(0.0, 0.0))
            .expect("la CTA se puede situar dentro de la ventana");
        let bottom = origin.y() + cta.height() as f32;
        assert!(
            bottom <= window.height() as f32,
            "«Añadir a la cola» tiene que entrar completa a 960×640: termina en {bottom:.0} de {}",
            window.height()
        );
    });
    if !ran {
        skip("the_drop_area_is_full_width_and_visible");
    }
}

/// A 1080p H.264/AAC snapshot with `size` bytes, as the daemon sends it.
fn sized_snapshot(path: &str, size: u64) -> cauce_core::MediaSnapshot {
    serde_json::from_value(serde_json::json!({
        "path": path, "container": "mp4", "duration_ms": 12500, "size_bytes": size,
        "bitrate_bps": 900000,
        "video_streams": [{ "index": 0, "codec": "h264", "profile": "High", "level": null,
            "width": 1920, "height": 1080, "fps": 30.0, "pixel_format": "yuv420p",
            "bit_depth": 8, "is_hdr": false, "bitrate_bps": null, "language": null }],
        "audio_streams": [{ "index": 1, "codec": "aac", "channels": 2, "sample_rate": 48000,
            "bitrate_bps": null, "language": "spa", "is_default": true }],
        "subtitle_streams": [], "probed_at": "2026-11-14T10:00:00Z",
    }))
    .expect("el snapshot de prueba es válido")
}

#[test]
fn the_selection_header_summary_and_row_removal_are_visible() {
    use cauce_gui::pages::convert::state::ConvertState;
    use std::cell::RefCell;
    use std::rc::Rc;

    let ran = on_gtk_thread(|| {
        let ui = Ui::build();
        let state = Rc::new(RefCell::new(ConvertState::new()));
        let presets = Rc::new(RefCell::new(Vec::new()));
        let removed = Rc::new(RefCell::new(Vec::<String>::new()));
        let handler: Rc<dyn Fn(String)> = {
            let removed = Rc::clone(&removed);
            Rc::new(move |path: String| removed.borrow_mut().push(path))
        };
        let refresh = view::refresh(&ui, &state, &presets, &handler);
        refresh();
        assert!(
            !ui.selection_box.is_visible(),
            "sin archivos no hay cabecera de selección"
        );

        {
            let mut state = state.borrow_mut();
            state.set_output_dir(Some("/home/wilkin/Vídeos".to_owned()));
            state.set_preset(Some("mp4-h264".to_owned()));
            state.add_inputs(vec!["/tmp/uno.mp4".to_owned()]);
            state.accept_probe(sized_snapshot("/tmp/uno.mp4", 1_048_576));
        }
        refresh();

        assert!(
            ui.selection_box.is_visible(),
            "con archivos la cabecera aparece"
        );
        assert_eq!(ui.selection_heading.text(), "Seleccionado (1)");
        assert_eq!(
            ui.selection_size.text(),
            "1.0 MiB",
            "la cabecera suma tamaños"
        );
        let summary = ui.summary_label.text().to_string();
        assert!(
            summary.starts_with("Se añadirá 1 archivo a la cola"),
            "la línea anterior a la CTA anuncia el alta: {summary}"
        );
        assert!(
            summary.contains("carpeta: /home/wilkin/Vídeos"),
            "{summary}"
        );

        // One removal button per accepted row, with its own name in the tooltip.
        let mut remove_buttons = Vec::new();
        walk(&ui.files_list.clone().upcast(), &mut |widget| {
            if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                if button.label().as_deref() == Some(t("convert.remove_file")) {
                    remove_buttons.push(button.clone());
                }
            }
        });
        assert_eq!(remove_buttons.len(), 1, "una fila, un botón de quitar");
        assert_eq!(
            remove_buttons[0].tooltip_text().as_deref(),
            Some("Quitar «uno.mp4»"),
            "el botón nombra el archivo que quita"
        );
        remove_buttons[0].emit_clicked();
        assert_eq!(removed.borrow().as_slice(), ["/tmp/uno.mp4".to_owned()]);
    });
    if !ran {
        skip("the_selection_header_summary_and_row_removal_are_visible");
    }
}

/// What the row actually paints: Pango reads title and subtitle as markup, so
/// this is the text the user sees, and it fails when the markup is broken.
fn rendered(markup: &str) -> String {
    match gtk::pango::parse_markup(markup, '\0') {
        Ok((_, text, _)) => text.to_string(),
        Err(error) => panic!("el markup no parsea ({error}): {markup:?}"),
    }
}

#[test]
fn the_file_row_texts_escape_the_name_and_keep_it_whole() {
    for name in MARKUP_NAMES {
        let summary = "MP4 · 00:12 · 1920×1080 · h264 + aac";
        let (title, subtitle) = view::file_row_texts(name, summary);

        assert_eq!(
            title,
            gtk::glib::markup_escape_text(name),
            "el título de la fila va escapado, nunca el nombre en crudo: {title:?}"
        );
        assert_eq!(
            subtitle,
            gtk::glib::markup_escape_text(summary),
            "el subtítulo de la fila va escapado: {subtitle:?}"
        );
        assert!(!title.is_empty(), "el título no puede quedar vacío");
        assert!(!subtitle.is_empty(), "el subtítulo no puede quedar vacío");
        assert_eq!(
            rendered(&title),
            name,
            "al desescapar vuelve el nombre exacto, con sus acentos y símbolos"
        );
        assert_eq!(rendered(&subtitle), summary, "y el resumen, intacto");
    }

    // El nombre real conserva su `&` como entidad, que es lo que rompía la fila.
    let (title, _) = view::file_row_texts(MARKUP_NAMES[0], "");
    assert!(
        title.contains("&amp;"),
        "el `&` sobrevive escapado: {title:?}"
    );
    // El nombre hostil escapa los cinco caracteres que Pango interpreta.
    let (hostile, _) = view::file_row_texts(MARKUP_NAMES[1], "");
    for entity in ["&lt;", "&gt;", "&quot;", "&apos;", "&amp;"] {
        assert!(hostile.contains(entity), "falta {entity} en {hostile:?}");
    }
}

#[test]
fn the_rows_of_the_page_escape_what_the_user_brings() {
    use cauce_gui::pages::convert::state::RejectedInput;
    use std::cell::RefCell;
    use std::rc::Rc;

    let ran = on_gtk_thread(|| {
        let remove: Rc<dyn Fn(String)> = Rc::new(|_| {});
        // La fila aceptada: el nombre real y el hostil.
        for name in MARKUP_NAMES {
            let path = format!("/tmp/entrada/{name}");
            let snapshot = sized_snapshot(&path, 1024);
            let row = view::file_row(&snapshot, &remove);
            let title = row.title();
            let subtitle = row.subtitle().unwrap_or_default();
            assert_eq!(
                rendered(title.as_ref()),
                base_name(&path),
                "título de la fila: {title:?}"
            );
            assert_eq!(
                rendered(subtitle.as_ref()),
                summarize(&snapshot),
                "subtítulo: {subtitle:?}"
            );
            assert!(
                !rendered(title.as_ref()).is_empty(),
                "la fila no queda vacía"
            );
        }

        // La fila rechazada: el mismo nombre y el mensaje del daemon.
        let rejected = RejectedInput {
            path: format!("/tmp/entrada/{}", MARKUP_NAMES[1]),
            message: "no se pudo leer <stream 0> & «codec»".to_owned(),
        };
        let row = view::rejected_row(&rejected);
        assert_eq!(
            rendered(row.title().as_ref()),
            base_name(&rejected.path),
            "el título de la fila rechazada va escapado"
        );
        assert_eq!(
            rendered(row.subtitle().unwrap_or_default().as_ref()),
            rejected.message,
            "el mensaje del daemon va escapado"
        );

        // La carpeta de salida, que también elige el usuario.
        let folder = "/home/wilkin/Vídeos & Música <final>";
        let ui = Ui::build();
        let mut state = ConvertState::new();
        state.set_output_dir(Some(folder.to_owned()));
        let state = Rc::new(RefCell::new(state));
        let presets = Rc::new(RefCell::new(Vec::new()));
        let refresh = view::refresh(&ui, &state, &presets, &remove);
        refresh();
        assert_eq!(
            rendered(ui.output_row.subtitle().unwrap_or_default().as_ref()),
            folder,
            "la carpeta de salida va escapada en su fila"
        );
    });
    if !ran {
        skip("the_rows_of_the_page_escape_what_the_user_brings");
    }
}
