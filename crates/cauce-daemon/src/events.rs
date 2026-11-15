//! Event broadcaster and in-memory log ring (`docs/contracts.md` §7).
//!
//! One `tokio::sync::broadcast` channel carries every event; each open
//! connection forwards whatever it receives as a JSON-RPC notification, so v0.1
//! needs no subscription handshake. Mutations broadcast `job.updated` with the
//! full `Job`; the queue broadcasts `queue.updated`; the daemon announces
//! `daemon.status` once on start.
//!
//! The log ring is deliberately in memory: file logging, rotation and the
//! `log.entry` stream are T14 (`docs/architecture.md` §14).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use cauce_core::{Job, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::protocol;

/// Entries kept when a client does not ask for a smaller window.
pub const DEFAULT_LOG_CAPACITY: usize = 200;
/// Events buffered before a slow connection starts lagging.
const CHANNEL_CAPACITY: usize = 256;
const EPOCH: &str = "1970-01-01T00:00:00Z";

/// One line of the diagnostic log (`logs.list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub origin: String,
    pub message: String,
    pub created_at: Timestamp,
}

/// One event, ready to be serialized as a notification.
#[derive(Debug, Clone)]
pub struct Event {
    pub method: String,
    pub params: Value,
}

impl Event {
    /// `{"jsonrpc":"2.0","method":…,"params":…}`
    pub fn to_json(&self) -> Value {
        protocol::notification(&self.method, self.params.clone())
    }
}

/// The single fan-out point of the daemon.
#[derive(Debug)]
pub struct Events {
    sender: broadcast::Sender<Event>,
    queue_paused: AtomicBool,
    logs: Mutex<VecDeque<LogEntry>>,
}

impl Events {
    pub fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender,
            queue_paused: AtomicBool::new(false),
            logs: Mutex::new(VecDeque::with_capacity(DEFAULT_LOG_CAPACITY)),
        }
    }

    /// A fresh receiver; each connection owns one for its whole lifetime.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub fn queue_paused(&self) -> bool {
        self.queue_paused.load(Ordering::SeqCst)
    }

    /// Flips the in-memory queue flag and reports the new value.
    ///
    /// The flag survives connections but not a daemon restart: v0.1 keeps it in
    /// memory and `docs/data-model.md` has no column for it yet.
    pub fn set_queue_paused(&self, paused: bool) -> bool {
        self.queue_paused.store(paused, Ordering::SeqCst);
        paused
    }

    /// Sends to every open connection; no receivers is not an error.
    pub fn broadcast(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    /// `job.updated` carries the whole job (`docs/contracts.md` §7).
    pub fn job_updated(&self, job: &Job) {
        let params = serde_json::to_value(job).unwrap_or(Value::Null);
        self.broadcast(Event {
            method: "job.updated".to_owned(),
            params,
        });
    }

    /// `queue.updated` carries the paused flag and the depth of the queue.
    pub fn queue_updated(&self, paused: bool, running: usize, queued: usize) {
        self.broadcast(Event {
            method: "queue.updated".to_owned(),
            params: json!({"paused": paused, "running": running, "queued": queued}),
        });
    }

    pub fn daemon_status(&self, state: &str, version: &str) {
        self.broadcast(Event {
            method: "daemon.status".to_owned(),
            params: json!({"state": state, "version": version}),
        });
    }

    /// Appends to the ring buffer and drops the oldest entry past the capacity.
    pub fn log(&self, level: &str, origin: &str, message: impl Into<String>) {
        let entry = LogEntry {
            level: level.to_owned(),
            origin: origin.to_owned(),
            message: message.into(),
            created_at: now(),
        };
        let mut logs = self
            .logs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if logs.len() == DEFAULT_LOG_CAPACITY {
            logs.pop_front();
        }
        logs.push_back(entry);
    }

    /// The last `limit` entries, oldest first.
    pub fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        let logs = self
            .logs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let skip = logs.len().saturating_sub(limit);
        logs.iter().skip(skip).cloned().collect()
    }
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}

fn now() -> Timestamp {
    cauce_jobs::migrations::utc_now().unwrap_or_else(|_| {
        Timestamp::parse(EPOCH).expect("the epoch stamp is a valid UTC RFC 3339 timestamp")
    })
}
