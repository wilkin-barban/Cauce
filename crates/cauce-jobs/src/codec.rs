//! Row shape of one `jobs` record (`docs/data-model.md` §2.2).

use cauce_core::{ErrorCode, Job, JobError, Progress};
use rusqlite::Row;
use serde_json::Value as JsonValue;

use crate::sql_support::{is_retryable, json_error, parse_plan, parse_probe, stamp};
/// One `jobs` row as stored, before it is validated back into the domain.
pub(crate) struct JobRow {
    id: String,
    input_path: String,
    output_path: String,
    temp_path: Option<String>,
    preset_id: String,
    state: String,
    priority: i32,
    progress_json: Option<String>,
    probe_json: Option<String>,
    plan_json: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
}

impl JobRow {
    pub(crate) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            input_path: row.get(1)?,
            output_path: row.get(2)?,
            temp_path: row.get(3)?,
            preset_id: row.get(4)?,
            state: row.get(5)?,
            priority: row.get(6)?,
            progress_json: row.get(7)?,
            probe_json: row.get(8)?,
            plan_json: row.get(9)?,
            error_code: row.get(10)?,
            error_message: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
            started_at: row.get(14)?,
            finished_at: row.get(15)?,
        })
    }

    pub(crate) fn into_job(self) -> Result<Job, JobError> {
        let updated_at = stamp(&self.updated_at)?;
        let progress = match &self.progress_json {
            Some(text) => serde_json::from_str(text).map_err(json_error)?,
            None => Progress {
                percent: None,
                processed_ms: 0,
                total_ms: 0,
                fps: None,
                speed: None,
                bitrate_bps: None,
                eta_seconds: None,
                updated_at: updated_at.clone(),
            },
        };
        let error = match self.error_code {
            Some(text) => {
                let code: ErrorCode =
                    serde_json::from_value(JsonValue::String(text)).map_err(json_error)?;
                Some(JobError::new(
                    code,
                    self.error_message.unwrap_or_default(),
                    is_retryable(code),
                ))
            }
            None => None,
        };

        Ok(Job {
            id: self.id,
            input_path: self.input_path,
            output_path: self.output_path,
            temp_path: self.temp_path,
            preset_id: self.preset_id,
            state: serde_json::from_value(JsonValue::String(self.state)).map_err(json_error)?,
            priority: self.priority,
            progress,
            probe: parse_probe(self.probe_json)?,
            plan: parse_plan(self.plan_json)?,
            error,
            created_at: stamp(&self.created_at)?,
            updated_at,
            started_at: self.started_at.as_deref().map(stamp).transpose()?,
            finished_at: self.finished_at.as_deref().map(stamp).transpose()?,
        })
    }
}
