//! Contract of the window chrome and of the client's connection state
//! (`docs/ui-mvp.md` §2 and §9, `docs/contracts.md` §5 and §7).
//!
//! Two defects seen in the real screenshot are pinned here: the window must draw
//! exactly one set of window controls, and the footer must follow a successful
//! request instead of waiting for a `daemon.status` event. The chrome test needs a
//! display, so it says out loud when it cannot run instead of passing by silence;
//! the two transport tests use a fake socket and need no display at all.

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use adw::prelude::*;
use cauce_gui::{client, i18n, pages, window};
use libadwaita as adw;
use serde_json::{Value, json};

/// `XDG_RUNTIME_DIR` is process-wide, so the three tests take turns.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unique_dir(tag: &str) -> PathBuf {
    static NEXT: Mutex<u64> = Mutex::new(0);
    let mut counter = NEXT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *counter += 1;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("gui-window")
        .join(format!("{tag}-{}-{}", std::process::id(), *counter));
    fs::create_dir_all(dir.join("cauce")).expect("carpeta temporal");
    dir
}

/// A temporary `XDG_RUNTIME_DIR` that is restored when the test ends.
struct RuntimeDir {
    path: PathBuf,
    previous: Option<OsString>,
}

impl RuntimeDir {
    fn new(tag: &str) -> Self {
        let path = unique_dir(tag);
        let previous = std::env::var_os("XDG_RUNTIME_DIR");
        // SAFETY: the guard held by every test makes this the only mutation in
        // flight, and the client resolves the path per connection attempt.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &path) };
        Self { path, previous }
    }

    fn socket(&self) -> PathBuf {
        self.path.join("cauce/daemon.sock")
    }
}

impl Drop for RuntimeDir {
    fn drop(&mut self) {
        match self.previous.take() {
            // SAFETY: see `RuntimeDir::new`.
            Some(previous) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", previous) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime de prueba")
}

fn initialize_params() -> Value {
    json!({"client_name": "cauce-window-tests", "client_version": "0.1.0"})
}

/// Answers the first request. Without a `close` receiver it stays open until the
/// client goes away; with one it waits for the test's signal and then hangs up,
/// like a daemon that crashes.
fn fake_daemon(
    socket: &Path,
    close: Option<std::sync::mpsc::Receiver<()>>,
) -> std::thread::JoinHandle<()> {
    let listener = UnixListener::bind(socket).expect("socket del daemon falso");
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("aceptar la conexión");
        let mut reader = BufReader::new(stream.try_clone().expect("clon del socket"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("leer la petición");
        let request: Value = serde_json::from_str(&line).expect("petición JSON");
        let reply = json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "result": {"protocol_version": 1},
        });
        let mut writer = stream;
        writer
            .write_all(format!("{reply}\n").as_bytes())
            .expect("respuesta");
        writer.flush().expect("descarga");
        match close {
            // A live daemon keeps the socket until the client drops it.
            None => {
                let mut rest = Vec::new();
                let _ = reader.read_to_end(&mut rest);
            }
            // The test decides when this daemon goes away.
            Some(close) => {
                let _ = close.recv_timeout(Duration::from_secs(10));
            }
        }
    })
}

