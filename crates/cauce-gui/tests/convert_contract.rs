//! Contract of the Convertir page core (`docs/ui-mvp.md` §4, `docs/contracts.md` §6).
//!
//! The decisions of the page live in a plain struct with no GTK types, so the
//! whole state machine, the folder expansion and the queue request are tested
//! without a display. The widgets are only a rendering of this state.

use std::fs;
use std::path::PathBuf;

use cauce_core::{AudioStream, Capabilities, MediaSnapshot, Timestamp, VideoStream};
use cauce_gui::pages::convert::{self, ConvertPhase, ConvertState};

fn snapshot(path: &str) -> MediaSnapshot {
    MediaSnapshot {
        path: path.to_owned(),
        container: "mp4".to_owned(),
        duration_ms: 12_500,
        size_bytes: 1024,
        bitrate_bps: 900_000,
        video_streams: vec![VideoStream {
            index: 0,
            codec: "h264".to_owned(),
            profile: Some("high".to_owned()),
            level: None,
            width: 1920,
            height: 1080,
            fps: 30.0,
            pixel_format: "yuv420p".to_owned(),
            bit_depth: 8,
            is_hdr: false,
            bitrate_bps: None,
            language: None,
        }],
        audio_streams: vec![AudioStream {
            index: 1,
            codec: "aac".to_owned(),
            channels: 2,
            sample_rate: 48_000,
            bitrate_bps: None,
            language: Some("spa".to_owned()),
            is_default: true,
        }],
        subtitle_streams: Vec::new(),
        probed_at: Timestamp::parse("2026-11-14T10:00:00Z").expect("timestamp válido"),
    }
}

/// A page with one accepted input, an output folder and a preset: queueable.
fn ready_state() -> ConvertState {
    let mut state = ConvertState::new();
    state.set_output_dir(Some("/home/wilkin/Vídeos".to_owned()));
    state.set_preset(Some("mp4-h264".to_owned()));
    state.add_inputs(vec!["/tmp/entrada.mp4".to_owned()]);
    state.accept_probe(snapshot("/tmp/entrada.mp4"));
    state
}

#[test]
fn empty_state_cannot_queue() {
    let mut state = ConvertState::new();

    assert_eq!(
        state.phase(),
        ConvertPhase::Empty,
        "sin entradas el estado es Inicial"
    );
    assert!(!state.can_queue(), "sin entradas no hay nada que encolar");
    assert!(
        state.take_queue_request().is_none(),
        "un estado vacío nunca produce una petición de cola"
    );
}

#[test]
fn ready_requires_inputs_output_dir_and_preset() {
    let state = ready_state();
    assert_eq!(state.phase(), ConvertPhase::Ready);
    assert!(
        state.can_queue(),
        "con entradas, carpeta y preset está listo"
    );

    let mut without_inputs = ConvertState::new();
    without_inputs.set_output_dir(Some("/tmp/salida".to_owned()));
    without_inputs.set_preset(Some("mp4-h264".to_owned()));
    assert!(!without_inputs.can_queue(), "sin entradas falta una pieza");

    let mut without_dir = ready_state();
    without_dir.set_output_dir(None);
    assert!(
        !without_dir.can_queue(),
        "sin carpeta de salida falta una pieza"
    );

    let mut without_preset = ready_state();
    without_preset.set_preset(None);
    assert!(!without_preset.can_queue(), "sin preset falta una pieza");

    let mut inspecting = ConvertState::new();
    inspecting.set_output_dir(Some("/tmp/salida".to_owned()));
    inspecting.set_preset(Some("mp4-h264".to_owned()));
    inspecting.add_inputs(vec!["/tmp/otro.mp4".to_owned()]);
    assert_eq!(inspecting.phase(), ConvertPhase::Inspecting);
    assert!(
        !inspecting.can_queue(),
        "mientras se inspecciona el botón sigue deshabilitado"
    );
}

