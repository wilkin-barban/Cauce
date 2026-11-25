//! Field-level validation used while deserializing persisted or IPC payloads.
//!
//! `docs/contracts.md` §2 constrains several numeric ranges; rejecting them at
//! the edge keeps invalid data out of SQLite and out of the job queue.

use serde::Deserialize;
use serde::de::{self, Deserializer};

pub(crate) fn percent<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<f64>::deserialize(deserializer)?;
    match value {
        Some(percent) if !(0.0..=100.0).contains(&percent) => Err(de::Error::custom(format!(
            "percent must be within 0..=100, got {percent}"
        ))),
        _ => Ok(value),
    }
}

pub(crate) fn priority<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = i32::deserialize(deserializer)?;
    if (-1000..=1000).contains(&value) {
        Ok(value)
    } else {
        Err(de::Error::custom(format!(
            "priority must be within -1000..=1000, got {value}"
        )))
    }
}

pub(crate) fn max_concurrent_jobs<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u8::deserialize(deserializer)?;
    if (1..=8).contains(&value) {
        Ok(value)
    } else {
        Err(de::Error::custom(format!(
            "max_concurrent_jobs must be within 1..=8, got {value}"
        )))
    }
}

pub(crate) fn optional_positive_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<u32>::deserialize(deserializer)?;
    match value {
        Some(0) => Err(de::Error::custom("a bitrate must be greater than zero")),
        other => Ok(other),
    }
}

pub(crate) fn crf<'de, D>(deserializer: D) -> Result<Option<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<u8>::deserialize(deserializer)?;
    match value {
        Some(crf) if !crate::preset::CRF_RANGE.contains(&crf) => Err(de::Error::custom(format!(
            "crf must be within {}..={}, got {crf}",
            crate::preset::CRF_RANGE.start(),
            crate::preset::CRF_RANGE.end()
        ))),
        other => Ok(other),
    }
}
