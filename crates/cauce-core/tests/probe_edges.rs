//! FFprobe parser across formats and edge cases (task T4 / C-012).
//!
//! Every assertion here pins behavior of `cauce_core::probe` that the contract
//! documents: unknown durations, missing stream bitrates, subtitle payload kinds,
//! fractional frame rates and rejected garbage.

use cauce_core::probe::parse_ffprobe;
use cauce_core::{ErrorCode, MediaSnapshot, SubtitleKind, Timestamp};

const MKV_CAPTURE: &str = include_str!("../../../tests/fixtures/ffprobe/sample-hevc.mkv.json");
const WEBM_CAPTURE: &str =
    include_str!("../../../tests/fixtures/ffprobe/sample-vp9-opus.webm.json");
const FLAC_CAPTURE: &str = include_str!("../../../tests/fixtures/ffprobe/sample-audio.flac.json");
const MP3_CAPTURE: &str = include_str!("../../../tests/fixtures/ffprobe/sample-audio.mp3.json");

fn probed_at() -> Timestamp {
    Timestamp::parse("2026-11-14T10:00:00Z").expect("fixture timestamp must be valid UTC")
}

fn parse(path: &str, source: &str) -> MediaSnapshot {
    parse_ffprobe(path, probed_at(), source).expect("real ffprobe output must parse")
}

struct Capture {
    path: &'static str,
    source: &'static str,
    container: &'static str,
    video_codec: Option<&'static str>,
    audio_codec: &'static str,
}

#[test]
fn reads_every_real_capture() {
    let captures = [
        Capture {
            path: "/home/wilkin/Vídeos/entrada.mkv",
            source: MKV_CAPTURE,
            container: "mkv",
            video_codec: Some("hevc"),
            audio_codec: "aac",
        },
        Capture {
            path: "/home/wilkin/Vídeos/entrada.webm",
            source: WEBM_CAPTURE,
            container: "webm",
            video_codec: Some("vp9"),
            audio_codec: "opus",
        },
        Capture {
            path: "/home/wilkin/Música/entrada.flac",
            source: FLAC_CAPTURE,
            container: "flac",
            video_codec: None,
            audio_codec: "flac",
        },
        Capture {
            path: "/home/wilkin/Música/entrada.mp3",
            source: MP3_CAPTURE,
            container: "mp3",
            video_codec: None,
            audio_codec: "mp3",
        },
    ];

    for capture in captures {
        let snapshot = parse(capture.path, capture.source);
        let label = capture.path;

        assert_eq!(
            snapshot.container, capture.container,
            "container of {label}"
        );
        assert!(
            (2000..=2100).contains(&snapshot.duration_ms),
            "duration of {label} must be ~2 s, got {}",
            snapshot.duration_ms
        );
        assert!(snapshot.size_bytes > 0, "size of {label} must be known");
        assert!(
            snapshot.bitrate_bps > 0,
            "format bitrate of {label} must be known"
        );

        match capture.video_codec {
            Some(codec) => {
                assert_eq!(snapshot.video_streams.len(), 1, "video streams of {label}");
                assert_eq!(snapshot.video_streams[0].codec, codec);
            }
            None => assert!(
                snapshot.video_streams.is_empty(),
                "{label} is audio only, got {:?}",
                snapshot.video_streams
            ),
        }

        assert_eq!(snapshot.audio_streams.len(), 1, "audio streams of {label}");
        assert_eq!(snapshot.audio_streams[0].codec, capture.audio_codec);
        assert!(snapshot.subtitle_streams.is_empty());
    }
}

#[test]
fn treats_missing_duration_as_zero() {
    let source = r#"{
      "streams": [],
      "format": { "format_name": "matroska,webm", "size": "1024", "bit_rate": "4096" }
    }"#;

    let snapshot = parse("/home/wilkin/Vídeos/sin-duracion.mkv", source);

    assert_eq!(snapshot.duration_ms, 0);
    assert_eq!(snapshot.container, "mkv");
    assert_eq!(snapshot.size_bytes, 1024);
    assert_eq!(snapshot.bitrate_bps, 4096);
}

