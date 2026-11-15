//! Pure FFprobe JSON parser: `ffprobe -show_streams -show_format -print_format json`
//! in, [`MediaSnapshot`] out (`docs/contracts.md` §2.1).
//!
//! This module never spawns a process and never touches the filesystem. Running
//! `ffprobe` belongs to `cauce-ffmpeg`; keeping the translation pure makes the
//! contract testable against real captures stored as JSON.
//!
//! The parser is defensive on purpose: FFprobe prints free-form strings
//! (`"N/A"`, `"0/0"`, numbers as text) and any missing or unparseable value
//! degrades to the documented empty value instead of panicking.

use serde_json::Value;

use crate::error::{ErrorCode, JobError};
use crate::media::{AudioStream, MediaSnapshot, SubtitleKind, SubtitleStream, VideoStream};
use crate::time::Timestamp;

/// Codecs whose subtitle payload is a bitmap.
const BITMAP_SUBTITLES: [&str; 4] = ["dvd_subtitle", "hdmv_pgs_subtitle", "dvb_subtitle", "xsub"];

/// Transfer characteristics that mean high dynamic range.
const HDR_TRANSFERS: [&str; 2] = ["smpte2084", "arib-std-b67"];

/// Extensions Cauce normalizes, mapped to the container name it reports.
const CONTAINERS: [(&str, &str); 10] = [
    ("mp4", "mp4"),
    ("m4v", "mp4"),
    ("mov", "mov"),
    ("mkv", "mkv"),
    ("webm", "webm"),
    ("mp3", "mp3"),
    ("flac", "flac"),
    ("wav", "wav"),
    ("ogg", "ogg"),
    ("m4a", "m4a"),
];

/// Translates FFprobe JSON into the domain snapshot.
///
/// `path` is kept verbatim and `probed_at` is supplied by the caller, so the
/// parser stays free of clocks and path resolution. Output is not valid media
/// when the document is not JSON, has no top-level `format`, or has no
/// `streams` array.
pub fn parse_ffprobe(
    path: &str,
    probed_at: Timestamp,
    source: &str,
) -> Result<MediaSnapshot, JobError> {
    let document: Value = serde_json::from_str(source).map_err(|_| malformed())?;

    let format = document
        .get("format")
        .filter(|value| value.is_object())
        .ok_or_else(malformed)?;
    let streams = document
        .get("streams")
        .and_then(Value::as_array)
        .ok_or_else(malformed)?;

    let mut snapshot = MediaSnapshot {
        path: path.to_owned(),
        container: container_of(path, format.get("format_name")),
        duration_ms: seconds_to_ms(format.get("duration")),
        size_bytes: integer(format.get("size")),
        bitrate_bps: integer(format.get("bit_rate")),
        video_streams: Vec::new(),
        audio_streams: Vec::new(),
        subtitle_streams: Vec::new(),
        probed_at,
    };

    for stream in streams {
        match text(stream.get("codec_type")).as_deref() {
            Some("video") => snapshot.video_streams.push(video_stream(stream)),
            Some("audio") => snapshot.audio_streams.push(audio_stream(stream)),
            Some("subtitle") => snapshot.subtitle_streams.push(subtitle_stream(stream)),
            _ => {}
        }
    }

    Ok(snapshot)
}

fn malformed() -> JobError {
    JobError::new(
        ErrorCode::ProbeFailed,
        "No se pudo interpretar la salida de FFprobe: el archivo no parece multimedia válido o está dañado.",
        false,
    )
}

/// Reads a JSON value that FFprobe prints as text, accepting raw numbers too.
fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(raw) => raw
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite()),
        _ => None,
    }
}

/// Non-negative integer field, `0` when absent or unparseable.
fn integer(value: Option<&Value>) -> u64 {
    match number(value) {
        Some(parsed) if parsed >= 0.0 => parsed as u64,
        _ => 0,
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|raw| !raw.is_empty() && *raw != "N/A")
        .map(str::to_owned)
}

/// `format.duration` is expressed in seconds; the contract uses milliseconds.
fn seconds_to_ms(value: Option<&Value>) -> u64 {
    match number(value) {
        Some(seconds) if seconds >= 0.0 => (seconds * 1000.0).round() as u64,
        _ => 0,
    }
}

