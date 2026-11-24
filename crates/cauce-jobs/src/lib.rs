//! Cauce persistent job queue backed by SQLite in WAL mode.
//!
//! The daemon is the single writer (`docs/data-model.md` §1). This crate owns the
//! schema, the transactional migrations and the job/output-reservation writes;
//! the domain types themselves live in `cauce-core` and never see SQLite.

pub mod codec;
pub mod job_store;
pub mod migrations;
pub mod recovery;
pub mod sql;
pub mod sql_support;
pub mod store;
pub mod transitions;

pub use migrations::{MIGRATIONS, Migration, run_migrations};
pub use recovery::{Reconciliation, reconcile_temp_files};
pub use store::Store;