/// Every header bar below `widget`, in tree order.
fn header_bars(widget: &adw::gtk::Widget, found: &mut Vec<adw::HeaderBar>) {
    if let Some(header) = widget.downcast_ref::<adw::HeaderBar>() {
        found.push(header.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        header_bars(&current, found);
        child = current.next_sibling();
    }
}

/// The `AdwWindowTitle` a header bar shows, when it uses one.
fn shown_title(header: &adw::HeaderBar) -> Option<adw::WindowTitle> {
    header
        .title_widget()
        .and_then(|widget| widget.downcast::<adw::WindowTitle>().ok())
}

#[test]
fn chrome_has_a_single_set_of_window_controls() {
    let _guard = lock_env();
    if adw::gtk::init().is_err() || adw::gtk::gdk::Display::default().is_none() {
        eprintln!(
            "sin display: el contrato de chrome se salta; corré `xvfb-run -a cargo test -p cauce-gui`"
        );
        return;
    }
    let dir = RuntimeDir::new("chrome");
    assert!(!dir.socket().exists(), "el chrome se prueba sin daemon");

    let application = adw::Application::builder()
        .application_id("io.github.wilkin.CauceChrometest")
        .build();
    // A window may only join a registered application; with no session bus the
    // registration stays local, which is all this chrome test needs.
    let _ = application.register(adw::gio::Cancellable::NONE);
    let window = window::build(&application);
    let root: adw::gtk::Widget = window.clone().upcast();
    let mut headers = Vec::new();
    header_bars(&root, &mut headers);

    // The sidebar page and the visible section page: one header bar each, and
    // no window-level header bar above the split view.
    assert_eq!(
        headers.len(),
        2,
        "la ventana debe tener una barra para el sidebar y una para la sección visible: {}",
        headers.len()
    );

    let mut with_controls: Vec<&adw::HeaderBar> = Vec::new();
    let mut without_controls: Vec<&adw::HeaderBar> = Vec::new();
    for header in &headers {
        if header.shows_end_title_buttons() {
            with_controls.push(header);
        } else {
            without_controls.push(header);
        }
    }
    assert_eq!(
        with_controls.len(),
        1,
        "el conjunto de controles de ventana debe ser único: hay {} barras que los dibujan",
        with_controls.len()
    );
    assert_eq!(
        without_controls.len(),
        1,
        "el sidebar no dibuja controles de ventana"
    );

    let sidebar = without_controls[0];
    assert!(
        !sidebar.shows_start_title_buttons(),
        "el sidebar tampoco dibuja los controles de inicio"
    );
    let sidebar_title = shown_title(sidebar).expect("el sidebar conserva el título de la app");
    assert_eq!(sidebar_title.title(), i18n::t("app.title"));

    // The visible section keeps its own header bar with the section name.
    let content = with_controls[0];
    let content_title = shown_title(content).expect("la sección muestra su propio título");
    assert_eq!(
        content_title.title(),
        i18n::t("section.convert_title"),
        "la sección visible sigue nombrando su área"
    );

    // No page adds a second header bar of its own.
    let client = Rc::new(client::Client::connect());
    for page in pages::all(&client) {
        let widget: adw::gtk::Widget = page.clone().upcast();
        let mut page_headers = Vec::new();
        header_bars(&widget, &mut page_headers);
        assert_eq!(
            page_headers.len(),
            1,
            "la página `{}` debe tener una sola barra de cabecera",
            page.property::<String>("tag")
        );
        assert!(
            shown_title(&page_headers[0]).is_some(),
            "cada página nombra su sección en su barra"
        );
    }
}

#[test]
fn a_successful_reply_marks_the_client_connected() {
    let _guard = lock_env();
    let dir = RuntimeDir::new("connected");
    let socket = dir.socket();
    let server = fake_daemon(&socket, None);

    let runtime = runtime();
    let client = client::Client::connect();
    let states = client.connection();
    assert!(
        !client.is_connected(),
        "un cliente recién creado todavía no habló con el daemon"
    );

    let result = runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("initialize responde");

    assert_eq!(result["protocol_version"], json!(1));
    assert!(
        client.is_connected(),
        "una respuesta correcta prueba que el daemon está conectado"
    );
    assert!(
        next_state(&runtime, &states),
        "el shell recibe el cambio a conectado sin preguntar"
    );
    drop(client);
    server.join().expect("el servidor termina");
}

#[test]
fn a_disconnect_marks_the_client_disconnected() {
    let _guard = lock_env();
    let dir = RuntimeDir::new("disconnected");
    let socket = dir.socket();
    let (close, close_rx) = std::sync::mpsc::channel();
    let server = fake_daemon(&socket, Some(close_rx));

    let runtime = runtime();
    let client = client::Client::connect();
    let events = client.events();
    let states = client.connection();
    runtime
        .block_on(client.call("initialize", initialize_params()))
        .expect("initialize responde");
    assert!(
        client.is_connected(),
        "la respuesta deja la conexión en línea"
    );
    assert!(next_state(&runtime, &states));

    close.send(()).expect("cerrar el daemon falso");
    server.join().expect("el servidor termina");

    let disconnect = runtime.block_on(async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            match tokio::time::timeout_at(deadline, events.recv()).await {
                Ok(Ok(event)) if event["method"] == json!(client::DISCONNECTED_EVENT) => {
                    break event;
                }
                Ok(Ok(_)) => continue,
                Ok(Err(_)) => panic!("el canal de eventos se cerró antes de la desconexión"),
                Err(_) => panic!("la desconexión no llegó en cinco segundos"),
            }
        }
    });

    assert_eq!(disconnect["params"], json!({}));
    assert!(
        !client.is_connected(),
        "un servidor que se va deja el cliente desconectado"
    );
    assert!(
        !next_state(&runtime, &states),
        "el shell recibe el cambio a desconectado sin preguntar"
    );
}

/// Next connection state the shell would paint, waiting a bounded time.
fn next_state(runtime: &tokio::runtime::Runtime, states: &async_channel::Receiver<bool>) -> bool {
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), states.recv())
            .await
            .expect("el cambio de estado llega a tiempo")
            .expect("el canal de estado sigue abierto")
    })
}
