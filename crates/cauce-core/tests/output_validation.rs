//! Output validation contracts (task T8 / C-016).
//!
//! `validate_output` judges the probe of the temporary file against the plan and
//! the source snapshot before anything is published (`docs/architecture.md` §12,
//! `docs/contracts.md` §3). Plans and snapshots are built literally so the
//! validator is tested in isolation from `plan_conversion` and `parse_ffprobe`.

use cauce_core::{
    AudioPlan, AudioStream, ConversionPlan, ErrorCode, HdrAction, JobError, MediaSnapshot,
    MetadataPolicy, StreamAction, SubtitleAction, SubtitleKind, SubtitlePlan, SubtitleStream,
    Timestamp, VideoPlan, VideoStream, validate_output,
};

const SOURCE: &str = "/home/wilkin/Vídeos/holiday.mp4";
const TEMP: &str = "/home/wilkin/Vídeos/convertidos/.cauce-7f3a.part.mp4";
const PROBED_AT: &str = "2026-11-14T10:00:00Z";

#[rustfmt::skip]
fn video_plan(action: StreamAction, encoder: Option<&str>, max: Option<(u32, u32)>) -> VideoPlan {
    VideoPlan {
        action, encoder: encoder.map(Into::into), quality: Some(23),
        max_width: max.map(|size| size.0), max_height: max.map(|size| size.1),
        fps: None, pixel_format: None, profile: None, level: None,
        hdr_action: HdrAction::Preserve, stream_index: Some(0),
    }
}

#[rustfmt::skip]
fn audio_plan(action: StreamAction, encoder: Option<&str>) -> AudioPlan {
    AudioPlan {
        action, encoder: encoder.map(Into::into), bitrate: Some("192k".into()),
        channels: None, stream_index: Some(1), normalize: false,
        filter: None,
    }
}

#[rustfmt::skip]
fn subtitle_plan(action: SubtitleAction) -> SubtitlePlan {
    SubtitlePlan { action, encoder: None, stream_index: None, source_index: None }
}

fn plan(video: VideoPlan, audio: AudioPlan, subtitles: SubtitlePlan) -> ConversionPlan {
    ConversionPlan {
        container: "mp4".into(),
        video,
        audio,
        subtitles,
        faststart: true,
        metadata: MetadataPolicy::Preserve,
        reasoning: vec!["fixture plan built by hand".into()],
    }
}

#[rustfmt::skip]
fn video_stream(index: u32, codec: &str, width: u32, height: u32) -> VideoStream {
    VideoStream {
        index, codec: codec.into(), profile: Some("High".into()), level: Some(41),
        width, height, fps: 30.0, pixel_format: "yuv420p".into(), bit_depth: 8,
        is_hdr: false, bitrate_bps: Some(4_000_000), language: None,
    }
}

#[rustfmt::skip]
fn audio_stream(index: u32, codec: &str) -> AudioStream {
    AudioStream {
        index, codec: codec.into(), channels: 2, sample_rate: 48_000,
        bitrate_bps: Some(192_000), language: Some("spa".into()), is_default: true,
    }
}

#[rustfmt::skip]
fn subtitle_stream(index: u32, codec: &str) -> SubtitleStream {
    SubtitleStream {
        index, codec: codec.into(), language: Some("spa".into()),
        kind: SubtitleKind::Text, is_default: true, is_forced: false,
    }
}

fn snapshot(
    path: &str,
    duration_ms: u64,
    size_bytes: u64,
    video: Vec<VideoStream>,
    audio: Vec<AudioStream>,
    subtitles: Vec<SubtitleStream>,
) -> MediaSnapshot {
    MediaSnapshot {
        path: path.into(),
        container: "mp4".into(),
        duration_ms,
        size_bytes,
        bitrate_bps: 5_000_000,
        video_streams: video,
        audio_streams: audio,
        subtitle_streams: subtitles,
        probed_at: Timestamp::parse(PROBED_AT).expect("the fixture timestamp is UTC"),
    }
}

fn source() -> MediaSnapshot {
    snapshot(
        SOURCE,
        60_000,
        42_000_000,
        vec![video_stream(0, "h264", 1920, 1080)],
        vec![audio_stream(1, "aac")],
        vec![subtitle_stream(2, "mov_text")],
    )
}

fn valid_transcode() -> ConversionPlan {
    plan(
        video_plan(StreamAction::Transcode, Some("libx264"), Some((1280, 720))),
        audio_plan(StreamAction::Transcode, Some("aac")),
        subtitle_plan(SubtitleAction::Remove),
    )
}

fn valid_output() -> MediaSnapshot {
    snapshot(
        TEMP,
        60_000,
        21_000_000,
        vec![video_stream(0, "h264", 1280, 720)],
        vec![audio_stream(1, "aac")],
        vec![],
    )
}

fn assert_rule(error: &JobError, needle: &str) {
    assert_eq!(error.code, ErrorCode::OutputValidationFailed, "{error:?}");
    assert!(
        error.retryable,
        "a validation failure is retryable: {error:?}"
    );
    assert!(
        error.message.contains(needle),
        "the message names the rule `{needle}`: {}",
        error.message
    );
}

