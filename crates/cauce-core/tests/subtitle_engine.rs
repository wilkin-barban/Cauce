//! Subtitle engine (phase 1, T5 / `docs/architecture-phases-0-and-1.md` §3.3).
//!
//! The matrix has three axes — source payload, destination container and the
//! mode the user asked for — and every cell either produces a track or explains
//! why it produced none. A silent drop is the defect phase 0 fixed; a plan that
//! aborts the whole conversion is the one before that.

use cauce_core::compat::resolve_subtitles;
use cauce_core::{
    ErrorCode, Preset, SubtitleAction, SubtitleKind, SubtitleMode, SubtitleStream, system_presets,
};

fn preset(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("`{id}` must exist in the catalog"))
}

#[rustfmt::skip]
fn track(index: u32, codec: &str, kind: SubtitleKind, language: &str) -> SubtitleStream {
    SubtitleStream {
        index, codec: codec.into(), language: Some(language.into()),
        kind, is_default: true, is_forced: false,
    }
}

fn subrip() -> SubtitleStream {
    track(7, "subrip", SubtitleKind::Text, "spa")
}

fn pgs() -> SubtitleStream {
    track(8, "hdmv_pgs_subtitle", SubtitleKind::Bitmap, "spa")
}

fn reasons() -> Vec<String> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// Strip
// ---------------------------------------------------------------------------

#[test]
fn strip_mode_returns_no_track_and_says_so() {
    let mut reasoning = reasons();

    let tracks = resolve_subtitles(
        preset("system-smarttv-hevc"),
        Some(&subrip()),
        SubtitleMode::Strip,
        &mut reasoning,
    )
    .expect("strip never fails");

    assert!(tracks.is_empty());
    assert!(
        reasoning.iter().any(|line| line.contains("strip")),
        "{reasoning:?}"
    );
}

#[test]
fn a_preset_that_forbids_subtitles_wins_over_the_selection() {
    let mut reasoning = reasons();

    // `system-fast-compress` declares `subtitle_policy: remove`.
    let tracks = resolve_subtitles(
        preset("system-fast-compress"),
        Some(&subrip()),
        SubtitleMode::SoftEmbed,
        &mut reasoning,
    )
    .expect("a preset policy is not an error");

    assert!(
        tracks.is_empty(),
        "the preset decides, the selection only chooses which track"
    );
    assert!(
        reasoning.iter().any(|line| line.contains("remove")),
        "{reasoning:?}"
    );
}

// ---------------------------------------------------------------------------
// SoftEmbed (S14)
// ---------------------------------------------------------------------------

#[test]
fn soft_embed_copies_a_subrip_into_mkv() {
    let mut reasoning = reasons();

    let tracks = resolve_subtitles(
        preset("system-smarttv-hevc"),
        Some(&subrip()),
        SubtitleMode::SoftEmbed,
        &mut reasoning,
    )
    .expect("mkv carries subrip");

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].action, SubtitleAction::Copy);
    assert_eq!(tracks[0].source_index, 7);
    assert_eq!(tracks[0].output_index, Some(0));
    assert_eq!(tracks[0].encoder, None, "a copy names no encoder");
    assert_eq!(tracks[0].language.as_deref(), Some("spa"));
}

#[test]
fn soft_embed_transcodes_a_subrip_into_mp4() {
    let mut reasoning = reasons();

    let tracks = resolve_subtitles(
        preset("system-smarttv-compat"),
        Some(&subrip()),
        SubtitleMode::SoftEmbed,
        &mut reasoning,
    )
    .expect("mp4 carries mov_text");

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].action, SubtitleAction::Transcode);
    assert_eq!(tracks[0].encoder.as_deref(), Some("mov_text"));
    assert_eq!(tracks[0].output_index, Some(0));
}

#[test]
fn soft_embed_degrades_a_bitmap_track_into_mp4_with_a_reason() {
    let mut reasoning = reasons();

    let tracks = resolve_subtitles(
        preset("system-smarttv-compat"),
        Some(&pgs()),
        SubtitleMode::SoftEmbed,
        &mut reasoning,
    )
    .expect("an impossible soft embed is not an error");

    assert!(
        tracks.is_empty(),
        "PGS cannot be a soft track in MP4 without OCR"
    );
    let line = reasoning
        .iter()
        .find(|line| line.contains("hdmv_pgs_subtitle"))
        .unwrap_or_else(|| panic!("the drop must name the codec: {reasoning:?}"));
    assert!(line.contains("mp4"), "and the container: {line}");
    assert!(
        line.to_lowercase().contains("burn") || line.contains("incrust"),
        "and the way out: {line}"
    );
}

// ---------------------------------------------------------------------------
// BurnIn (S15, S17)
// ---------------------------------------------------------------------------

#[test]
fn burn_in_marks_a_text_track_with_the_subtitles_filter() {
    let mut reasoning = reasons();

    let tracks = resolve_subtitles(
        preset("system-smarttv-compat"),
        Some(&subrip()),
        SubtitleMode::BurnIn,
        &mut reasoning,
    )
    .expect("text burn-in is supported");

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].action, SubtitleAction::BurnIn);
    assert_eq!(tracks[0].encoder.as_deref(), Some("subtitles"));
    assert_eq!(
        tracks[0].output_index, None,
        "a burned-in track occupies no stream"
    );
}

#[test]
fn burn_in_marks_a_bitmap_track_with_the_overlay_composite() {
    let mut reasoning = reasons();

    let tracks = resolve_subtitles(
        preset("system-smarttv-compat"),
        Some(&pgs()),
        SubtitleMode::BurnIn,
        &mut reasoning,
    )
    .expect("bitmap burn-in is supported");

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].action, SubtitleAction::BurnIn);
    assert_eq!(tracks[0].encoder.as_deref(), Some("overlay"));
    assert_eq!(tracks[0].output_index, None);
}

#[test]
fn burn_in_is_refused_when_the_preset_has_no_video() {
    let mut reasoning = reasons();

    let error = resolve_subtitles(
        preset("system-audio-mp3"),
        Some(&subrip()),
        SubtitleMode::BurnIn,
        &mut reasoning,
    )
    .expect_err("there are no frames to paint subtitles into");

    assert_eq!(error.code, ErrorCode::PlanInvalid);
    assert!(
        error.message.contains("video"),
        "the message must say what is missing: {}",
        error.message
    );
}

#[test]
fn no_selected_track_produces_no_plan() {
    let mut reasoning = reasons();

    let tracks = resolve_subtitles(
        preset("system-smarttv-hevc"),
        None,
        SubtitleMode::SoftEmbed,
        &mut reasoning,
    )
    .expect("nothing selected is not an error");

    assert!(tracks.is_empty());
}
