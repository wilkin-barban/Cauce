//! The job store: one SQLite connection in WAL mode.
//!
//! Contracts live in `docs/data-model.md` §1 (pragmas), §2 (schema), §3
//! (transactions), §4 (recovery) and §7 (integrity invariants). Every write that
//! touches more than one row runs in a single transaction, so a rejected
//! reservation leaves neither a job nor an event behind.

use std::path::Path;
use std::time::Duration;

use cauce_core::{
    ConversionPlan, CustomPreset, DuplicatePolicy, ErrorCode, JobError, Language, MediaSnapshot,
    Progress, Settings, Theme,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params, params_from_iter};
use serde_json::Value as JsonValue;

use crate::migrations::{self, MIGRATIONS, seed_system_presets};
use crate::sql;
use crate::sql_support::{
    code_text, internal, json_error, plan_text, probe_text, progress_text, sql_error, state_text,
};
use crate::transitions::FINISHED_STATES;

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
    /// Deletes every finished job with its events and its output reservation in a
    /// single transaction (`docs/data-model.md` §3 and §5), and reports how many
    /// jobs went.
    ///
    /// The set is [`FINISHED_STATES`], so `queued` and the states that still hold a
    /// process are never touched. Nothing is refused: a clear over a list with no
    /// finished job is a successful zero, and a failure half way rolls the whole
    /// transaction back.
    pub fn clear_finished(&mut self) -> Result<usize, JobError> {
        let states: Vec<String> = FINISHED_STATES
            .iter()
            .map(|state| state_text(*state))
            .collect::<Result<_, _>>()?;
        let placeholders = vec!["?"; states.len()].join(", ");
        let tx = self
            .conn
            .transaction()
            .map_err(|error| sql_error(error, "abrir la transacción de limpieza"))?;
        for (statement, context) in [
            (
                format!(
                    "DELETE FROM job_events WHERE job_id IN \
                     (SELECT id FROM jobs WHERE state IN ({placeholders}))"
                ),
                "borrar los eventos de los trabajos terminados",
            ),
            (
                format!(
                    "DELETE FROM output_reservations WHERE job_id IN \
                     (SELECT id FROM jobs WHERE state IN ({placeholders}))"
                ),
                "liberar las reservas de los trabajos terminados",
            ),
            (
                format!("DELETE FROM jobs WHERE state IN ({placeholders})"),
                "borrar los trabajos terminados",
            ),
        ] {
            tx.execute(&statement, params_from_iter(states.iter()))
                .map_err(|error| sql_error(error, context))?;
        }
        let removed = tx.changes() as usize;
        tx.commit()
            .map_err(|error| sql_error(error, "confirmar la limpieza"))?;
        Ok(removed)
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

// ---------------------------------------------------------------------------
// The one custom preset (`docs/contracts.md` §2.3, `docs/data-model.md` §2.5).
//
// It lives as JSON under the `custom_preset` key of the settings table: one row,
// one value, replaced by saving. A value that cannot be used never breaks the
// daemon — the read reports it and the caller logs the warning — and a rejected
// write never leaves the key behind.
// ---------------------------------------------------------------------------

/// The settings key of the custom preset (`docs/data-model.md` §2.5).
pub const CUSTOM_PRESET_KEY: &str = "custom_preset";

const SELECT_SETTING_VALUE: &str = "SELECT value_json FROM settings WHERE key = ?1";
const DELETE_SETTING: &str = "DELETE FROM settings WHERE key = ?1";

/// What the `custom_preset` key holds right now.
#[derive(Debug, Clone, PartialEq)]
pub enum CustomPresetRead {
    /// The key was never written: the catalog is just the thirteen presets.
    Missing,
    /// The key exists but the value cannot be used. The caller ignores it and
    /// logs `warning`; it never fails the request.
    Ignored { warning: String },
    /// A valid custom preset.
    Ready(CustomPreset),
}

impl CustomPresetRead {
    /// The preset, ignoring a value that cannot be used.
    pub fn preset(self) -> Option<CustomPreset> {
        match self {
            Self::Ready(preset) => Some(preset),
            Self::Missing | Self::Ignored { .. } => None,
        }
    }
}

impl Store {
    /// Reads the custom preset, ignoring an unreadable or invalid value.
    ///
    /// A corrupt row is a leftover, not a reason to refuse `presets.list`, so the
    /// read succeeds and names the problem instead of returning an error.
    pub fn custom_preset(&self) -> Result<CustomPresetRead, JobError> {
        let raw: Option<String> = self
            .conn
            .query_row(SELECT_SETTING_VALUE, [CUSTOM_PRESET_KEY], |row| row.get(0))
            .optional()
            .map_err(|error| sql_error(error, "leer el preset personalizado"))?;
        let Some(raw) = raw else {
            return Ok(CustomPresetRead::Missing);
        };

        let ignored = |reason: String| CustomPresetRead::Ignored {
            warning: format!("el preset personalizado guardado no se usa: {reason}"),
        };
        let parsed: CustomPreset = match serde_json::from_str(&raw) {
            Ok(preset) => preset,
            Err(error) => return Ok(ignored(format!("JSON inválido ({error})"))),
        };
        match parsed.validate() {
            Ok(()) => Ok(CustomPresetRead::Ready(parsed)),
            Err(error) => Ok(ignored(error.message)),
        }
    }

    /// Validates and stores the one custom preset, replacing any previous value.
    pub fn set_custom_preset(&mut self, preset: &CustomPreset) -> Result<(), JobError> {
        // The domain judges the combination before a single byte is written, so a
        // rejected write cannot leave an unusable value behind (`data-model.md` §1).
        preset.validate()?;
        let encoded = serde_json::to_string(preset).map_err(json_error)?;
        let now = migrations::utc_now()?;
        self.conn
            .execute(
                sql::UPSERT_SETTING,
                params![CUSTOM_PRESET_KEY, encoded, now.as_str()],
            )
            .map_err(|error| sql_error(error, "guardar el preset personalizado"))?;
        Ok(())
    }

    /// Deletes the custom preset. Reports whether there was one.
    pub fn clear_custom_preset(&mut self) -> Result<bool, JobError> {
        let deleted = self
            .conn
            .execute(DELETE_SETTING, [CUSTOM_PRESET_KEY])
            .map_err(|error| sql_error(error, "borrar el preset personalizado"))?;
        Ok(deleted > 0)
    }
}
