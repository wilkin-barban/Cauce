//! The SQL surface: statements, the row codec they are coupled to and the
//! rusqlite-to-`JobError` mapping.
//!
//! The DDL follows `docs/data-model.md` §2 exactly: six tables, three indices on
//! `jobs`, the `CHECK` constraints of §2.2 and the `CHECK` on preset `kind` from
//! §2.4. Timestamps are stored as the RFC 3339 text the domain already carries.

/// Version 1 of the schema (`docs/data-model.md` §2.1-§2.6).
pub const CREATE_SCHEMA: &str = r#"
CREATE TABLE schema_version (
    version     INTEGER PRIMARY KEY,
    applied_at  TEXT NOT NULL,
    app_version TEXT NOT NULL
);

CREATE TABLE jobs (
    id             TEXT PRIMARY KEY,
    input_path     TEXT NOT NULL,
    output_path    TEXT NOT NULL,
    temp_path      TEXT,
    preset_id      TEXT NOT NULL,
    state          TEXT NOT NULL CHECK (state IN (
        'queued', 'probing', 'running', 'paused', 'validating', 'cancelling',
        'completed', 'cancelled', 'failed', 'interrupted'
    )),
    priority       INTEGER NOT NULL DEFAULT 0
                   CHECK (priority BETWEEN -1000 AND 1000),
    progress_json  TEXT,
    probe_json     TEXT,
    plan_json      TEXT,
    error_code     TEXT,
    error_message  TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    started_at     TEXT,
    finished_at    TEXT,
    CHECK (output_path <> input_path),
    CHECK (state <> 'completed' OR finished_at IS NOT NULL),
    CHECK (state <> 'failed' OR error_code IS NOT NULL)
);

CREATE INDEX jobs_state_priority_created ON jobs (state, priority, created_at);
CREATE INDEX jobs_created_at ON jobs (created_at);
CREATE INDEX jobs_input_path ON jobs (input_path);

CREATE TABLE job_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id     TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    from_state TEXT,
    to_state   TEXT NOT NULL CHECK (to_state IN (
        'queued', 'probing', 'running', 'paused', 'validating', 'cancelling',
        'completed', 'cancelled', 'failed', 'interrupted'
    )),
    reason     TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE presets (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('system', 'custom')),
    name_es         TEXT NOT NULL,
    name_en         TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value_json  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE output_reservations (
    path       TEXT PRIMARY KEY,
    job_id     TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL
);
"#;

/// Column order read back by [`SELECT_JOB`]; never change it without [`JobRow`].
pub const JOB_COLUMNS: &str = "id, input_path, output_path, temp_path, preset_id, state, \
     priority, progress_json, probe_json, plan_json, error_code, error_message, \
     created_at, updated_at, started_at, finished_at";

pub const INSERT_JOB: &str = "INSERT INTO jobs (id, input_path, output_path, temp_path, \
     preset_id, state, priority, progress_json, probe_json, plan_json, error_code, \
     error_message, created_at, updated_at, started_at, finished_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)";

pub const INSERT_RESERVATION: &str =
    "INSERT INTO output_reservations (path, job_id, created_at) VALUES (?1, ?2, ?3)";

pub const INSERT_EVENT: &str = "INSERT INTO job_events (job_id, from_state, to_state, reason, \
     created_at) VALUES (?1, ?2, ?3, ?4, ?5)";

pub const SELECT_JOB: &str = "SELECT id, input_path, output_path, temp_path, preset_id, state, \
     priority, progress_json, probe_json, plan_json, error_code, error_message, \
     created_at, updated_at, started_at, finished_at FROM jobs WHERE id = ?1";

/// States that a daemon restart must convert into `interrupted`
/// (`docs/data-model.md` §4). Ordered so the recovery events are deterministic.
pub const SELECT_ACTIVE_JOBS: &str = "SELECT id, state FROM jobs \
     WHERE state IN ('probing', 'running', 'paused', 'validating') \
     ORDER BY created_at ASC, id ASC";

pub const UPDATE_JOB_STATE: &str = "UPDATE jobs SET state = ?2, updated_at = ?3 WHERE id = ?1";

/// Reservation cleanup for terminal jobs (`docs/data-model.md` §4 step 4).
pub const DELETE_INACTIVE_RESERVATIONS: &str = "DELETE FROM output_reservations \
     WHERE job_id IN (SELECT id FROM jobs WHERE state IN ('completed', 'cancelled', 'failed'))";

pub const UPSERT_SETTING: &str = "INSERT INTO settings (key, value_json, updated_at) \
     VALUES (?1, ?2, ?3) ON CONFLICT (key) DO UPDATE SET \
     value_json = excluded.value_json, updated_at = excluded.updated_at";

pub const SELECT_SETTINGS: &str = "SELECT key, value_json FROM settings";

/// Seeds one system preset. `updated_at` is never rewritten, so a re-open is a
/// no-op instead of a visible edit; the update only reshapes system rows.
pub const UPSERT_PRESET: &str = "INSERT INTO presets \
     (id, kind, name_es, name_en, definition_json, created_at, updated_at) \
     VALUES (?1, 'system', ?2, ?3, ?4, ?5, ?5) ON CONFLICT (id) DO UPDATE SET \
     name_es = excluded.name_es, name_en = excluded.name_en, \
     definition_json = excluded.definition_json WHERE presets.kind = 'system'";

pub const CLEAR_SCHEMA_VERSION: &str = "DELETE FROM schema_version";

pub const INSERT_SCHEMA_VERSION: &str =
    "INSERT INTO schema_version (version, applied_at, app_version) VALUES (?1, ?2, ?3)";

/// Path of the main database file; empty for in-memory databases.
pub const SELECT_DATABASE_PATH: &str = "SELECT file FROM pragma_database_list WHERE name = 'main'";
