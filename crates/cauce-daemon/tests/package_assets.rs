//! Packaging assets of the single `cauce` `.deb` (`docs/architecture.md` §16,
//! `odd/tasks/cauce-0.1.md` T20/C-050).
//!
//! The package layout is plain text in `crates/cauce-daemon/Cargo.toml`
//! (`[package.metadata.deb]`) plus four own files in `packaging/`. Every
//! assertion here is a string scan of those files, so the test proves the
//! declared layout without needing `cargo-deb` or a release build:
//!
//! - the three binaries are declared as `target/release/<bin>` assets, checked
//!   as paths only (they are produced by `cargo build --release --workspace`
//!   before `cargo deb --no-build`, so they need not exist for this test);
//! - every other asset source is a real file in `packaging/` and must exist on
//!   disk, because those bytes are what the package ships;
//! - no FFmpeg binary is shipped (`S2`): FFmpeg comes from the system `Depends`.
//!
//! `desktop_and_appstream_validate` runs the distribution validators when they
//! exist and fails on their exit status; when a validator is missing the test
//! prints a loud skip instead of passing silently.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The three executables the package must install in `/usr/bin`.
const BINARIES: [&str; 3] = ["cauce-daemon", "cauce-gui", "cauce-cli"];

/// Runtime dependencies exactly as `docs/architecture.md` §16 lists them.
const DEPENDS: [&str; 5] = [
    "ffmpeg",
    "libgtk-4-1",
    "libadwaita-1-0",
    "libnotify4",
    "xdg-desktop-portal",
];

/// Non-binary assets: (source relative to the workspace, target directory, mode).
const PACKAGING_ASSETS: [(&str, &str, &str); 5] = [
    ("packaging/cauce.socket", "usr/lib/systemd/user/", "644"),
    ("packaging/cauce.service", "usr/lib/systemd/user/", "644"),
    ("packaging/cauce.desktop", "usr/share/applications/", "644"),
    (
        "packaging/io.github.wilkin.Cauce.metainfo.xml",
        "usr/share/metainfo/",
        "644",
    ),
    (
        "packaging/icons/hicolor/scalable/apps/io.github.wilkin.Cauce.svg",
        "usr/share/icons/hicolor/scalable/apps/",
        "644",
    ),
];

/// `/home/wilkin/proyectos/Cauce` when the tests run from the crate directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("cauce-daemon debe vivir en <raíz>/crates/cauce-daemon")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", path.display()))
}

fn deb_manifest() -> String {
    read(&workspace_root().join("crates/cauce-daemon/Cargo.toml"))
}

fn read_packaging(rel: &str) -> String {
    read(&workspace_root().join(rel))
}

