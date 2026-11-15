//! FFprobe parser contract: `ffprobe -show_streams -show_format` JSON in,
//! `MediaSnapshot` out (`docs/contracts.md` §2.1, task T4 / C-012).
//!
//! The parser is pure and never spawns a process, so the real captures taken
//! from the T3 fixtures are committed as JSON and embedded here.

use cauce_core::probe::parse_ffprobe;
use cauce_core::{MediaSnapshot, Timestamp};

const MP4_CAPTURE: &str = include_str!("../../../tests/fixtures/ffprobe/sample-h264-aac.mp4.json");

pub(crate) fn probed_at() -> Timestamp {
    Timestamp::parse("2026-11-14T10:00:00Z").expect("fixture timestamp must be valid UTC")
}

pub(crate) fn parse(path: &str, source: &str) -> MediaSnapshot {
    parse_ffprobe(path, probed_at(), source).expect("real ffprobe output must parse")
}

#[test]
fn parses_the_real_mp4_capture() {
    let path = "/home/wilkin/Vídeos/entrada.mp4";
    let snapshot = parse(path, MP4_CAPTURE);

    assert_eq!(snapshot.path, path);
    assert_eq!(snapshot.probed_at, probed_at());
    assert_eq!(snapshot.container, "mp4");
    assert!(
        (1990..=2010).contains(&snapshot.duration_ms),
        "duration_ms should be ~2000, got {}",
        snapshot.duration_ms
    );
    assert!(snapshot.size_bytes > 0, "size_bytes must be known");
    assert!(snapshot.bitrate_bps > 0, "bitrate_bps must be known");

    assert_eq!(snapshot.video_streams.len(), 1, "one video stream");
    let video = &snapshot.video_streams[0];
    assert_eq!(video.index, 0);
    assert_eq!(video.codec, "h264");
    assert_eq!((video.width, video.height), (320, 240));
    assert!(
        (video.fps - 25.0).abs() < 0.001,
        "fps should be 25.0, got {}",
        video.fps
    );
    assert_eq!(video.pixel_format, "yuv420p");
    assert_eq!(video.bit_depth, 8);
    assert!(!video.is_hdr, "yuv420p H.264 is not HDR");
    assert_eq!(video.language, None, "`und` is not a language");

    assert_eq!(snapshot.audio_streams.len(), 1, "one audio stream");
    let audio = &snapshot.audio_streams[0];
    assert_eq!(audio.index, 1);
    assert_eq!(audio.codec, "aac");
    // Real capture of tests/fixtures/generate.sh: mono sine.
    assert_eq!(audio.channels, 1);
    assert_eq!(audio.sample_rate, 48_000);
    assert_eq!(audio.language, None);

    assert!(snapshot.subtitle_streams.is_empty());
}
