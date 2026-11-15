//! `Settings`: the validated key/value configuration exposed over IPC.
//!
//! See `docs/contracts.md` §2.6 and `docs/data-model.md` §2.5.

use serde::{Deserialize, Serialize};

use crate::validate;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub output_dir: String,
    /// Between 1 and 8 concurrent jobs.
    #[serde(deserialize_with = "validate::max_concurrent_jobs")]
    pub max_concurrent_jobs: u8,
    pub theme: Theme,
    pub language: Language,
    /// Desktop notifications are opt-in.
    pub notifications: bool,
    pub duplicate_policy: DuplicatePolicy,
    pub keep_temp_on_failure: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// Interface locale. The Brazilian tag keeps its exact `pt-BR` spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "pt-BR")]
    PtBr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    Suffix,
    Skip,
    Overwrite,
}