#[test]
fn keeps_unknown_audio_bitrate_empty() {
    let source = r#"{
      "streams": [
        { "index": 0, "codec_name": "flac", "codec_type": "audio", "channels": 2, "sample_rate": "44100" }
      ],
      "format": { "format_name": "flac", "duration": "3.000000" }
    }"#;

    let snapshot = parse("/home/wilkin/Música/variable.flac", source);

    assert_eq!(snapshot.audio_streams.len(), 1);
    assert_eq!(snapshot.audio_streams[0].bitrate_bps, None);
    assert_eq!(snapshot.duration_ms, 3_000);
}

#[test]
fn classifies_text_and_bitmap_subtitles() {
    let source = r#"{
      "streams": [
        {
          "index": 2,
          "codec_name": "subrip",
          "codec_type": "subtitle",
          "disposition": { "default": 1, "forced": 0 },
          "tags": { "language": "eng" }
        },
        {
          "index": 3,
          "codec_name": "hdmv_pgs_subtitle",
          "codec_type": "subtitle",
          "disposition": { "default": 0, "forced": 1 },
          "tags": { "language": "spa" }
        }
      ],
      "format": { "format_name": "matroska,webm", "duration": "1.500000" }
    }"#;

    let snapshot = parse("/home/wilkin/Vídeos/subtitulos.mkv", source);

    assert_eq!(snapshot.subtitle_streams.len(), 2);

    let text = &snapshot.subtitle_streams[0];
    assert_eq!(text.kind, SubtitleKind::Text);
    assert_eq!(text.codec, "subrip");
    assert_eq!(text.language.as_deref(), Some("eng"));
    assert!(text.is_default);
    assert!(!text.is_forced);

    let bitmap = &snapshot.subtitle_streams[1];
    assert_eq!(bitmap.kind, SubtitleKind::Bitmap);
    assert_eq!(bitmap.codec, "hdmv_pgs_subtitle");
    assert_eq!(bitmap.language.as_deref(), Some("spa"));
    assert!(!bitmap.is_default);
    assert!(bitmap.is_forced);
}

#[test]
fn parses_fractional_frame_rates() {
    let source = r#"{
      "streams": [
        {
          "index": 0,
          "codec_name": "h264",
          "codec_type": "video",
          "width": 1920,
          "height": 1080,
          "r_frame_rate": "30000/1001",
          "avg_frame_rate": "30000/1001",
          "pix_fmt": "yuv420p10le",
          "color_transfer": "smpte2084"
        }
      ],
      "format": { "format_name": "matroska,webm", "duration": "4.000000" }
    }"#;

    let snapshot = parse("/home/wilkin/Vídeos/ntsc.mkv", source);
    let video = &snapshot.video_streams[0];

    assert!(
        (29.96..=29.98).contains(&video.fps),
        "fps should be ~29.97, got {}",
        video.fps
    );
    assert_eq!(video.bit_depth, 10, "yuv420p10le is 10 bit");
    assert!(video.is_hdr, "smpte2084 is HDR");
}

#[test]
fn ignores_unknown_stream_types() {
    let source = r#"{
      "streams": [
        {
          "index": 0,
          "codec_name": "h264",
          "codec_type": "video",
          "width": 640,
          "height": 360,
          "r_frame_rate": "24/1",
          "pix_fmt": "yuv420p"
        },
        { "index": 1, "codec_name": "bin_data", "codec_type": "data" },
        { "index": 2, "codec_name": "ttf", "codec_type": "attachment" }
      ],
      "format": { "format_name": "matroska,webm", "duration": "2.000000" }
    }"#;

    let snapshot = parse("/home/wilkin/Vídeos/mixto.mkv", source);

    assert_eq!(snapshot.video_streams.len(), 1);
    assert_eq!(snapshot.video_streams[0].codec, "h264");
    assert!(snapshot.audio_streams.is_empty());
    assert!(snapshot.subtitle_streams.is_empty());
}

#[test]
fn rejects_malformed_probe_output() {
    let cases = ["not json", "{}", r#"{"streams":[]}"#];

    for source in cases {
        let error = parse_ffprobe("/home/wilkin/Vídeos/roto.mp4", probed_at(), source)
            .expect_err("malformed probe output must be rejected");
        assert_eq!(
            error.code,
            ErrorCode::ProbeFailed,
            "input {source:?} must be PROBE_FAILED"
        );
        assert!(!error.retryable, "a malformed probe is not retryable");
        assert!(!error.message.is_empty(), "the error needs a human message");
    }
}
