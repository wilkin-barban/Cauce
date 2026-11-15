//! The job store: one SQLite connection in WAL mode.
//!
//! Contracts live in `docs/data-model.md` §1 (pragmas), §2 (schema), §3
//! (transactions), §4 (recovery) and §7 (integrity invariants). Every write that
//! touches more than one row runs in a single transaction, so a rejected
//! reservation leaves neither a job nor an event behind.

use std::path::Path;
use std::time::Duration;

use cauce_core::{
    ConversionPlan, DuplicatePolicy, ErrorCode, JobError, Language, MediaSnapshot, Progress,
    Settings, Theme,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde_json::Value as JsonValue;

use crate::migrations::{self, MIGRATIONS, seed_system_presets};
use crate::sql;
use crate::sql_support::{
    code_text, internal, json_error, plan_text, probe_text, progress_text, sql_error,
};

const BUSY_TIMEOUT_MS: u64 = 5_000;

/// Writes the frozen probe and plan of a job before it starts running
/// (`docs/contracts.md` §3: a persisted plan is never recomputed).
const UPDATE_PROBE_AND_PLAN: &str =
    "UPDATE jobs SET probe_json = ?2, plan_json = ?3, updated_at = ?4 WHERE id = ?1";

/// The temporary output, present only while a job is active.
const UPDATE_TEMP_PATH: &str = "UPDATE jobs SET temp_path = ?2, updated_at = ?3 WHERE id = ?1";

/// Error columns. The schema requires `error_code` on a `failed` row, so this
/// runs *before* the transition to `failed` (`docs/data-model.md` §2.2).
const UPDATE_ERROR: &str =
    "UPDATE jobs SET error_code = ?2, error_message = ?3, updated_at = ?4 WHERE id = ?1";

/// `progress_json` and `updated_at` only: a sample is not a state change and
/// writes no `job_events` row (`docs/data-model.md` §3 "Progreso").
const UPDATE_PROGRESS: &str = "UPDATE jobs SET progress_json = ?2, updated_at = ?3 WHERE id = ?1";

#[derive(Debug)]
pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// Opens `path`, applies the connection pragmas and migrates it.
    ///
    /// A database stamped by a newer app is inspected read-only and refused
    /// before any write, per `docs/data-model.md` §6.
    pub fn open(path: &Path) -> Result<Self, JobError> {
        let supported = supported_version();
        if let Some(current) = probe_user_version(path).filter(|current| *current > supported) {
            return Err(migrations::newer_than_app(current, supported));
        }
        let mut conn =
            Connection::open(path).map_err(|error| sql_error(error, "abrir la base de datos"))?;
        let current = migrations::user_version(&conn)?;
        if current > supported {
            return Err(migrations::newer_than_app(current, supported));
        }
        configure(&conn)?;
        migrations::run_migrations(&mut conn, MIGRATIONS)?;
        seed_system_presets(&mut conn)?;
        Ok(Self { conn })
    }

    /// Raw connection. Sibling crates and tests use it for queries the typed API
    /// does not expose yet; no schema knowledge lives outside this crate.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn schema_version(&self) -> Result<u32, JobError> {
        let version: Option<i64> = self
            .conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .optional()
            .map_err(|error| sql_error(error, "leer la versión del esquema"))?;
        Ok(version.map_or(0, |value| u32::try_from(value).unwrap_or(0)))
    }

    /// Reads the `settings` table and validates it through `Settings`' serde
    /// contract; keys that were never written fall back to their defaults.
    pub fn settings_get(&self) -> Result<Settings, JobError> {
        let defaults = serde_json::to_value(default_settings()).map_err(json_error)?;
        let JsonValue::Object(mut entries) = defaults else {
            return Err(internal(
                "los ajustes por defecto no serializan como objeto",
            ));
        };

        let mut statement = self
            .conn
            .prepare(sql::SELECT_SETTINGS)
            .map_err(|error| sql_error(error, "preparar la lectura de ajustes"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| sql_error(error, "leer los ajustes"))?;
        for row in rows {
            let (key, value) = row.map_err(|error| sql_error(error, "leer los ajustes"))?;
            let parsed: JsonValue = serde_json::from_str(&value).map_err(json_error)?;
            entries.insert(key, parsed);
        }

        serde_json::from_value(JsonValue::Object(entries)).map_err(|error| {
            JobError::new(
                ErrorCode::InvalidInput,
                format!("Ajustes inválidos en la base: {error}"),
                false,
            )
        })
    }

    /// Validates `settings` through its own serde contract before writing a
    /// single row, then stores one `settings` row per field.
    pub fn settings_set(&mut self, settings: &Settings) -> Result<(), JobError> {
        let value = serde_json::to_value(settings).map_err(json_error)?;
        serde_json::from_value::<Settings>(value.clone()).map_err(|error| {
            JobError::new(
                ErrorCode::InvalidInput,
                format!("Ajustes inválidos: {error}"),
                false,
            )
        })?;
        let JsonValue::Object(entries) = value else {
            return Err(internal("los ajustes no serializan como objeto"));
        };

        let now = migrations::utc_now()?;
        let tx = self
            .conn
            .transaction()
            .map_err(|error| sql_error(error, "abrir la transacción de ajustes"))?;
        for (key, entry) in &entries {
            let encoded = serde_json::to_string(entry).map_err(json_error)?;
            tx.execute(sql::UPSERT_SETTING, params![key, encoded, now.as_str()])
                .map_err(|error| sql_error(error, "guardar los ajustes"))?;
        }
        tx.commit()
            .map_err(|error| sql_error(error, "confirmar los ajustes"))
    }

    /// Persists the probe of the input and the plan the conversion will replay.
    pub fn set_probe_and_plan(
        &mut self,
        id: &str,
        probe: &MediaSnapshot,
        plan: &ConversionPlan,
    ) -> Result<(), JobError> {
        let probe = probe_text(probe)?;
        let plan = plan_text(plan)?;
        let now = migrations::utc_now()?;
        let changed = self
            .conn
            .execute(
                UPDATE_PROBE_AND_PLAN,
                params![id, probe, plan, now.as_str()],
            )
            .map_err(|error| sql_error(error, "guardar el probe y el plan"))?;
        require_row(changed, id)
    }

    /// Records (or clears) the temporary output of an active job.
    pub fn set_temp_path(&mut self, id: &str, temp: Option<&str>) -> Result<(), JobError> {
        let now = migrations::utc_now()?;
        let changed = self
            .conn
            .execute(UPDATE_TEMP_PATH, params![id, temp, now.as_str()])
            .map_err(|error| sql_error(error, "guardar el temporal"))?;
        require_row(changed, id)
    }

    /// Stores the error of a job. Must run before the transition to `failed`:
    /// the schema refuses a `failed` row without `error_code`.
    pub fn set_error(&mut self, id: &str, error: &JobError) -> Result<(), JobError> {
        let code = code_text(error.code)?;
        let now = migrations::utc_now()?;
        let changed = self
            .conn
            .execute(
                UPDATE_ERROR,
                params![id, code, &error.message, now.as_str()],
            )
            .map_err(|sql_error_value| sql_error(sql_error_value, "guardar el error"))?;
        require_row(changed, id)
    }

    /// Stores one progress sample. Called at most once per second per job
    /// (`docs/contracts.md` §7); it never touches the state nor the events.
    pub fn save_progress(&mut self, id: &str, progress: &Progress) -> Result<(), JobError> {
        let encoded = progress_text(progress)?;
        let now = migrations::utc_now()?;
        let changed = self
            .conn
            .execute(UPDATE_PROGRESS, params![id, encoded, now.as_str()])
            .map_err(|error| sql_error(error, "guardar el progreso"))?;
        require_row(changed, id)
    }
}

