//! Schema, pragma and migration contracts.
//!
//! See `docs/data-model.md` §1 (persistence rules), §2 (logical schema), §6
//! (migrations) and `docs/contracts.md` §2.4. Every test builds its database in
//! a throwaway directory under `std::env::temp_dir()` and removes it at the end,
//! so nothing ever touches `$XDG_DATA_HOME/cauce/cauce.db`.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cauce_core::{ErrorCode, Preset, system_presets};
use cauce_jobs::{MIGRATIONS, Migration, Store, run_migrations};
use rusqlite::Connection;

/// A private database directory that cleans itself up.
struct TempDir {
    dir: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "cauce-jobs-migrations-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir must be creatable");
        Self { dir }
    }

    fn db_path(&self) -> PathBuf {
        self.dir.join("cauce.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn pragma_text(conn: &Connection, name: &str) -> String {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .unwrap_or_else(|error| panic!("PRAGMA {name} must be readable: {error}"))
}

fn pragma_int(conn: &Connection, name: &str) -> i64 {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .unwrap_or_else(|error| panic!("PRAGMA {name} must be readable: {error}"))
}

fn object_exists(conn: &Connection, kind: &str, name: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
        [kind, name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or_else(|error| panic!("sqlite_master must be readable: {error}"))
        == 1
}

fn entries(dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read.filter_map(|entry| Some(entry.ok()?.path())).collect()
}

fn preset_rows(conn: &Connection) -> Vec<(String, String, String, String)> {
    let mut statement = conn
        .prepare("SELECT id, kind, definition_json, updated_at FROM presets ORDER BY id")
        .expect("presets must be queryable");
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .expect("presets must be queryable");
    rows.collect::<Result<Vec<_>, _>>()
        .expect("preset rows must be readable")
}

const SIX_TABLES: [&str; 6] = [
    "schema_version",
    "jobs",
    "job_events",
    "presets",
    "settings",
    "output_reservations",
];

#[test]
fn migrates_an_empty_database() {
    let temp = TempDir::new("empty");
    let path = temp.db_path();
    let store = Store::open(&path).expect("an empty database must migrate");
    assert_eq!(store.schema_version().expect("version must be readable"), 1);

    let conn = store.connection();
    for table in SIX_TABLES {
        assert!(
            object_exists(conn, "table", table),
            "table `{table}` must exist (docs/data-model.md §2)"
        );
    }
    for index in [
        "jobs_state_priority_created",
        "jobs_created_at",
        "jobs_input_path",
    ] {
        assert!(
            object_exists(conn, "index", index),
            "index `{index}` must exist (docs/data-model.md §2.2)"
        );
    }

    assert_eq!(pragma_text(conn, "journal_mode").to_lowercase(), "wal");
    assert_eq!(pragma_int(conn, "foreign_keys"), 1);
    assert_eq!(pragma_int(conn, "busy_timeout"), 5000);
    assert_eq!(pragma_int(conn, "synchronous"), 1, "NORMAL must be 1");
    assert_eq!(
        pragma_int(conn, "user_version"),
        1,
        "user_version mirrors §2.1"
    );
    assert_eq!(
        conn.query_row("SELECT version FROM schema_version", [], |row| row
            .get::<_, i64>(0))
            .expect("schema_version must hold one row"),
        1
    );
}

#[test]
fn migration_is_idempotent() {
    let temp = TempDir::new("idempotent");
    let path = temp.db_path();

    let first = {
        let store = Store::open(&path).expect("first open must migrate");
        assert_eq!(store.schema_version().expect("version"), 1);
        preset_rows(store.connection())
    };
    let second = {
        let store = Store::open(&path).expect("second open must not migrate again");
        assert_eq!(store.schema_version().expect("version"), 1);
        preset_rows(store.connection())
    };

    assert_eq!(first.len(), 13, "the catalog seeds thirteen presets");
    assert_eq!(
        first, second,
        "a re-open must neither duplicate nor reshape the seeded presets"
    );
}

#[test]
fn failing_migration_rolls_back() {
    let temp = TempDir::new("rollback");
    let path = temp.db_path();
    drop(Store::open(&path).expect("initial migration"));
    let mut conn = Connection::open(&path).expect("reopen the migrated file");

    let broken = Migration {
        version: 2,
        sql: "CREATE TABLE doomed (id TEXT);\nINSERT INTO does_not_exist (id) VALUES ('x');",
    };
    let error = run_migrations(&mut conn, &[MIGRATIONS[0], broken])
        .expect_err("a failing migration must be reported");

    assert_eq!(error.code, ErrorCode::Internal);
    assert!(!error.retryable);
    assert_eq!(
        pragma_int(&conn, "user_version"),
        1,
        "version must not advance"
    );
    assert!(
        !object_exists(&conn, "table", "doomed"),
        "no partial tables"
    );
    assert_eq!(
        conn.query_row("SELECT version FROM schema_version", [], |row| row
            .get::<_, i64>(0))
            .expect("schema_version must keep the previous row"),
        1
    );

    let backups: Vec<PathBuf> = entries(&temp.dir.join("backups"));
    assert_eq!(
        backups.len(),
        1,
        "an existing database must be backed up before migrating, found {backups:?}"
    );
}

#[test]
fn rejects_a_newer_database() {
    let temp = TempDir::new("newer");
    let path = temp.db_path();
    {
        let conn = Connection::open(&path).expect("create the file");
        conn.pragma_update(None, "user_version", 99)
            .expect("stamp a future version");
    }

    let error = Store::open(&path).expect_err("a newer database must be refused");
    assert_eq!(error.code, ErrorCode::Internal);
    assert!(!error.retryable);
    assert!(
        error.message.contains("DATABASE_NEWER_THAN_APP"),
        "message must name the condition, got `{}`",
        error.message
    );

    let readonly = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("the file must still be readable");
    assert!(
        !object_exists(&readonly, "table", "jobs"),
        "a refused database must not be opened read-write"
    );
    assert_eq!(pragma_int(&readonly, "user_version"), 99);
}

#[test]
fn seeds_the_thirteen_system_presets() {
    let temp = TempDir::new("presets");
    let path = temp.db_path();
    let store = Store::open(&path).expect("open must seed the catalog");
    let rows = preset_rows(store.connection());

    let mut catalog: Vec<&Preset> = system_presets().iter().collect();
    catalog.sort_by(|left, right| left.id.cmp(&right.id));
    let expected: Vec<&str> = catalog.iter().map(|preset| preset.id.as_str()).collect();
    let stored: Vec<&str> = rows.iter().map(|row| row.0.as_str()).collect();

    assert_eq!(stored.len(), 13);
    assert_eq!(
        stored, expected,
        "the table holds exactly the 13 catalog IDs"
    );

    for (row, preset) in rows.iter().zip(catalog.iter()) {
        assert_eq!(row.1, "system");
        let definition: Preset =
            serde_json::from_str(&row.2).expect("definition_json must round-trip");
        assert_eq!(&definition, *preset);
        assert!(
            row.3.ends_with('Z'),
            "`{}` must be a UTC RFC 3339 timestamp",
            row.3
        );
    }
}
