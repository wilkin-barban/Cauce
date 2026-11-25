//! Contract of the per-file selection state (T8).
//!
//! Every file retains independent `TrackSelection` and `CustomConversionOptions`
//! derived initially from the probe, recomputed against new presets only for
//! untouched fields, and validated before queueing.

use cauce_core::{
    AudioMixMode, AudioStream, MediaSnapshot, Preset, ResolutionTarget, SubtitleKind, SubtitleMode,
    SubtitleStream, Timestamp, ToneMappingOption, VideoStream, system_presets,
};
use cauce_gui::selection::{FileSelection, SelectionTable};

fn preset(id: &str) -> &'static Preset {
    system_presets()
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("preset `{id}` must exist"))
}

fn test_snapshot(
    path: &str,
    audio_streams: Vec<AudioStream>,
    subtitle_streams: Vec<SubtitleStream>,
) -> MediaSnapshot {
    MediaSnapshot {
        path: path.to_owned(),
        container: "mkv".to_owned(),
        duration_ms: 120_000,
        size_bytes: 10_000_000,
        bitrate_bps: 2_000_000,
        video_streams: vec![VideoStream {
            index: 0,
            codec: "h264".to_owned(),
            profile: Some("high".to_owned()),
            level: None,
            width: 1920,
            height: 1080,
            fps: 24.0,
            pixel_format: "yuv420p".to_owned(),
            bit_depth: 8,
            is_hdr: false,
            bitrate_bps: None,
            language: None,
        }],
        audio_streams,
        subtitle_streams,
        probed_at: Timestamp::parse("2026-11-24T12:00:00Z").unwrap(),
    }
}

fn audio(index: u32, channels: u8, language: &str, is_default: bool) -> AudioStream {
    AudioStream {
        index,
        codec: if channels >= 6 { "eac3" } else { "aac" }.to_owned(),
        channels,
        sample_rate: 48_000,
        bitrate_bps: Some(if channels >= 6 { 640_000 } else { 192_000 }),
        language: Some(language.to_owned()),
        is_default,
    }
}

fn subtitle(
    index: u32,
    codec: &str,
    kind: SubtitleKind,
    language: &str,
    is_default: bool,
) -> SubtitleStream {
    SubtitleStream {
        index,
        codec: codec.to_owned(),
        language: Some(language.to_owned()),
        kind,
        is_default,
        is_forced: false,
    }
}

#[test]
fn defaults_come_from_the_probe() {
    // File 1: has default 6ch audio and subtitles
    let snap1 = test_snapshot(
        "/media/movie1.mkv",
        vec![audio(1, 2, "eng", false), audio(2, 6, "spa", true)],
        vec![subtitle(3, "subrip", SubtitleKind::Text, "spa", true)],
    );
    let sel1 = FileSelection::from_probe(&snap1, None);

    // Audio: is_default stream (index 2)
    assert_eq!(sel1.selection.audio_track_indices, vec![2]);
    // 6 channels -> StereoDialogueBoost
    assert_eq!(
        sel1.selection.audio_mix_mode,
        AudioMixMode::StereoDialogueBoost
    );
    // Subtitles present -> SoftEmbed, track 3
    assert_eq!(sel1.selection.subtitle_mode, SubtitleMode::SoftEmbed);
    assert_eq!(sel1.selection.subtitle_track_index, Some(3));
    // Defaults for custom options
    assert_eq!(sel1.options.resolution, ResolutionTarget::Original);
    assert_eq!(sel1.options.tone_mapping, ToneMappingOption::Hable);
    assert_eq!(sel1.options.crf, None);

    // File 2: no default stream (first wins), 2 channels (Passthrough), no subtitles (Strip)
    let snap2 = test_snapshot(
        "/media/clip.mkv",
        vec![audio(1, 2, "spa", false), audio(2, 2, "eng", false)],
        vec![],
    );
    let sel2 = FileSelection::from_probe(&snap2, None);

    assert_eq!(sel2.selection.audio_track_indices, vec![1]);
    assert_eq!(sel2.selection.audio_mix_mode, AudioMixMode::Passthrough);
    assert_eq!(sel2.selection.subtitle_mode, SubtitleMode::Strip);
    assert_eq!(sel2.selection.subtitle_track_index, None);
}

