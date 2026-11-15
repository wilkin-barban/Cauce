//! Stable job errors (`docs/contracts.md` §8).
//!
//! Codes are part of the IPC contract: the UI maps them to actions, so they are
//! never renamed without a protocol version bump.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobError {
    pub code: ErrorCode,
    /// Human message. Never contains stack traces or credentials.
    pub message: String,
    pub retryable: bool,
}

impl JobError {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    PathDenied,
    FfmpegMissing,
    FfmpegFeatureMissing,
    ProbeFailed,
    PlanInvalid,
    OutputCollision,
    OutputValidationFailed,
    ProcessFailed,
    Cancelled,
    DaemonUnavailable,
    DatabaseBusy,
    Internal,
}
