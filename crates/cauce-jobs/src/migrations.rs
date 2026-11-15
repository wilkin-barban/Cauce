//! Transactional, versioned migrations with a restorable backup.
//!
//! `docs/data-model.md` §6 fixes the contract: `schema_version` starts at 1, each
//! migration runs in its own transaction, a failure leaves the previous version
//! intact, an existing database is backed up first, and a database written by a
//! newer app is refused with `DATABASE_NEWER_THAN_APP`.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cauce_core::{ErrorCode, JobError, Timestamp, system_presets};
use rusqlite::{Connection, params};

use crate::sql;
use crate::sql_support::{internal, sql_error};

/// One schema step. `version` is monotonic; `sql` runs inside a transaction.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

/// The migration chain this build knows how to apply.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: sql::CREATE_SCHEMA,
}];

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const NEWER_THAN_APP: &str = "DATABASE_NEWER_THAN_APP";

/// Applies every migration newer than the file's `user_version` and returns the
/// resulting version. Each step commits its DDL together with its
/// `schema_version` row and the mirrored `user_version`, so a failure rolls the
/// step back completely.
pub fn run_migrations(conn: &mut Connection, migrations: &[Migration]) -> Result<u32, JobError> {
    let current = user_version(conn)?;
    let highest = migrations
        .iter()
        .map(|step| step.version)
        .max()
        .unwrap_or(0);
    if current > highest {
        return Err(newer_than_app(current, highest));
    }

    let mut version = current;
    for migration in migrations.iter().filter(|step| step.version > current) {
        if version > 0 {
            backup(conn, version)?;
        }
        let applied_at = utc_now()?;
        let tx = conn
            .transaction()
            .map_err(|error| sql_error(error, "abrir la transacción de migración"))?;
        tx.execute_batch(migration.sql)
            .map_err(|error| sql_error(error, "aplicar la migración"))?;
        tx.execute(sql::CLEAR_SCHEMA_VERSION, [])
            .map_err(|error| sql_error(error, "reemplazar schema_version"))?;
        tx.execute(
            sql::INSERT_SCHEMA_VERSION,
            params![migration.version, applied_at.as_str(), APP_VERSION],
        )
        .map_err(|error| sql_error(error, "registrar schema_version"))?;
        tx.pragma_update(None, "user_version", migration.version)
            .map_err(|error| sql_error(error, "escribir user_version"))?;
        tx.commit()
            .map_err(|error| sql_error(error, "confirmar la migración"))?;
        version = migration.version;
    }

    Ok(version)
}

/// Seeds the thirteen canonical presets from `cauce_core` in one transaction.
///
/// The upsert keeps `created_at`/`updated_at` untouched and only rewrites system
/// rows whose definition actually changed, so re-opening a database neither
/// duplicates nor reshapes the catalog (`docs/data-model.md` §2.4).
pub fn seed_system_presets(conn: &mut Connection) -> Result<(), JobError> {
    let now = utc_now()?;
    let tx = conn
        .transaction()
        .map_err(|error| sql_error(error, "abrir la transacción de presets"))?;
    for preset in system_presets() {
        let definition = serde_json::to_string(preset)
            .map_err(|error| internal(format!("catálogo no serializable: {error}")))?;
        tx.execute(
            sql::UPSERT_PRESET,
            params![
                &preset.id,
                &preset.name_es,
                &preset.name_en,
                definition,
                now.as_str()
            ],
        )
        .map_err(|error| sql_error(error, "sembrar los presets del sistema"))?;
    }
    tx.commit()
        .map_err(|error| sql_error(error, "confirmar los presets del sistema"))
}

/// Current `user_version` of the connection.
pub fn user_version(conn: &Connection) -> Result<u32, JobError> {
    let value: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| sql_error(error, "leer user_version"))?;
    Ok(u32::try_from(value).unwrap_or(u32::MAX))
}

/// Refuses a database stamped by a newer app (`docs/data-model.md` §6).
pub fn newer_than_app(current: u32, supported: u32) -> JobError {
    JobError::new(
        ErrorCode::Internal,
        format!(
            "{NEWER_THAN_APP}: la base está en la versión {current} y esta compilación \
             solo conoce hasta la {supported}. Actualizá Cauce para abrirla."
        ),
        false,
    )
}

/// Writes a consistent copy of the database into `<dir>/backups/`.
///
/// `VACUUM INTO` snapshots the file including its WAL, which is what
/// `docs/data-model.md` §5 asks for; copying a live WAL by hand is forbidden.
fn backup(conn: &Connection, version: u32) -> Result<Option<PathBuf>, JobError> {
    let Some(path) = database_path(conn)? else {
        return Ok(None);
    };
    let directory = path
        .parent()
        .map_or_else(|| PathBuf::from("backups"), |parent| parent.join("backups"));
    fs::create_dir_all(&directory).map_err(|error| {
        internal(format!(
            "no se pudo crear el directorio de backups `{}`: {error}",
            directory.display()
        ))
    })?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("cauce");
    let target = directory.join(format!("{stem}-v{version}-{}.db", compact_stamp()?));
    conn.execute("VACUUM INTO ?1", params![target.to_string_lossy()])
        .map_err(|error| sql_error(error, "respaldar la base antes de migrar"))?;
    Ok(Some(target))
}

fn database_path(conn: &Connection) -> Result<Option<PathBuf>, JobError> {
    let file: String = conn
        .query_row(sql::SELECT_DATABASE_PATH, [], |row| row.get(0))
        .map_err(|error| sql_error(error, "leer la ruta de la base"))?;
    Ok(if file.is_empty() {
        None
    } else {
        Some(PathBuf::from(file))
    })
}

/// Current UTC time in the exact `Timestamp` shape the domain stores.
pub fn utc_now() -> Result<Timestamp, JobError> {
    Timestamp::parse(&render(moment()?))
        .map_err(|error| internal(format!("reloj del sistema inválido: {error}")))
}

/// Current UTC time in a filename-safe shape, `YYYYMMDDTHHMMSSZ`.
fn compact_stamp() -> Result<String, JobError> {
    Ok(render(moment()?).replace(['-', ':'], ""))
}

fn moment() -> Result<i64, JobError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .map_err(|error| internal(format!("el reloj del sistema está antes de 1970: {error}")))
}

/// Renders `seconds` since the epoch as `YYYY-MM-DDTHH:MM:SSZ`.
fn render(seconds: i64) -> String {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let rest = seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