#[test]
fn two_files_keep_independent_selections() {
    let mut table = SelectionTable::new();
    let snap1 = test_snapshot("/media/file1.mkv", vec![audio(1, 2, "spa", true)], vec![]);
    let snap2 = test_snapshot("/media/file2.mkv", vec![audio(1, 6, "eng", true)], vec![]);

    table.register(&snap1, None);
    table.register(&snap2, None);

    // Customize file 1
    let file1 = table.get_mut("/media/file1.mkv").expect("file1 exists");
    file1.set_crf(Some(18));
    file1.set_audio_mix_mode(AudioMixMode::StereoDialogueBoost);

    // Verify file 2 is untouched
    let file2 = table.get("/media/file2.mkv").expect("file2 exists");
    assert_eq!(file2.options.crf, None);
    assert_eq!(
        file2.selection.audio_mix_mode,
        AudioMixMode::StereoDialogueBoost
    ); // default for 6ch
    assert!(!file2.touched.crf);
    assert!(!file2.touched.audio_mix_mode);

    // Customize file 2
    let file2_mut = table.get_mut("/media/file2.mkv").expect("file2 exists");
    file2_mut.set_crf(Some(24));
    file2_mut.set_audio_mix_mode(AudioMixMode::DualTrackUniversal);

    // Verify file 1 kept its own values
    let file1_check = table.get("/media/file1.mkv").expect("file1 exists");
    assert_eq!(file1_check.options.crf, Some(18));
    assert_eq!(
        file1_check.selection.audio_mix_mode,
        AudioMixMode::StereoDialogueBoost
    );
}

#[test]
fn changing_the_preset_keeps_user_choices() {
    let mut table = SelectionTable::new();
    let snap = test_snapshot(
        "/media/film.mkv",
        vec![audio(1, 2, "eng", true), audio(2, 6, "spa", false)],
        vec![subtitle(3, "subrip", SubtitleKind::Text, "spa", true)],
    );
    let p_smarttv = preset("system-smarttv-hevc");
    table.register(&snap, Some(p_smarttv));

    // User explicitly sets crf and subtitle_mode
    let sel = table.get_mut("/media/film.mkv").expect("registered");
    sel.set_crf(Some(21));
    sel.set_subtitle_mode(SubtitleMode::BurnIn);

    // Now change preset to a preset with preferred_audio_language or different policy
    let mut p_spanish = p_smarttv.clone();
    p_spanish.preferred_audio_language = Some("spa".to_owned());

    table.apply_preset(&p_spanish, std::slice::from_ref(&snap));

    let updated = table.get("/media/film.mkv").expect("registered");
    // Untouched audio track recomputed to match preferred_audio_language "spa" (track 2)
    assert_eq!(updated.selection.audio_track_indices, vec![2]);
    // Untouched mix mode follows track 2 (6 channels -> StereoDialogueBoost)
    assert_eq!(
        updated.selection.audio_mix_mode,
        AudioMixMode::StereoDialogueBoost
    );
    // User choices survive!
    assert_eq!(updated.options.crf, Some(21));
    assert_eq!(updated.selection.subtitle_mode, SubtitleMode::BurnIn);
}

#[test]
fn user_touched_audio_track_survives_preset_change() {
    let mut table = SelectionTable::new();
    let snap = test_snapshot(
        "/media/film.mkv",
        vec![
            audio(1, 2, "eng", true),
            audio(2, 6, "spa", false),
            audio(3, 2, "fra", false),
        ],
        vec![],
    );
    let p_smarttv = preset("system-smarttv-hevc");
    table.register(&snap, Some(p_smarttv));

    // User chooses French (track 3)
    let sel = table.get_mut("/media/film.mkv").expect("registered");
    sel.set_audio_track(3, Some(&snap));

    // Preset changes to Spanish preferred
    let mut p_spanish = p_smarttv.clone();
    p_spanish.preferred_audio_language = Some("spa".to_owned());

    table.apply_preset(&p_spanish, std::slice::from_ref(&snap));

    let updated = table.get("/media/film.mkv").expect("registered");
    // Track 3 was touched, so it stays track 3!
    assert_eq!(updated.selection.audio_track_indices, vec![3]);
}

