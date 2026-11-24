//! The conversion orchestrator: one synchronous scheduling tick and the child
//! processes it owns (`docs/architecture.md` §6-§7, `docs/contracts.md` §4, §7).
//!
//! A tick first reaps every conversion whose process ended and publishes — or
//! fails — its result (§12), then starts at most one `queued` job if
//! `cauce_core::next_startable` allows it. Child handles live in memory in
//! [`Daemon::children`], so a restart leaves the rows `interrupted`
//! (`docs/data-model.md` §4) and never resumes a conversion on its own.
//!
//! The runner exposes a blocking `wait` and a `Drop` that kills the group, but no
//! non-blocking status check. Each child's stderr callback therefore carries a
//! [`StreamEnd`] marker: when FFmpeg closes its pipes the pump thread drops it
//! and the next tick reaps instead of stalling the scheduler.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use cauce_core::command::HostFilters;
use cauce_core::{DuplicatePolicy, JobError};
use cauce_ffmpeg::runner::ConversionChild;
use cauce_ffmpeg::toolchain::{self, Toolchain};

use crate::dispatch::Daemon;

/// Cadence of the async loop in [`crate::spawn_orchestrator`].
pub const TICK_INTERVAL: Duration = Duration::from_millis(250);
/// A progress sample is persisted at most once per second per job
/// (`docs/contracts.md` §7); the runner already throttles the broadcasts to 2 Hz.
const SAVE_PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
/// Grace between `SIGTERM` and `SIGKILL` when a job is cancelled.
pub const CANCEL_GRACE: Duration = Duration::from_secs(2);
/// Reason persisted for every transition the scheduler causes.
const REASON_SCHEDULER: &str = "scheduler";

/// Everything one tick needs. The async loop builds it once, from the settings
/// and the located toolchain ([`config_for`]).
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub toolchain: Toolchain,
    /// Optional filters of the host, probed once so a plan never asks for
    /// a filter this FFmpeg cannot run (`docs/architecture-phases-0-and-1.md` §2
    /// BUG-005).
    pub host_filters: HostFilters,
    /// `max_concurrent_jobs` from the settings (`docs/architecture.md` §7).
    pub max_concurrent: u8,
    pub duplicate_policy: DuplicatePolicy,
}

/// One conversion in flight, owned in memory by the daemon.
pub struct RunningJob {
    pub child: ConversionChild,
    /// Set when the child's output pipes close, i.e. when the process exits.
    pub exited: Arc<AtomicBool>,
}

/// Builds the configuration of a running daemon from its settings.
pub fn config_for(daemon: &Daemon) -> Result<OrchestratorConfig, JobError> {
    let settings = daemon.store_mut().settings_get()?;
    let toolchain = cauce_ffmpeg::locate()?;
    let host_filters = HostFilters::from_names(&toolchain::filters(&toolchain)?);
    Ok(OrchestratorConfig {
        toolchain,
        host_filters,
        max_concurrent: settings.max_concurrent_jobs,
        duplicate_policy: settings.duplicate_policy,
    })
}

mod tick;

pub use tick::tick;
