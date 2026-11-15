//! Job writes: insertion with reservation, reads, recovery and release.

const REASON_USER: &str = "user";
const REASON_RECOVERY: &str = "recovery";
const INTERRUPTED: &str = "interrupted";

use cauce_core::{ErrorCode, Job, JobError, JobState, Timestamp};
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, params, params_from_iter};

use crate::codec::JobRow;
use crate::migrations;
use crate::sql;
use crate::sql_support::{
    code_text, is_constraint_violation, plan_text, probe_text, progress_text, sql_error, state_text,
};
use crate::store::Store;

impl Store {
    pub fn insert_job(&mut self, job: &Job) -> Result<(), JobError> {
        job.validate()?;
        let state = state_text(job.state)?;
        let progress = progress_text(&job.progress)?;
        let probe = job.probe.as_ref().map(probe_text).transpose()?;
        let plan = job.plan.as_ref().map(plan_text).transpose()?;
        let (error_code, error_message) = match &job.error {
            Some(error) => (Some(code_text(error.code)?), Some(error.message.clone())),
            None => (None, None),
        };

        let tx = self
            .conn
            .transaction()
            .map_err(|error| sql_error(error, "abrir la transacción de alta"))?;
        tx.execute(
            sql::INSERT_JOB,
            params![
                &job.id,
                &job.input_path,
                &job.output_path,
                job.temp_path.as_deref(),
                &job.preset_id,
                state,
                job.priority,
                progress,
                probe,
                plan,
                error_code,
                error_message,
                job.created_at.as_str(),
                job.updated_at.as_str(),
                job.started_at.as_ref().map(Timestamp::as_str),
                job.finished_at.as_ref().map(Timestamp::as_str),
            ],
        )
        .map_err(|error| sql_error(error, "insertar el trabajo"))?;

        if let Err(error) = tx.execute(
            sql::INSERT_RESERVATION,
            params![&job.output_path, &job.id, job.created_at.as_str()],
        ) {
            if is_constraint_violation(&error) {
                return Err(JobError::new(
                    ErrorCode::OutputCollision,
                    format!(
                        "El destino `{}` ya está reservado por otro trabajo activo.",
                        job.output_path
                    ),
                    true,
                ));
            }
            return Err(sql_error(error, "reservar la ruta de salida"));
        }

        tx.execute(
            sql::INSERT_EVENT,
            params![
                &job.id,
                Option::<String>::None,
                "queued",
                REASON_USER,
                job.created_at.as_str()
            ],
        )
        .map_err(|error| sql_error(error, "registrar el evento de alta"))?;
        tx.commit()
            .map_err(|error| sql_error(error, "confirmar el alta del trabajo"))
    }

    pub fn get_job(&self, id: &str) -> Result<Option<Job>, JobError> {
        let row = self
            .conn
            .query_row(sql::SELECT_JOB, params![id], JobRow::read)
            .optional()
            .map_err(|error| sql_error(error, "leer el trabajo"))?;
        row.map(JobRow::into_job).transpose()
    }

    /// `priority DESC, created_at ASC`, with `id` as a deterministic tiebreak.
    pub fn list_jobs(&self, states: Option<&[JobState]>) -> Result<Vec<Job>, JobError> {
        let mut query = format!("SELECT {} FROM jobs", sql::JOB_COLUMNS);
        let mut filters: Vec<SqlValue> = Vec::new();
        if let Some(states) = states {
            if states.is_empty() {
                return Ok(Vec::new());
            }
            let placeholders = vec!["?"; states.len()].join(", ");
            query.push_str(&format!(" WHERE state IN ({placeholders})"));
            for state in states {
                filters.push(SqlValue::Text(state_text(*state)?));
            }
        }
        query.push_str(" ORDER BY priority DESC, created_at ASC, id ASC");

        let mut statement = self
            .conn
            .prepare(&query)
            .map_err(|error| sql_error(error, "preparar el listado"))?;
        let rows = statement
            .query_map(params_from_iter(filters), JobRow::read)
            .map_err(|error| sql_error(error, "listar los trabajos"))?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(
                row.map_err(|error| sql_error(error, "leer el listado"))?
                    .into_job()?,
            );
        }
        Ok(jobs)
    }

    /// Recovery step from `docs/data-model.md` §4: every `probing`, `running`,
    /// `paused` or `validating` job becomes `interrupted` with a `recovery`
    /// event. `queued` jobs stay queued for the scheduler.
    pub fn mark_active_as_interrupted(&mut self) -> Result<usize, JobError> {
        let now = migrations::utc_now()?;
        let tx = self
            .conn
            .transaction()
            .map_err(|error| sql_error(error, "abrir la transacción de recuperación"))?;
        let active: Vec<(String, String)> = {
            let mut statement = tx
                .prepare(sql::SELECT_ACTIVE_JOBS)
                .map_err(|error| sql_error(error, "preparar la recuperación"))?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|error| sql_error(error, "buscar los trabajos activos"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| sql_error(error, "leer los trabajos activos"))?
        };

        for (id, from_state) in &active {
            tx.execute(
                sql::UPDATE_JOB_STATE,
                params![id, INTERRUPTED, now.as_str()],
            )
            .map_err(|error| sql_error(error, "interrumpir el trabajo"))?;
            tx.execute(
                sql::INSERT_EVENT,
                params![id, from_state, INTERRUPTED, REASON_RECOVERY, now.as_str()],
            )
            .map_err(|error| sql_error(error, "registrar la recuperación"))?;
        }
        tx.commit()
            .map_err(|error| sql_error(error, "confirmar la recuperación"))?;
        Ok(active.len())
    }

    /// Drops the reservations of `completed`, `cancelled` and `failed` jobs.
    pub fn release_inactive_reservations(&mut self) -> Result<usize, JobError> {
        let tx = self
            .conn
            .transaction()
            .map_err(|error| sql_error(error, "abrir la transacción de liberación"))?;
        tx.execute(sql::DELETE_INACTIVE_RESERVATIONS, [])
            .map_err(|error| sql_error(error, "liberar las reservas inactivas"))?;
        let released = tx.changes() as usize;
        tx.commit()
            .map_err(|error| sql_error(error, "confirmar la liberación"))?;
        Ok(released)
    }
}
