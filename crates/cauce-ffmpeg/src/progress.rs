//! Parser for `ffmpeg -progress pipe:1`.
//!
//! FFmpeg writes a block of `key=value` lines and closes it with a
//! `progress=continue` (or `progress=end`) line; the block as a whole is one
//! real progress sample (`docs/architecture.md` §6 step 9, `docs/contracts.md`
//! §2.5). Parsing is pure — it takes the caller's timestamp — so it stays
//! unit-testable, while the runner owns throttling (`docs/contracts.md` §7).
//!
//! Every unparsable value becomes `None`: a broken measurement must never fail
//! a conversion that is otherwise fine. The bounded line reader and the UTC
//! clock at the bottom serve both FFmpeg pipes and complete each `Progress`.

use std::io::{BufRead, BufReader, Read};
use std::time::{SystemTime, UNIX_EPOCH};

use cauce_core::{Progress, Timestamp};

/// Accumulates one `-progress` block and turns it into a [`Progress`].
pub struct ProgressParser {
    /// Duration of the conversion, as reported by FFprobe. `0` means unknown.
    total_ms: u64,
    block: Block,
}

/// The keys Cauce reads from a single block. Unknown keys are ignored.
#[derive(Debug, Default, Clone, Copy)]
struct Block {
    fps: Option<f64>,
    speed: Option<f64>,
    bitrate_bps: Option<u64>,
    out_time_us: Option<u64>,
    out_time_ms: Option<u64>,
}

impl ProgressParser {
    pub fn new(total_ms: u64) -> Self {
        Self {
            total_ms,
            block: Block::default(),
        }
    }

    /// Feeds one line of `ffmpeg -progress pipe:1`. Returns `Some(Progress)` when
    /// a block closes (`progress=continue` or `progress=end`).
    pub fn feed_line(&mut self, line: &str, now: Timestamp) -> Option<Progress> {
        let (key, value) = line.trim().split_once('=')?;
        let value = value.trim();

        match key.trim() {
            // `out_time_ms` carries the same value as `out_time_us`: FFmpeg's
            // `-progress` reports microseconds under both names (a long-standing
            // naming quirk), so both are divided by 1000 to get milliseconds.
            "out_time_us" => self.block.out_time_us = value.parse().ok(),
            "out_time_ms" => self.block.out_time_ms = value.parse().ok(),
            "fps" => self.block.fps = value.parse().ok(),
            "speed" => self.block.speed = parse_speed(value),
            "bitrate" => self.block.bitrate_bps = parse_bitrate_bps(value),
            "progress" => return Some(self.close(now)),
            _ => {}
        }

        None
    }

    fn close(&mut self, now: Timestamp) -> Progress {
        let block = std::mem::take(&mut self.block);
        let processed_ms = block
            .out_time_us
            .or(block.out_time_ms)
            .map(|microseconds| microseconds / 1_000)
            .unwrap_or(0);
        let total_ms = self.total_ms;

        Progress {
            percent: percent(processed_ms, total_ms),
            processed_ms,
            total_ms,
            fps: block.fps,
            speed: block.speed,
            bitrate_bps: block.bitrate_bps,
            eta_seconds: eta_seconds(processed_ms, total_ms, block.speed),
            updated_at: now,
        }
    }
}

/// `None` while the total duration is unknown; clamped so a long block never
/// reports more than 100 % (`docs/contracts.md` §2.5).
fn percent(processed_ms: u64, total_ms: u64) -> Option<f64> {
    if total_ms == 0 {
        return None;
    }
    let raw = processed_ms as f64 / total_ms as f64 * 100.0;
    Some(raw.clamp(0.0, 100.0))
}

/// ETA from the sample itself, never from the wall clock: remaining media
/// milliseconds divided by the encoding speed (`docs/architecture.md` §17).
fn eta_seconds(processed_ms: u64, total_ms: u64, speed: Option<f64>) -> Option<u64> {
    if total_ms == 0 {
        return None;
    }
    let speed = speed.filter(|speed| speed.is_finite() && *speed > 0.0)?;
    let remaining_ms = total_ms.saturating_sub(processed_ms) as f64;
    Some((remaining_ms / speed / 1_000.0).floor() as u64)
}

/// `1.02x` becomes `1.02`; `N/A` and garbage become `None`.
fn parse_speed(value: &str) -> Option<f64> {
    let value = value.trim().trim_end_matches('x').trim();
    let speed: f64 = value.parse().ok()?;
    speed.is_finite().then_some(speed)
}

