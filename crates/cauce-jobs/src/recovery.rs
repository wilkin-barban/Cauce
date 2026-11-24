//! Orphan temporary file reconciliation (`docs/architecture-phases-0-and-1.md` §2 BUG-006).
//!
//! When the daemon restarts after an abrupt stop or cancellation, active jobs
//! become `interrupted`. This module purges orphan `.cauce-*.part.*` files left
//! behind by jobs in non-active terminal or interrupted states.

use cauce_core::{JobError, JobState, temp_belongs_to};

use crate::Store;

/// Results of the orphan temporary file reconciliation pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    pub removed: usize,
    pub missing: usize,
    pub failures: Vec<String>,
}

/// Reconciles orphan temporary files for jobs that are no longer active.
///
/// Only checks jobs in [`JobState::Interrupted`], [`JobState::Failed`], and
/// [`JobState::Cancelled`]. For each job with a `temp_path` verified by
/// [`temp_belongs_to`], the file is deleted. Missing files are counted under
/// `missing`; any I/O errors are recorded in `failures` without aborting the pass.
pub fn reconcile_temp_files(store: &Store) -> Result<Reconciliation, JobError> {
    let states = [JobState::Interrupted, JobState::Failed, JobState::Cancelled];
    let jobs = store.list_jobs(Some(&states))?;
    let mut reconciliation = Reconciliation::default();

    for job in jobs {
        if let Some(temp_path) = &job.temp_path {
            if temp_belongs_to(temp_path, &job.id) {
                match std::fs::remove_file(temp_path) {
                    Ok(()) => {
                        reconciliation.removed += 1;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        reconciliation.missing += 1;
                    }
                    Err(err) => {
                        reconciliation.failures.push(format!(
                            "No se pudo eliminar el archivo temporal `{temp_path}` del trabajo `{}`: {err}",
                            job.id
                        ));
                    }
                }
            }
        }
    }

    Ok(reconciliation)
}