/// Container name: the extension wins, `format_name` is the fallback.
fn container_of(path: &str, format_name: Option<&Value>) -> String {
    if let Some((_, extension)) = path.rsplit_once('.') {
        if let Some((_, container)) = CONTAINERS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(extension))
        {
            return (*container).to_owned();
        }
    }

    text(format_name)
        .and_then(|name| name.split(',').next().map(str::trim).map(str::to_owned))
        .unwrap_or_default()
}

/// `num/den` rational, `0.0` when missing or invalid.
fn rational(value: Option<&Value>) -> Option<f64> {
    let raw = text(value)?;
    let (numerator, denominator) = raw.split_once('/')?;
    let numerator = numerator.trim().parse::<f64>().ok()?;
    let denominator = denominator.trim().parse::<f64>().ok()?;
    if denominator <= 0.0 || !numerator.is_finite() {
        return None;
    }
    let parsed = numerator / denominator;
    (parsed.is_finite() && parsed >= 0.0).then_some(parsed)
}

/// `r_frame_rate` first, `avg_frame_rate` as the fallback.
fn frame_rate(stream: &Value) -> f64 {
    rational(stream.get("r_frame_rate"))
        .or_else(|| rational(stream.get("avg_frame_rate")))
        .unwrap_or(0.0)
}

/// `bits_per_raw_sample` when known, else inferred from `pix_fmt`.
fn bit_depth(stream: &Value) -> u8 {
    let reported = integer(stream.get("bits_per_raw_sample"));
    if reported > 0 {
        return u8::try_from(reported).unwrap_or(u8::MAX);
    }

    let pixel_format = text(stream.get("pix_fmt")).unwrap_or_default();
    if ["p10", "10le", "10be"]
        .iter()
        .any(|marker| pixel_format.contains(marker))
    {
        10
    } else if ["p12", "12le", "12be"]
        .iter()
        .any(|marker| pixel_format.contains(marker))
    {
        12
    } else {
        8
    }
}

/// Subtitle languages are `und` when FFprobe cannot detect them.
fn language(stream: &Value) -> Option<String> {
    let language = text(stream.get("tags").and_then(|tags| tags.get("language")))?;
    (language != "und").then_some(language)
}

/// Stream bitrate, `None` when FFprobe does not know it (`0` included).
fn stream_bitrate(stream: &Value) -> Option<u64> {
    stream
        .get("bit_rate")
        .map(|value| integer(Some(value)))
        .filter(|bitrate| *bitrate > 0)
}

/// `disposition.<flag>` is `1` when set.
fn disposition(stream: &Value, flag: &str) -> bool {
    integer(
        stream
            .get("disposition")
            .and_then(|disposition| disposition.get(flag)),
    ) == 1
}

fn video_stream(stream: &Value) -> VideoStream {
    VideoStream {
        index: integer(stream.get("index")) as u32,
        codec: text(stream.get("codec_name")).unwrap_or_default(),
        profile: text(stream.get("profile")),
        level: number(stream.get("level")).map(|level| level as i32),
        width: integer(stream.get("width")) as u32,
        height: integer(stream.get("height")) as u32,
        fps: frame_rate(stream),
        pixel_format: text(stream.get("pix_fmt")).unwrap_or_default(),
        bit_depth: bit_depth(stream),
        is_hdr: text(stream.get("color_transfer"))
            .is_some_and(|transfer| HDR_TRANSFERS.contains(&transfer.as_str())),
        bitrate_bps: stream_bitrate(stream),
        language: language(stream),
    }
}

fn audio_stream(stream: &Value) -> AudioStream {
    AudioStream {
        index: integer(stream.get("index")) as u32,
        codec: text(stream.get("codec_name")).unwrap_or_default(),
        channels: integer(stream.get("channels")) as u8,
        sample_rate: integer(stream.get("sample_rate")) as u32,
        bitrate_bps: stream_bitrate(stream),
        language: language(stream),
        is_default: disposition(stream, "default"),
    }
}

fn subtitle_stream(stream: &Value) -> SubtitleStream {
    let codec = text(stream.get("codec_name")).unwrap_or_default();
    let kind = if BITMAP_SUBTITLES.contains(&codec.as_str()) {
        SubtitleKind::Bitmap
    } else {
        SubtitleKind::Text
    };

    SubtitleStream {
        index: integer(stream.get("index")) as u32,
        codec,
        language: language(stream),
        kind,
        is_default: disposition(stream, "default"),
        is_forced: disposition(stream, "forced"),
    }
}
