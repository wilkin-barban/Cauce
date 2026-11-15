//! `ffmpeg -progress pipe:1` parser (task T10 / C-021).
//!
//! Pure parsing, no processes: `docs/contracts.md` §2.5 fixes the `Progress`
//! shape, §7 caps the emit rate and `docs/architecture.md` §6 step 9 makes this
//! the only source of real progress.

use cauce_core::Timestamp;
use cauce_ffmpeg::progress::ProgressParser;

fn now() -> Timestamp {
    Timestamp::parse("2026-11-14T10:00:00Z").expect("the fixture timestamp must be valid")
}

/// Feeds every line but the closing one; the closing line returns the sample.
fn feed_block(parser: &mut ProgressParser, lines: &[&str]) -> cauce_core::Progress {
    let (last, head) = lines.split_last().expect("a block must not be empty");
    for line in head {
        assert!(
            parser.feed_line(line, now()).is_none(),
            "`{line}` must not close the block"
        );
    }
    parser
        .feed_line(last, now())
        .expect("the closing `progress=` line must emit a sample")
}

const FULL_BLOCK: &[&str] = &[
    "frame=120",
    "fps=48.5",
    "bitrate=1234.5kbits/s",
    "total_size=987654",
    "out_time_us=2500000",
    "out_time_ms=2500000",
    "out_time=00:00:02.500000",
    "dup_frames=0",
    "drop_frames=2",
    "speed=1.02x",
    "progress=continue",
];

#[test]
fn parses_a_full_block() {
    let mut parser = ProgressParser::new(10_000);
    let progress = feed_block(&mut parser, FULL_BLOCK);

    assert!((progress.percent.expect("percent") - 25.0).abs() < 1e-9);
    assert_eq!(progress.processed_ms, 2_500);
    assert_eq!(progress.total_ms, 10_000);
    assert!((progress.fps.expect("fps") - 48.5).abs() < 1e-9);
    assert!((progress.speed.expect("speed") - 1.02).abs() < 1e-9);
    assert_eq!(progress.bitrate_bps, Some(1_234_500));
    assert_eq!(progress.updated_at, now());
}

#[test]
fn reads_out_time_us_and_falls_back_to_out_time_ms_as_microseconds() {
    let mut from_us = ProgressParser::new(10_000);
    let us = feed_block(&mut from_us, &["out_time_us=1500000", "progress=continue"]);
    assert_eq!(us.processed_ms, 1_500);

    // FFmpeg's `out_time_ms` is microseconds despite the name: it is the same
    // value as `out_time_us`, so it must be divided by 1000 as well.
    let mut from_ms = ProgressParser::new(10_000);
    let ms = feed_block(&mut from_ms, &["out_time_ms=2500000", "progress=continue"]);
    assert_eq!(ms.processed_ms, 2_500);
    assert!((ms.percent.expect("percent") - 25.0).abs() < 1e-9);
}

#[test]
fn absent_total_means_indeterminate_percent() {
    let mut parser = ProgressParser::new(0);
    let progress = feed_block(
        &mut parser,
        &["out_time_us=1000000", "speed=2.0x", "progress=continue"],
    );

    assert_eq!(progress.percent, None);
    assert_eq!(progress.eta_seconds, None);
    assert_eq!(progress.total_ms, 0);
    assert_eq!(progress.processed_ms, 1_000);
}

#[test]
fn converts_kbits_to_bits_per_second() {
    let mut parser = ProgressParser::new(1_000);
    let progress = feed_block(&mut parser, &["bitrate=1234.5kbits/s", "progress=continue"]);
    assert_eq!(progress.bitrate_bps, Some(1_234_500));
}

#[test]
fn handles_na_values_as_none() {
    let mut parser = ProgressParser::new(4_000);
    let progress = feed_block(
        &mut parser,
        &[
            "fps=N/A",
            "speed=N/A",
            "bitrate=N/A",
            "out_time_us=1000000",
            "progress=continue",
        ],
    );

    assert_eq!(progress.fps, None);
    assert_eq!(progress.speed, None);
    assert_eq!(progress.bitrate_bps, None);
    assert!((progress.percent.expect("percent") - 25.0).abs() < 1e-9);
}

#[test]
fn computes_eta_from_speed() {
    let mut parser = ProgressParser::new(10_000);
    let progress = feed_block(
        &mut parser,
        &["out_time_us=2000000", "speed=2.0x", "progress=continue"],
    );
    // 8000 ms remaining at 2.0x => 4 s.
    assert_eq!(progress.eta_seconds, Some(4));

    let mut slow = ProgressParser::new(10_000);
    let throttled = feed_block(
        &mut slow,
        &["out_time_us=2000000", "speed=0.5x", "progress=continue"],
    );
    assert_eq!(throttled.eta_seconds, Some(16));
}

#[test]
fn clamps_percent_at_one_hundred() {
    let mut parser = ProgressParser::new(1_000);
    let progress = feed_block(&mut parser, &["out_time_us=3000000", "progress=continue"]);
    assert_eq!(progress.percent, Some(100.0));

    let mut zero_speed = ProgressParser::new(1_000);
    let no_eta = feed_block(
        &mut zero_speed,
        &["out_time_us=500000", "speed=0.0x", "progress=continue"],
    );
    assert_eq!(no_eta.eta_seconds, None);
    assert_eq!(no_eta.percent, Some(50.0));
}

#[test]
fn ignores_unknown_keys() {
    let mut parser = ProgressParser::new(2_000);
    let progress = feed_block(
        &mut parser,
        &[
            "frame=1",
            "cauce_future_key=whatever",
            "another one without an equal sign",
            "out_time_us=1000000",
            "progress=continue",
        ],
    );
    assert_eq!(progress.processed_ms, 1_000);
    assert!((progress.percent.expect("percent") - 50.0).abs() < 1e-9);
}

#[test]
fn end_block_is_delivered() {
    let mut parser = ProgressParser::new(2_000);
    let progress = feed_block(
        &mut parser,
        &["out_time_us=2000000", "speed=1.0x", "progress=end"],
    );
    assert_eq!(progress.processed_ms, 2_000);
    assert_eq!(progress.percent, Some(100.0));
    assert_eq!(progress.eta_seconds, Some(0));
}
