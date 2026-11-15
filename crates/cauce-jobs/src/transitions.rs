//! Transactional state transitions (`docs/contracts.md` §3, §4 and
//! `docs/data-model.md` §3 "Transición de estado").
//!
//! The state change and its `job_events` row are written by the same transaction,
//! so `job_events` always explains the state a row is in. A rejected transition —
//! unknown id or undocumented edge — returns before writing anything.

use cauce_core::{ErrorCode, Job, JobAction, JobError, JobState, Timestamp, transition_for};
use rusqlite::{OptionalExtension, params};

use crate::codec::JobRow;
use crate::migrations;
use crate::sql;
use crate::sql_support::{sql_error, state_text};
use crate::store::Store;

/// Reason written for an action the user asked for over IPC.
const REASON_USER: &str = "user";

/// Updates the row and stamps the lifecycle columns in one statement.
const UPDATE_JOB_STATE: &str = "UPDATE jobs SET state = ?2, updated_at = ?3, \
     started_at = ?4, finished_at = ?5 WHERE id = ?1";

/// States that occupy a worker slot: `paused` still holds the process and
/// `cancelling` is still finishing.
const COUNT_ACTIVE: &str = "SELECT count(*) FROM jobs \
     WHERE state IN ('probing', 'running', 'paused', 'validating', 'cancelling')";

impl Store {
    /// Validates the transition against the core table, then updates the job and
    /// appends the `job_events` row in one transaction.
    ///
    /// `started_at` is stamped when the job enters `running`; `finished_at` when
    /// it enters `completed`, `cancelled` or `failed`.
    pub fn transition(&mut self, id: &str, to: JobState, reason: &str) -> Result<Job, JobError> {
        let now = migrations::utc_now()?;
        let tx = self
            .conn
            .transaction()
            .map_err(|error| sql_error(error, "abrir la transacción de estado"))?;

        let current = read_job(&tx, id)?.ok_or_else(|| unknown_job(id))?;
        if !current.state.can_transition_to(to) {
            return Err(JobError::new(
                ErrorCode::InvalidInput,
                format!(
                    "La transición de `{}` a `{}` no está permitida.",
                    current.state.as_str(),
                    to.as_str()
                ),
                false,
            ));
        }

        let state_from = state_text(current.state)?;
        let state_to = state_text(to)?;
        let started_at = if to == JobState::Running {
            Some(now.clone())
        } else {
            current.started_at.clone()
        };
        let finished_at = if matches!(
            to,
            JobState::Completed | JobState::Cancelled | JobState::Failed
        ) {
            Some(now.clone())
        } else {
            current.finished_at.clone()
        };

        tx.execute(
            UPDATE_JOB_STATE,
            params![
                id,
                state_to,
                now.as_str(),
                started_at.as_ref().map(Timestamp::as_str),
                finished_at.as_ref().map(Timestamp::as_str),
            ],
        )
        .map_err(|error| sql_error(error, "actualizar el estado del trabajo"))?;
        tx.execute(
            sql::INSERT_EVENT,
            params![id, state_from, state_to, reason, now.as_str()],
        )
        .map_err(|error| sql_error(error, "registrar el cambio de estado"))?;

        let updated = read_job(&tx, id)?.ok_or_else(|| unknown_job(id))?;
        tx.commit()
            .map_err(|error| sql_error(error, "confirmar el cambio de estado"))?;
        Ok(updated)
    }

    /// Applies a user action through [`transition_for`], so the guards live in
    /// one place: `cauce-core`.
    pub fn apply_action(&mut self, id: &str, action: JobAction) -> Result<Job, JobError> {
        let current = self.get_job(id)?.ok_or_else(|| unknown_job(id))?;
        let target = transition_for(action, current.state)?;
        self.transition(id, target, REASON_USER)
    }

    /// Count of jobs currently in the states that occupy a worker slot.
    pub fn active_count(&self) -> Result<usize, JobError> {
        let count: i64 = self
            .conn
            .query_row(COUNT_ACTIVE, [], |row| row.get(0))
            .map_err(|error| sql_error(error, "contar los trabajos activos"))?;
        Ok(usize::try_from(count).unwrap_or(0))
    }
}

/// Unknown ids are a request error, never a retryable failure.
fn unknown_job(id: &str) -> JobError {
    JobError::new(
        ErrorCode::InvalidInput,
        format!("No existe el trabajo `{id}`."),
        false,
    )
}

fn read_job(conn: &rusqlite::Connection, id: &str) -> Result<Option<Job>, JobError> {
    let row = conn
        .query_row(sql::SELECT_JOB, params![id], JobRow::read)
        .optional()
        .map_err(|error| sql_error(error, "leer el trabajo"))?;
    row.map(JobRow::into_job).transpose()
}