/// `1234.5kbits/s` becomes `1_234_500` bps; `N/A` and garbage become `None`.
fn parse_bitrate_bps(value: &str) -> Option<u64> {
    let value = value.trim();
    let end = value
        .find(|character: char| !(character.is_ascii_digit() || character == '.'))
        .unwrap_or(value.len());
    let (number, suffix) = value.split_at(end);
    let number: f64 = number.parse().ok()?;
    let multiplier = match suffix.chars().next() {
        Some('k' | 'K') => 1_000.0,
        Some('m' | 'M') => 1_000_000.0,
        Some('g' | 'G') => 1_000_000_000.0,
        _ => 1.0,
    };
    let bits = number * multiplier;
    if !bits.is_finite() || bits < 0.0 {
        return None;
    }
    Some(bits.round() as u64)
}

// -- Pipe plumbing -----------------------------------------------------------
//
// Both FFmpeg pipes are newline-delimited: stdout carries `-progress` blocks and
// stderr carries diagnostics. This reader is bounded — it never buffers more
// than one truncated line — so a chatty FFmpeg cannot exhaust memory, and it
// lives next to the parser because both are the "progress stream" concern.

/// A single line is truncated to this many bytes; the total is unbounded.
pub(crate) const LINE_LIMIT: usize = 4096;
/// Fallback timestamp if the system clock predates the Unix epoch.
const EPOCH: &str = "1970-01-01T00:00:00Z";

/// Reads newline-delimited text, truncating each line to [`LINE_LIMIT`] bytes.
pub(crate) fn for_each_line(stream: impl Read, mut emit: impl FnMut(&str)) {
    let mut reader = BufReader::new(stream);
    let mut line: Vec<u8> = Vec::new();
    let mut truncated = false;

    while let Ok(chunk) = reader.fill_buf() {
        if chunk.is_empty() {
            break;
        }
        let mut consumed = 0;
        while let Some(offset) = chunk[consumed..].iter().position(|byte| *byte == b'\n') {
            let end = consumed + offset;
            push_limited(&mut line, &chunk[consumed..end], &mut truncated);
            emit_line(&line, &mut emit);
            line.clear();
            truncated = false;
            consumed = end + 1;
        }
        push_limited(&mut line, &chunk[consumed..], &mut truncated);
        let length = chunk.len();
        reader.consume(length);
    }

    if !line.is_empty() || truncated {
        emit_line(&line, &mut emit);
    }
}

fn push_limited(line: &mut Vec<u8>, bytes: &[u8], truncated: &mut bool) {
    let room = LINE_LIMIT.saturating_sub(line.len());
    if bytes.len() > room {
        line.extend_from_slice(&bytes[..room]);
        *truncated = true;
    } else {
        line.extend_from_slice(bytes);
    }
}

fn emit_line(line: &[u8], emit: &mut impl FnMut(&str)) {
    let text = String::from_utf8_lossy(line);
    emit(text.trim_end_matches('\r'));
}

/// Current time as a UTC RFC 3339 timestamp, without pulling in a date library.
///
/// `Progress.updated_at` is part of the IPC contract (`docs/contracts.md` §2.5),
/// so the sample carries the instant it was produced.
pub(crate) fn utc_now() -> Timestamp {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    Timestamp::parse(&format_utc(seconds)).unwrap_or_else(|_| {
        Timestamp::parse(EPOCH).expect("the Unix epoch is a valid UTC RFC 3339 timestamp")
    })
}

fn format_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch to a date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bitrate_suffixes() {
        assert_eq!(parse_bitrate_bps("1234.5kbits/s"), Some(1_234_500));
        assert_eq!(parse_bitrate_bps("0.0kbits/s"), Some(0));
        assert_eq!(parse_bitrate_bps("128kbits/s"), Some(128_000));
        assert_eq!(parse_bitrate_bps("N/A"), None);
        assert_eq!(parse_bitrate_bps(""), None);
    }

    #[test]
    fn rejects_speeds_that_are_not_numbers() {
        assert_eq!(parse_speed("1.02x"), Some(1.02));
        assert_eq!(parse_speed("N/A"), None);
        assert_eq!(parse_speed("nanx"), None);
    }

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_utc(1_767_225_600), "2026-01-01T00:00:00Z");
    }

    #[test]
    fn truncates_long_lines_without_buffering_them() {
        let input = format!("{}\nshort\n", "x".repeat(10_000));
        let mut lines = Vec::new();
        for_each_line(input.as_bytes(), |line| lines.push(line.to_owned()));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), LINE_LIMIT);
        assert_eq!(lines[1], "short");
    }
}
