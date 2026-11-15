//! `Job`, its state machine vocabulary, progress and stable errors.
//!
//! See `docs/contracts.md` §2.4, §2.5, §4 and §8.

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, JobError};
use crate::media::MediaSnapshot;
use crate::plan::ConversionPlan;
use crate::time::Timestamp;
use crate::validate;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// UUIDv7 minted by the daemon.
    pub id: String,
    pub input_path: String,
    pub output_path: String,
    /// Unique temporary file, present only while the job is active.
    pub temp_path: Option<String>,
    pub preset_id: String,
    pub state: JobState,
    #[serde(deserialize_with = "validate::priority")]
    pub priority: i32,
    pub progress: Progress,
    pub probe: Option<MediaSnapshot>,
    pub plan: Option<ConversionPlan>,
    pub error: Option<JobError>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

impl Job {
    /// Checks the invariants of `docs/contracts.md` §3 that hold for any state.
    pub fn validate(&self) -> Result<(), JobError> {
        if self.output_path == self.input_path {
            return Err(JobError::new(
                ErrorCode::InvalidInput,
                "La ruta de salida no puede ser igual a la de entrada.",
                false,
            ));
        }

        Ok(())
    }
}

/// States from `docs/contracts.md` §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Probing,
    Running,
    Paused,
    Validating,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// `None` while the total duration is unknown; the UI shows indeterminate mode.
    #[serde(deserialize_with = "validate::percent")]
    pub percent: Option<f64>,
    pub processed_ms: u64,
    pub total_ms: u64,
    pub fps: Option<f64>,
    pub speed: Option<f64>,
    pub bitrate_bps: Option<u64>,
    pub eta_seconds: Option<u64>,
    pub updated_at: Timestamp,
}