#[test]
fn rejected_probes_keep_the_phase_partial() {
    let mut state = ConvertState::new();
    state.set_output_dir(Some("/tmp/salida".to_owned()));
    state.set_preset(Some("mp4-h264".to_owned()));
    state.add_inputs(vec![
        "/tmp/bueno.mp4".to_owned(),
        "/tmp/roto.mkv".to_owned(),
    ]);
    state.accept_probe(snapshot("/tmp/bueno.mp4"));
    state.reject(
        "/tmp/roto.mkv".to_owned(),
        "FFprobe no pudo leer el archivo".to_owned(),
    );

    assert_eq!(
        state.phase(),
        ConvertPhase::Partial,
        "un rechazo no borra los válidos"
    );
    assert_eq!(state.accepted().len(), 1);
    assert_eq!(state.rejected().len(), 1);
    assert_eq!(state.rejected()[0].path, "/tmp/roto.mkv");
    assert!(
        state.rejected()[0].message.contains("FFprobe"),
        "el motivo del rechazo se conserva: {}",
        state.rejected()[0].message
    );
    assert!(state.can_queue(), "los válidos se pueden encolar igual");
}

#[test]
fn queue_request_contains_every_accepted_input() {
    let mut state = ready_state();
    state.add_inputs(vec![
        "/tmp/segundo.mkv".to_owned(),
        "/tmp/roto.wav".to_owned(),
    ]);
    state.accept_probe(snapshot("/tmp/segundo.mkv"));
    state.reject("/tmp/roto.wav".to_owned(), "duración ilegible".to_owned());

    let request = state.take_queue_request().expect("listo para encolar");

    assert_eq!(
        request.inputs,
        vec!["/tmp/entrada.mp4".to_owned(), "/tmp/segundo.mkv".to_owned()],
        "la petición lleva todas las entradas aceptadas, en orden"
    );
    assert!(
        !request.inputs.contains(&"/tmp/roto.wav".to_owned()),
        "los rechazados nunca entran en la cola"
    );
    assert_eq!(request.output_dir, "/home/wilkin/Vídeos");
    assert_eq!(request.preset_id, "mp4-h264");
}

#[test]
fn queue_request_is_taken_once() {
    let mut state = ready_state();

    assert!(
        state.take_queue_request().is_some(),
        "la primera vez se entrega"
    );
    assert!(
        state.take_queue_request().is_none(),
        "sin cambios de estado una segunda llamada no repite el alta"
    );

    state.set_preset(Some("mkv-h265".to_owned()));
    let request = state
        .take_queue_request()
        .expect("un cambio de estado vuelve a habilitar la petición");
    assert_eq!(request.preset_id, "mkv-h265");
}

#[test]
fn missing_ffmpeg_short_circuits_the_page() {
    let capabilities = Capabilities {
        ffmpeg_missing: vec!["libx265".to_owned()],
        ..Capabilities::default()
    };
    let mut state = ready_state();
    state.apply_capabilities(&capabilities);

    assert_eq!(
        state.phase(),
        ConvertPhase::Error,
        "sin FFmpeg la página queda bloqueada"
    );
    assert!(!state.can_queue(), "no se puede encolar sin FFmpeg");
    let message = state.blocking().expect("hay un bloqueo con mensaje");
    assert!(
        message.contains("sudo apt install ffmpeg"),
        "la acción es concreta: {message}"
    );

    let healthy = Capabilities {
        encoders: vec!["libx264".to_owned()],
        ..Capabilities::default()
    };
    let mut state = ready_state();
    state.apply_capabilities(&healthy);
    assert_eq!(state.phase(), ConvertPhase::Ready);
    assert!(
        state.blocking().is_none(),
        "con FFmpeg completo no hay bloqueo"
    );
    assert!(state.can_queue());
}

