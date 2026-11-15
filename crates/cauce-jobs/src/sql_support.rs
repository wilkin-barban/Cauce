//! Shared SQL error mapping for the Cauce job store.

use cauce_core::{
    ConversionPlan, ErrorCode, JobError, JobState, MediaSnapshot, Progress, Timestamp,
};
use rusqlite::ErrorCode as SqlErrorCode;
use serde_json::Value as JsonValue;

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
pub(crate) fn is_retryable(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::OutputCollision
            | ErrorCode::OutputValidationFailed
            | ErrorCode::ProcessFailed
            | ErrorCode::Cancelled
            | ErrorCode::DaemonUnavailable
            | ErrorCode::DatabaseBusy
    )
}

pub(crate) fn stamp(text: &str) -> Result<Timestamp, JobError> {
    Timestamp::parse(text)
        .map_err(|error| internal(format!("timestamp inválido en la base: {error}")))
}

pub(crate) fn state_text(state: JobState) -> Result<String, JobError> {
    match serde_json::to_value(state).map_err(json_error)? {
        JsonValue::String(text) => Ok(text),
        _ => Err(internal("el estado no serializa como texto")),
    }
}

pub(crate) fn code_text(code: ErrorCode) -> Result<String, JobError> {
    match serde_json::to_value(code).map_err(json_error)? {
        JsonValue::String(text) => Ok(text),
        _ => Err(internal("el código de error no serializa como texto")),
    }
}

pub(crate) fn progress_text(progress: &Progress) -> Result<String, JobError> {
    serde_json::to_string(progress).map_err(json_error)
}

pub(crate) fn probe_text(probe: &MediaSnapshot) -> Result<String, JobError> {
    serde_json::to_string(probe).map_err(json_error)
}

pub(crate) fn plan_text(plan: &ConversionPlan) -> Result<String, JobError> {
    serde_json::to_string(plan).map_err(json_error)
}

pub(crate) fn parse_probe(text: Option<String>) -> Result<Option<MediaSnapshot>, JobError> {
    text.map(|text| serde_json::from_str(&text).map_err(json_error))
        .transpose()
}

pub(crate) fn parse_plan(text: Option<String>) -> Result<Option<ConversionPlan>, JobError> {
    text.map(|text| serde_json::from_str(&text).map_err(json_error))
        .transpose()
}

pub(crate) fn is_constraint_violation(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(failure, _)
        if failure.code == SqlErrorCode::ConstraintViolation)
}
