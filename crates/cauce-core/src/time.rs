//! UTC RFC 3339 timestamps, in the exact shape the contracts use.
//!
//! The domain carries timestamps as validated text: parsing and formatting stay
//! out of `cauce-core`, which must not depend on a clock abstraction.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A UTC RFC 3339 timestamp such as `2026-11-14T10:00:00Z`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(String);

/// The input was not a UTC RFC 3339 timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTimestamp {
    input: String,
}

impl fmt::Display for InvalidTimestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`{}` is not a UTC RFC 3339 timestamp (expected `YYYY-MM-DDTHH:MM:SS[.fff]Z`)",
            self.input
        )
    }
}

impl std::error::Error for InvalidTimestamp {}

impl Timestamp {
    /// Validates `input` without accepting local offsets: Cauce stores UTC only.
    pub fn parse(input: &str) -> Result<Self, InvalidTimestamp> {
        if is_utc_rfc3339(input) {
            Ok(Self(input.to_owned()))
        } else {
            Err(InvalidTimestamp {
                input: input.to_owned(),
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn digits(bytes: &[u8], range: std::ops::Range<usize>) -> Option<u32> {
    let slice = bytes.get(range)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    slice
        .iter()
        .try_fold(0_u32, |acc, digit| Some(acc * 10 + u32::from(digit - b'0')))
}

fn is_utc_rfc3339(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.len() < 20 || bytes[10] != b'T' {
        return false;
    }

    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        digits(bytes, 0..4),
        digits(bytes, 5..7),
        digits(bytes, 8..10),
        digits(bytes, 11..13),
        digits(bytes, 14..16),
        digits(bytes, 17..19),
    ) else {
        return false;
    };

    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[13] != b':' || bytes[16] != b':' {
        return false;
    }
    if year == 0 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return false;
    }

    let mut rest = &bytes[19..];
    if let Some(fraction) = rest.strip_prefix(b".") {
        let length = fraction
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if length == 0 {
            return false;
        }
        rest = &fraction[length..];
    }

    rest == b"Z"
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

struct TimestampVisitor;

impl Visitor<'_> for TimestampVisitor {
    type Value = Timestamp;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a UTC RFC 3339 timestamp")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Timestamp::parse(value).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(TimestampVisitor)
    }
}
