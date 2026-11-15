//! Shared SQL error mapping for the Cauce job store.

use cauce_core::{ErrorCode, JobError};
use rusqlite::ErrorCode as SqlErrorCode;

pub(crate) fn internal(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::Internal, message, false)
}

pub(crate) fn sql_error(error: rusqlite::Error, context: &str) -> JobError {
    let busy = matches!(&error, rusqlite::Error::SqliteFailure(failure, _)
        if matches!(failure.code, SqlErrorCode::DatabaseBusy | SqlErrorCode::DatabaseLocked));
    let code = if busy {
        ErrorCode::DatabaseBusy
    } else {
        ErrorCode::Internal
    };
    JobError::new(code, format!("No se pudo {context}: {error}"), busy)
}

pub(crate) fn json_error(error: serde_json::Error) -> JobError {
    internal(format!("JSON inválido en la base: {error}"))
}