/// Text of the `[package.metadata.deb]` table, header excluded.
fn deb_table(manifest: &str) -> String {
    let mut lines = manifest
        .lines()
        .skip_while(|l| l.trim() != "[package.metadata.deb]");
    assert!(
        lines.next().is_some(),
        "Cargo.toml debe declarar [package.metadata.deb]"
    );
    lines
        .take_while(|line| {
            let line = line.trim();
            // A TOML table header, not an `assets` entry such as
            // `["target/release/...", "usr/bin/", "755"],`.
            !(line.starts_with('[') && line.ends_with(']') && !line.contains('"'))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Value of `key = "value"` inside the deb table.
fn deb_string(table: &str, key: &str) -> String {
    for line in table.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key)
            && let Some(rest) = rest.trim_start().strip_prefix('=')
        {
            let value = rest.trim();
            return value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or_else(|| panic!("{key} debe ser una cadena entrecomillada en una línea"))
                .to_string();
        }
    }
    panic!("[package.metadata.deb] debe declarar {key}");
}

/// Body of `assets = [ ... ]`, one entry per declared asset, flattened so that
/// an entry may be written on one line or across several.
fn deb_assets(table: &str) -> Vec<String> {
    let mut lines = table
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("assets"));
    let header = lines
        .next()
        .expect("[package.metadata.deb] debe declarar assets");
    assert!(
        header.contains('['),
        "assets debe abrirse con una lista: {header}"
    );
    let mut body = String::new();
    for line in lines {
        let line = line.trim();
        if line == "]" {
            return body
                .split(']')
                .map(|entry| {
                    entry
                        .trim()
                        .trim_start_matches('[')
                        .trim_end_matches(',')
                        .trim()
                })
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect();
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        body.push_str(line);
        body.push(' ');
    }
    panic!("assets debe cerrarse con ]");
}

/// `(source, target, mode)` of an `assets` line.
fn asset_fields(line: &str) -> (String, String, String) {
    let quoted: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
    assert_eq!(
        quoted.len(),
        3,
        "cada activo debe ser [\"origen\", \"destino\", \"modo\"]: {line}"
    );
    (
        quoted[0].to_string(),
        quoted[1].to_string(),
        quoted[2].to_string(),
    )
}

/// Source path of an asset as it exists on disk: `target/release/<bin>` is
/// resolved against the workspace target dir, everything else against the crate.
fn asset_source_on_disk(source: &str) -> PathBuf {
    if let Some(bin) = source.strip_prefix("target/release/") {
        return workspace_root().join("target/release").join(bin);
    }
    workspace_root().join("crates/cauce-daemon").join(source)
}

#[test]
fn package_manifests_are_complete() {
    let manifest = deb_manifest();
    let table = deb_table(&manifest);

    // 1. One Debian package named `cauce` with exactly the §16 dependencies.
    let name = deb_string(&table, "name");
    assert_eq!(name, "cauce", "el paquete Debian debe llamarse cauce");

    let depends = deb_string(&table, "depends");
    let declared: Vec<&str> = depends.split(',').map(str::trim).collect();
    assert_eq!(
        declared,
        DEPENDS,
        "Depends debe ser exactamente {}, sin $auto ni extras",
        DEPENDS.join(", ")
    );

    // 2. Every asset the manifest references is either a release binary
    //    (checked as a path only) or an existing file in `packaging/`.
    let assets = deb_assets(&table);
    let mut installed: Vec<(String, String, String)> = Vec::new();
    for line in &assets {
        let (source, target, mode) = asset_fields(line);
        if source.starts_with("target/release/") {
            let bin = source
                .strip_prefix("target/release/")
                .expect("prefijo comprobado");
            assert!(
                BINARIES.contains(&bin),
                "{bin} no es uno de los tres binarios de Cauce: {source}"
            );
            assert_eq!(target, "usr/bin/", "{bin} debe instalarse en usr/bin/");
            assert_eq!(mode, "755", "{bin} debe instalarse con modo 755");
        } else {
            let path = asset_source_on_disk(&source);
            assert!(
                path.is_file(),
                "el activo declarado no existe en disco: {}",
                path.display()
            );
            assert!(
                !source.contains("ffmpeg") && !source.contains("ffprobe"),
                "S2: el paquete no embebe binarios de FFmpeg, y declara {source}"
            );
        }
        installed.push((source, target, mode));
    }

    for (source, target, mode) in PACKAGING_ASSETS {
        let expected = format!("../../{source}");
        let found = installed
            .iter()
            .find(|(src, _, _)| *src == expected)
            .unwrap_or_else(|| {
                panic!("falta el activo {expected} en [package.metadata.deb] assets")
            });
        assert_eq!(found.1, target, "{source} debe instalarse en {target}");
        assert_eq!(found.2, mode, "{source} debe instalarse con modo {mode}");
    }

    for bin in BINARIES {
        let source = format!("target/release/{bin}");
        assert!(
            installed.iter().any(|(src, _, _)| *src == source),
            "falta el binario {source} en los activos"
        );
    }

    // 3. The desktop entry points at the GUI binary and the Cauce icon.
    let desktop = read_packaging("packaging/cauce.desktop");
    assert!(
        desktop.contains("Exec=cauce-gui"),
        "el .desktop debe lanzar cauce-gui"
    );
    assert!(
        desktop.contains("Icon=io.github.wilkin.Cauce"),
        "el .desktop debe usar el icono io.github.wilkin.Cauce"
    );
    for category in ["AudioVideo", "AudioVideoEditing"] {
        let key = format!("Categories={category};AudioVideoEditing;");
        assert!(
            desktop.contains("Categories=")
                && desktop.lines().any(|line| line.starts_with("Categories=")
                    && line.contains(&format!("{category};"))),
            "el .desktop debe declarar la categoría {category} (se esperaba algo como {key})"
        );
    }
    assert!(
        desktop.lines().any(|line| line.starts_with("Categories=")
            && line.trim_end().ends_with("AudioVideo;AudioVideoEditing;")),
        "Categories debe ser AudioVideo;AudioVideoEditing;"
    );
    assert!(
        desktop.lines().any(|line| line.starts_with("Comment=")),
        "el .desktop debe llevar un Comment visible"
    );

    // 4. The AppStream file is a real component, not a stub.
    let metainfo = read_packaging("packaging/io.github.wilkin.Cauce.metainfo.xml");
    for element in ["id", "launchable", "provides", "releases"] {
        assert!(
            metainfo.contains(&format!("<{element}>"))
                || metainfo.contains(&format!("<{element} ")),
            "el metainfo debe declarar <{element}>"
        );
    }
    assert!(
        metainfo.contains("<id>io.github.wilkin.Cauce</id>"),
        "el id del metainfo debe coincidir con el nombre de archivo io.github.wilkin.Cauce"
    );
    assert!(
        metainfo.contains("type=\"desktop-id\">cauce.desktop"),
        "launchable debe ser cauce.desktop"
    );
    assert!(
        metainfo.contains("<description>") && metainfo.contains("<p>"),
        "la descripción debe tener al menos un párrafo <p>"
    );

    // 5. The icon is a real SVG at the hicolor scalable path: before the first
    //    `<svg` only the XML declaration and comments may appear, so `<svg>` is
    //    the root element.
    let icon_path =
        workspace_root().join("packaging/icons/hicolor/scalable/apps/io.github.wilkin.Cauce.svg");
    let icon = read(&icon_path);
    let root = icon
        .find("<svg")
        .unwrap_or_else(|| panic!("{} debe tener un elemento <svg>", icon_path.display()));
    let mut in_comment = false;
    for line in icon[..root].lines() {
        let line = line.trim();
        if in_comment {
            in_comment = !line.contains("-->");
            continue;
        }
        if line.is_empty() || line.starts_with("<?xml") {
            continue;
        }
        if line.starts_with("<!--") && !line.contains("-->") {
            in_comment = true;
            continue;
        }
        panic!(
            "{} debe tener <svg> como elemento raíz, y antes aparece {line:?}",
            icon_path.display()
        );
    }
}

#[test]
fn desktop_and_appstream_validate() {
    let root = workspace_root();
    let desktop = root.join("packaging/cauce.desktop");
    let metainfo = root.join("packaging/io.github.wilkin.Cauce.metainfo.xml");

    match which("desktop-file-validate") {
        Some(tool) => {
            let output = Command::new(&tool)
                .arg(&desktop)
                .output()
                .unwrap_or_else(|error| panic!("no se pudo ejecutar {}: {error}", tool.display()));
            assert!(
                output.status.success(),
                "desktop-file-validate falló con {:?}:\n{}{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        None => eprintln!(
            "SKIP desktop_and_appstream_validate: desktop-file-validate no está instalado"
        ),
    }

    match which("appstreamcli") {
        Some(tool) => {
            let output = Command::new(&tool)
                .args(["validate", "--no-net"])
                .arg(&metainfo)
                .output()
                .unwrap_or_else(|error| panic!("no se pudo ejecutar {}: {error}", tool.display()));
            assert!(
                output.status.success(),
                "appstreamcli validate falló con {:?}:\n{}{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        None => eprintln!("SKIP desktop_and_appstream_validate: appstreamcli no está instalado"),
    }
}

/// `PATH` lookup without pulling a dependency for two optional validators.
fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}