/// A single-row write that matched nothing names an unknown job, never silence.
fn require_row(changed: usize, id: &str) -> Result<(), JobError> {
    if changed == 0 {
        return Err(JobError::new(
            ErrorCode::InvalidInput,
            format!("No existe el trabajo `{id}`."),
            false,
        ));
    }
    Ok(())
}

/// Defaults from `docs/contracts.md` §2.6 and `docs/ui-mvp.md` §8.
fn default_settings() -> Settings {
    Settings {
        output_dir: String::new(),
        max_concurrent_jobs: 1,
        theme: Theme::System,
        language: Language::Es,
        notifications: false,
        duplicate_policy: DuplicatePolicy::Suffix,
        keep_temp_on_failure: false,
    }
}

fn configure(conn: &Connection) -> Result<(), JobError> {
    conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))
        .map_err(|error| sql_error(error, "configurar busy_timeout"))?;
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(|error| sql_error(error, "activar el modo WAL"))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(internal(format!(
            "no se pudo activar WAL: journal_mode sigue en `{mode}`"
        )));
    }
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")
        .map_err(|error| sql_error(error, "configurar foreign_keys y synchronous"))
}

fn supported_version() -> u32 {
    MIGRATIONS
        .iter()
        .map(|step| step.version)
        .max()
        .unwrap_or(0)
}

/// Reads `user_version` through a read-only connection, so a database written by
/// a newer app is never opened read-write. A WAL left behind by a crash can make
/// this probe impossible; the read-write check in [`Store::open`] then still runs
/// before any pragma or migration writes.
fn probe_user_version(path: &Path) -> Option<u32> {
    if !path.exists() {
        return Some(0);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    migrations::user_version(&conn).ok()
}