#[test]
fn accepts_a_valid_transcode() {
    validate_output(&valid_transcode(), &source(), &valid_output())
        .expect("a matching output must validate");
}

#[test]
fn rejects_a_duration_outside_tolerance() {
    let short = snapshot(
        TEMP,
        65_000,
        21_000_000,
        vec![video_stream(0, "h264", 1280, 720)],
        vec![audio_stream(1, "aac")],
        vec![],
    );
    let error = validate_output(&valid_transcode(), &source(), &short)
        .expect_err("5 s over a 60 s source is outside the 2 s / 3 % tolerance");
    assert_rule(&error, "duración");

    let inside = snapshot(
        TEMP,
        61_000,
        21_000_000,
        vec![video_stream(0, "h264", 1280, 720)],
        vec![audio_stream(1, "aac")],
        vec![],
    );
    validate_output(&valid_transcode(), &source(), &inside)
        .expect("1 s over a 60 s source is inside the tolerance");
}

#[test]
fn skips_the_duration_rule_when_the_source_has_none() {
    let mut unknown_source = source();
    unknown_source.duration_ms = 0;

    let mut odd_output = valid_output();
    odd_output.duration_ms = 1;

    validate_output(&valid_transcode(), &unknown_source, &odd_output)
        .expect("an unknown source duration cannot be compared");
}

#[test]
fn rejects_a_missing_declared_stream() {
    let mut output = valid_output();
    output.video_streams.clear();

    let error = validate_output(&valid_transcode(), &source(), &output)
        .expect_err("the plan declares video but the temporary has none");
    assert_rule(&error, "video");
}

#[test]
fn rejects_a_stream_that_should_have_been_removed() {
    let mut output = valid_output();
    output.subtitle_streams = vec![subtitle_stream(2, "mov_text")];

    let error = validate_output(&valid_transcode(), &source(), &output)
        .expect_err("the plan removes subtitles but the temporary still carries one");
    assert_rule(&error, "subtítulo");

    let mut audio_removed = valid_transcode();
    audio_removed.audio = audio_plan(StreamAction::Remove, None);
    let error = validate_output(&audio_removed, &source(), &valid_output())
        .expect_err("the plan removes audio but the temporary still carries it");
    assert_rule(&error, "audio");
}

#[test]
fn rejects_a_codec_that_does_not_match_the_encoder() {
    let mut wrong = valid_transcode();
    wrong.video = video_plan(StreamAction::Transcode, Some("libx265"), Some((1280, 720)));

    let error = validate_output(&wrong, &source(), &valid_output())
        .expect_err("libx265 cannot produce an h264 output");
    assert_rule(&error, "hevc");

    let mut wrong_audio = valid_transcode();
    wrong_audio.audio = audio_plan(StreamAction::Transcode, Some("libopus"));
    let error = validate_output(&wrong_audio, &source(), &valid_output())
        .expect_err("libopus cannot produce an aac output");
    assert_rule(&error, "opus");
}

#[test]
fn rejects_copied_codec_mismatch() {
    let mut copied = valid_transcode();
    copied.video = video_plan(StreamAction::Copy, None, Some((1280, 720)));
    let mut hevc_output = valid_output();
    hevc_output.video_streams = vec![video_stream(0, "hevc", 1280, 720)];

    let error = validate_output(&copied, &source(), &hevc_output)
        .expect_err("a copy cannot change the h264 source codec");
    assert_rule(&error, "h264");

    let mut copied_audio = valid_transcode();
    copied_audio.audio = audio_plan(StreamAction::Copy, None);
    let mut flac_output = valid_output();
    flac_output.audio_streams = vec![audio_stream(1, "flac")];
    let error = validate_output(&copied_audio, &source(), &flac_output)
        .expect_err("a copied audio stream keeps the aac source codec");
    assert_rule(&error, "aac");
}

#[test]
fn rejects_an_oversized_resolution() {
    let mut output = valid_output();
    output.video_streams = vec![video_stream(0, "h264", 1920, 1080)];

    let error = validate_output(&valid_transcode(), &source(), &output)
        .expect_err("1920 wide exceeds the 1280 limit of the plan");
    assert_rule(&error, "resolución");

    let mut oversized = valid_output();
    oversized.video_streams = vec![video_stream(0, "h264", 1280, 800)];
    let error = validate_output(&valid_transcode(), &source(), &oversized)
        .expect_err("800 high exceeds the 720 limit of the plan");
    assert_rule(&error, "resolución");
}

#[test]
fn rejects_an_empty_output() {
    let mut output = valid_output();
    output.size_bytes = 0;

    let error = validate_output(&valid_transcode(), &source(), &output)
        .expect_err("an empty temporary is never published");
    assert_rule(&error, "vacío");
    assert_eq!(
        source().size_bytes,
        42_000_000,
        "the source is never mutated"
    );
}
