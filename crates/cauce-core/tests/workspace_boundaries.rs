//! Structural guard for the workspace layout fixed by `docs/architecture.md` §5.
//!
//! The workspace must expose exactly the seven Cauce crates and must not contain
//! inverted dependency edges: the domain core cannot reach into GTK, Tokio,
//! SQLite or process handling, and the GUI cannot reach the job store or FFmpeg
//! wrappers. These assertions read `cargo metadata`, so they stay true no matter
//! how the individual crates evolve.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// Crates required by `docs/architecture.md` §5, in no particular order.
const WORKSPACE_CRATES: [&str; 7] = [
    "cauce-core",
    "cauce-ffmpeg",
    "cauce-jobs",
    "cauce-i18n",
    "cauce-daemon",
    "cauce-gui",
    "cauce-cli",
];

/// Workspace-internal edges allowed by `docs/architecture.md` §5.
///
/// A crate absent from this map may not depend on any other Cauce crate.
fn allowed_workspace_edges() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    BTreeMap::from([
        ("cauce-core", BTreeSet::from([])),
        ("cauce-i18n", BTreeSet::from([])),
        ("cauce-ffmpeg", BTreeSet::from(["cauce-core"])),
        ("cauce-jobs", BTreeSet::from(["cauce-core"])),
        (
            "cauce-daemon",
            BTreeSet::from(["cauce-core", "cauce-ffmpeg", "cauce-jobs"]),
        ),
        ("cauce-gui", BTreeSet::from(["cauce-core", "cauce-i18n"])),
        ("cauce-cli", BTreeSet::from(["cauce-core", "cauce-i18n"])),
    ])
}

/// External crates each layer must never pull in, as required by §5's rules.
fn forbidden_external_deps() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    BTreeMap::from([
        (
            "cauce-core",
            BTreeSet::from([
                "async-std",
                "gdk4",
                "gio",
                "glib",
                "gtk",
                "gtk4",
                "libadwaita",
                "rusqlite",
                "tokio",
            ]),
        ),
        (
            "cauce-i18n",
            BTreeSet::from([
                "gdk4",
                "gio",
                "glib",
                "gtk",
                "gtk4",
                "libadwaita",
                "rusqlite",
            ]),
        ),
        (
            "cauce-jobs",
            BTreeSet::from(["gdk4", "gio", "glib", "gtk", "gtk4", "libadwaita"]),
        ),
        ("cauce-gui", BTreeSet::from(["rusqlite"])),
    ])
}

fn metadata() -> serde_json::Value {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest = format!("{manifest_dir}/Cargo.toml");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));

    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("cargo metadata must be runnable");

    assert!(
        output.status.success(),
        "cargo metadata failed for {manifest}: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("cargo metadata must emit JSON")
}

fn workspace_members() -> BTreeMap<String, BTreeSet<String>> {
    let metadata = metadata();
    let packages = metadata
        .get("packages")
        .and_then(|packages| packages.as_array())
        .expect("cargo metadata must report packages");

    let mut members = BTreeMap::new();
    for package in packages {
        let name = package
            .get("name")
            .and_then(|name| name.as_str())
            .expect("every package has a name")
            .to_string();

        let deps = package
            .get("dependencies")
            .and_then(|deps| deps.as_array())
            .expect("every package reports dependencies")
            .iter()
            .filter_map(|dep| dep.get("name").and_then(|name| name.as_str()))
            .map(str::to_string)
            .collect();

        members.insert(name, deps);
    }

    members
}

#[test]
fn workspace_exposes_exactly_the_seven_crates() {
    let members = workspace_members();
    let expected: BTreeSet<String> = WORKSPACE_CRATES.iter().map(|n| n.to_string()).collect();
    let actual: BTreeSet<String> = members.keys().cloned().collect();

    assert_eq!(
        actual, expected,
        "workspace members must be exactly {expected:?}, found {actual:?}"
    );
}

#[test]
fn no_inverted_dependency_edges() {
    let members = workspace_members();
    let allowed = allowed_workspace_edges();
    let known: BTreeSet<String> = WORKSPACE_CRATES.iter().map(|n| n.to_string()).collect();

    for (package, deps) in &members {
        let permitted = allowed
            .get(package.as_str())
            .unwrap_or_else(|| panic!("{package} is not part of the documented workspace"));

        for dep in deps.intersection(&known) {
            assert!(
                permitted.contains(dep.as_str()),
                "{package} must not depend on {dep} (docs/architecture.md §5)"
            );
        }
    }
}

#[test]
fn core_jobs_and_i18n_stay_free_of_ui_and_store_crates() {
    let members = workspace_members();
    let forbidden = forbidden_external_deps();

    for (package, denied) in &forbidden {
        let deps = members
            .get(*package)
            .unwrap_or_else(|| panic!("{package} must exist in the workspace"));

        for dep in denied {
            assert!(
                !deps.contains(*dep),
                "{package} must not depend on {dep} (docs/architecture.md §5)"
            );
        }
    }
}