#[test]
fn adding_a_folder_expands_its_regular_files() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("gui-convert")
        .join(format!("carpeta-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).expect("carpeta temporal");

    for name in ["uno.mp4", "dos.mkv", "tres.wav"] {
        fs::write(dir.join(name), b"x").expect("archivo temporal");
    }
    fs::write(dir.join("sub").join("anidado.mp4"), b"x").expect("archivo anidado");
    std::os::unix::fs::symlink(dir.join("uno.mp4"), dir.join("enlace.mp4")).expect("enlace");

    let expanded = convert::expand_dropped_folder(&dir);

    assert_eq!(
        expanded.len(),
        3,
        "un nivel, solo archivos regulares y sin seguir enlaces: {expanded:?}"
    );
    let mut names: Vec<String> = expanded
        .iter()
        .map(|path| convert::base_name(path))
        .collect();
    names.sort();
    assert_eq!(names, vec!["dos.mkv", "tres.wav", "uno.mp4"]);
    assert!(
        !expanded.iter().any(|path| path.ends_with("enlace.mp4")),
        "un enlace simbólico no se sigue"
    );

    let _ = fs::remove_dir_all(&dir);
}

// The explicit selection of `docs/ui-mvp.md` §4: a header that counts and sizes
// what will be queued, one removal per row, a clear-all, and a summary line that
// names the piece the CTA is missing.

#[test]
fn header_counts_and_sizes_the_selection() {
    let mut state = ConvertState::new();
    state.add_inputs(vec!["/tmp/uno.mp4".to_owned(), "/tmp/dos.mkv".to_owned()]);
    let mut first = snapshot("/tmp/uno.mp4");
    first.size_bytes = 1_500_000;
    let mut second = snapshot("/tmp/dos.mkv");
    second.size_bytes = 2_500_000;
    state.accept_probe(first);
    state.accept_probe(second);

    assert_eq!(
        state.accepted_count(),
        2,
        "la cabecera cuenta los aceptados"
    );
    assert_eq!(
        state.accepted_total_bytes(),
        4_000_000,
        "y suma el tamaño de todos"
    );
    assert_eq!(
        state.selection_heading(),
        "Seleccionados (2)",
        "la cabecera se lee tal cual"
    );

    let mut single = ConvertState::new();
    single.add_inputs(vec!["/tmp/uno.mp4".to_owned()]);
    single.accept_probe(snapshot("/tmp/uno.mp4"));
    assert_eq!(single.accepted_count(), 1);
    assert_eq!(single.accepted_total_bytes(), 1024);
    assert_eq!(
        single.selection_heading(),
        "Seleccionado (1)",
        "un solo archivo se anuncia en singular"
    );
}

#[test]
fn removing_one_updates_count_and_size() {
    let mut state = ConvertState::new();
    state.add_inputs(vec!["/tmp/uno.mp4".to_owned(), "/tmp/dos.mkv".to_owned()]);
    let mut first = snapshot("/tmp/uno.mp4");
    first.size_bytes = 1_500_000;
    let mut second = snapshot("/tmp/dos.mkv");
    second.size_bytes = 2_500_000;
    state.accept_probe(first);
    state.accept_probe(second);

    let removed = state.removal("/tmp/uno.mp4").expect("la fila existe");

    assert_eq!(removed.path, "/tmp/uno.mp4");
    assert_eq!(state.accepted_count(), 1, "queda uno");
    assert_eq!(state.accepted_total_bytes(), 2_500_000, "y su tamaño");
    assert_eq!(state.accepted()[0].path, "/tmp/dos.mkv");
    assert!(
        state.removal("/tmp/uno.mp4").is_none(),
        "quitar dos veces no inventa una fila"
    );
    assert_eq!(state.accepted_count(), 1, "y no borra otra cosa");
}

#[test]
fn clear_all_empties_the_selection() {
    let mut state = ready_state();
    state.reject("/tmp/roto.mkv".to_owned(), "ilegible".to_owned());
    assert!(state.can_queue());

    state.clear_all();

    assert_eq!(state.accepted_count(), 0, "no queda ningún aceptado");
    assert_eq!(state.accepted_total_bytes(), 0);
    assert!(state.rejected().is_empty(), "ni ningún rechazado");
    assert!(state.pending_paths().is_empty(), "ni pendientes");
    assert!(!state.can_queue(), "sin archivos no hay nada que encolar");
    assert_eq!(state.phase(), ConvertPhase::Empty);
    assert_eq!(
        state.output_dir(),
        Some("/home/wilkin/Vídeos"),
        "la carpeta elegida se conserva"
    );
    assert_eq!(state.preset_id(), Some("mp4-h264"), "y el preset también");
}

#[test]
fn summary_names_the_missing_preset_or_folder() {
    let state = ready_state();
    let complete = state.summary_line().sentence();
    assert!(
        !complete.to_lowercase().contains("falta"),
        "con las dos piezas no se anuncia ninguna falta: {complete}"
    );

    let mut without_preset = ready_state();
    without_preset.set_preset(None);
    let sentence = without_preset.summary_line().sentence();
    assert!(
        sentence.contains("falta el preset"),
        "la frase nombra el preset que falta: {sentence}"
    );
    assert!(
        !sentence.contains("falta la carpeta"),
        "y no nombra lo que sí está: {sentence}"
    );

    let mut without_folder = ready_state();
    without_folder.set_output_dir(None);
    let sentence = without_folder.summary_line().sentence();
    assert!(
        sentence.contains("falta la carpeta de salida"),
        "la frase nombra la carpeta que falta: {sentence}"
    );
    assert!(!sentence.contains("falta el preset"), "{sentence}");

    let mut without_both = ready_state();
    without_both.set_preset(None);
    without_both.set_output_dir(None);
    let sentence = without_both.summary_line().sentence();
    assert!(
        sentence.contains("falta el preset") && sentence.contains("falta la carpeta de salida"),
        "sin las dos piezas se nombran las dos: {sentence}"
    );
}

#[test]
fn summary_uses_singular_and_plural() {
    let mut state = ready_state();
    let one = state.summary_line().sentence();
    assert!(
        one.starts_with("Se añadirá 1 archivo a la cola"),
        "un archivo se anuncia en singular: {one}"
    );

    state.add_inputs(vec!["/tmp/segundo.mkv".to_owned()]);
    state.accept_probe(snapshot("/tmp/segundo.mkv"));
    let many = state.summary_line().sentence();
    assert!(
        many.starts_with("Se añadirán 2 archivos a la cola"),
        "dos archivos se anuncian en plural: {many}"
    );
    assert!(
        many.contains("preset: mp4-h264"),
        "la frase nombra el preset elegido: {many}"
    );
    assert!(
        many.contains("carpeta: /home/wilkin/Vídeos"),
        "y la carpeta de salida: {many}"
    );

    // The confirmation after queueing counts in the same two forms.
    state.confirm_queue(1);
    assert_eq!(state.confirmation(), Some(1));
    assert!(
        state
            .confirmation_sentence()
            .starts_with("Se añadió 1 trabajo")
    );
    state.confirm_queue(3);
    assert_eq!(
        state.confirmation(),
        Some(3),
        "la confirmación lleva la cuenta"
    );
    assert!(
        state
            .confirmation_sentence()
            .starts_with("Se añadieron 3 trabajos")
    );
}

#[test]
fn cta_disabled_without_preset_or_folder() {
    assert!(
        ready_state().can_queue(),
        "con todo listo la CTA se habilita"
    );

    let mut without_preset = ready_state();
    without_preset.set_preset(None);
    assert!(
        !without_preset.can_queue(),
        "sin preset la CTA queda deshabilitada"
    );
    assert!(
        without_preset
            .summary_line()
            .sentence()
            .contains("falta el preset"),
        "y la frase dice cuál falta"
    );

    let mut without_folder = ready_state();
    without_folder.set_output_dir(None);
    assert!(
        !without_folder.can_queue(),
        "sin carpeta la CTA queda deshabilitada"
    );
    assert!(
        without_folder
            .summary_line()
            .sentence()
            .contains("falta la carpeta de salida"),
        "y la frase dice cuál falta"
    );
}