#[test]
fn removing_a_file_leaves_the_others_alone() {
    let mut table = SelectionTable::new();
    let snap1 = test_snapshot("/media/a.mkv", vec![audio(1, 2, "spa", true)], vec![]);
    let snap2 = test_snapshot("/media/b.mkv", vec![audio(1, 2, "eng", true)], vec![]);

    table.register(&snap1, None);
    table.register(&snap2, None);

    let b = table.get_mut("/media/b.mkv").expect("b exists");
    b.set_crf(Some(19));
    b.set_resolution(ResolutionTarget::MaxHeight(720));

    // Remove file a
    let removed = table.remove("/media/a.mkv");
    assert!(removed.is_some());
    assert_eq!(table.len(), 1);
    assert!(table.get("/media/a.mkv").is_none());

    // File b is intact
    let b_check = table.get("/media/b.mkv").expect("b still exists");
    assert_eq!(b_check.options.crf, Some(19));
    assert_eq!(b_check.options.resolution, ResolutionTarget::MaxHeight(720));
}

#[test]
fn an_unknown_track_index_is_refused_before_queueing() {
    let mut table = SelectionTable::new();
    let snap = test_snapshot(
        "/media/video.mkv",
        vec![audio(1, 2, "spa", true)],
        vec![subtitle(2, "subrip", SubtitleKind::Text, "spa", true)],
    );
    let p = preset("system-smarttv-hevc");
    table.register(&snap, Some(p));

    // Invalid audio track
    let sel = table.get_mut("/media/video.mkv").expect("registered");
    sel.set_audio_track(99, Some(&snap));

    let err = table
        .validate(std::slice::from_ref(&snap), p)
        .expect_err("audio track 99 does not exist");
    assert_eq!(err.path, "/media/video.mkv");
    assert!(err.to_string().contains("/media/video.mkv"));

    // Reset audio and set invalid subtitle track
    let sel = table.get_mut("/media/video.mkv").expect("registered");
    sel.set_audio_track(1, Some(&snap));
    sel.set_subtitle_track(Some(88));

    let err = table
        .validate(std::slice::from_ref(&snap), p)
        .expect_err("subtitle track 88 does not exist");
    assert_eq!(err.path, "/media/video.mkv");
    assert!(err.to_string().contains("/media/video.mkv"));
}

#[test]
fn burn_in_is_refused_for_an_audio_only_preset() {
    let mut table = SelectionTable::new();
    let snap = test_snapshot(
        "/media/film.mkv",
        vec![audio(1, 2, "spa", true)],
        vec![subtitle(2, "subrip", SubtitleKind::Text, "spa", true)],
    );
    // system-audio-mp3 has video_codec: "none"
    let p_audio = preset("system-audio-mp3");
    assert_eq!(p_audio.video_codec, "none");

    table.register(&snap, Some(p_audio));
    let sel = table.get_mut("/media/film.mkv").expect("registered");
    sel.set_subtitle_mode(SubtitleMode::BurnIn);

    let err = table
        .validate(std::slice::from_ref(&snap), p_audio)
        .expect_err("burn-in refused for audio preset");
    assert_eq!(err.path, "/media/film.mkv");
    assert!(err.to_string().contains("/media/film.mkv"));
}

#[test]
fn every_file_queues_its_own_selection() {
    use cauce_gui::pages::convert::state::ConvertState;

    let mut state = ConvertState::new();
    state.set_output_dir(Some("/tmp/out".to_owned()));
    state.set_preset(Some("system-smarttv-compat".to_owned()));
    state.accept_probe(test_snapshot(
        "/media/first.mkv",
        vec![audio(1, 6, "spa", true), audio(2, 2, "eng", false)],
        vec![],
    ));
    state.accept_probe(test_snapshot(
        "/media/second.mkv",
        vec![audio(1, 2, "eng", true)],
        vec![],
    ));

    state
        .selection_for_mut("/media/first.mkv")
        .expect("first selection")
        .selection
        .audio_track_indices = vec![2];
    state
        .selection_for_mut("/media/second.mkv")
        .expect("second selection");

    let requests = state.take_queue_requests();

    assert_eq!(requests.len(), 2, "one request per file, not one for all");
    assert!(requests.iter().all(|request| request.inputs.len() == 1));
    let first = requests
        .iter()
        .find(|request| request.inputs[0] == "/media/first.mkv")
        .expect("request for the first file");
    assert_eq!(
        first
            .track_selection
            .as_ref()
            .expect("selection")
            .audio_track_indices,
        vec![2],
        "the first file keeps the track the user chose"
    );
    let second = requests
        .iter()
        .find(|request| request.inputs[0] == "/media/second.mkv")
        .expect("request for the second file");
    assert_ne!(
        second
            .track_selection
            .as_ref()
            .expect("selection")
            .audio_track_indices,
        vec![2],
        "and the second file must not inherit it"
    );
}
