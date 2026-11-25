//! FFmpeg command builder contracts (task T7 / C-015).
//!
//! `build_ffmpeg_args` is pure: it returns an argument vector, never a shell
//! string and never a child process (`docs/architecture.md` §10, §15). Plans are
//! built literally so the builder is tested in isolation from `plan_conversion`.

use cauce_core::command::{HostFilters, REQUIRED_FILTERS, build_ffmpeg_args, tonemap_filter_chain};
use cauce_core::{
    AudioPlan, ConversionPlan, ErrorCode, HdrAction, MetadataPolicy, StreamAction, SubtitleAction,
    SubtitlePlan, ToneMappingOption, VideoPlan,
};

const INPUT: &str = "/home/wilkin/Vídeos/entrada.mp4";
const TEMP: &str = "/home/wilkin/Vídeos/.cauce/entrada.mp4.part-7f3a.mp4";

#[rustfmt::skip]
fn video(action: StreamAction, encoder: Option<&str>, index: Option<u32>) -> VideoPlan {
    VideoPlan {
        action, encoder: encoder.map(Into::into), quality: Some(23),
        max_width: None, max_height: None, fps: None, pixel_format: None,
        profile: None, level: None, hdr_action: HdrAction::Preserve,
        stream_index: index, tone_mapping: None,
    }
}

#[rustfmt::skip]
fn audio(action: StreamAction, encoder: Option<&str>, index: Option<u32>) -> AudioPlan {
    AudioPlan {
        action, encoder: encoder.map(Into::into), bitrate: None, channels: None,
        stream_index: index, normalize: false,
        filter: None,
    }
}

#[rustfmt::skip]
fn subs(action: SubtitleAction, encoder: Option<&str>, index: Option<u32>) -> SubtitlePlan {
    SubtitlePlan {
        action, encoder: encoder.map(Into::into), stream_index: index, source_index: None,
    }
}

fn plan(video: VideoPlan, audio: AudioPlan, subtitles: SubtitlePlan) -> ConversionPlan {
    ConversionPlan {
        container: "mp4".into(),
        video,
        audio,
        subtitles,
        faststart: false,
        metadata: MetadataPolicy::Preserve,
        reasoning: vec!["fixture plan built by hand".into()],
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

/// `-flag value` adjacent pair.
fn pair(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2)
        .any(|pair| pair[0] == flag && pair[1] == value)
}

fn flag(argv: &[String], name: &str) -> bool {
    argv.iter().any(|arg| arg == name)
}

fn arg_of<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    let index = argv.iter().position(|arg| arg == flag)?;
    argv.get(index + 1).map(String::as_str)
}

fn assert_plan_invalid(result: Result<Vec<String>, cauce_core::JobError>, case: &str) {
    match result {
        Ok(argv) => panic!("{case} must be rejected, got argv {argv:?}"),
        Err(error) => assert_eq!(error.code, ErrorCode::PlanInvalid, "{case}"),
    }
}

#[test]
fn builds_the_exact_argv_for_a_video_transcode() {
    let mut video_plan = video(StreamAction::Transcode, Some("libx264"), Some(0));
    video_plan.profile = Some("high".into());
    video_plan.pixel_format = Some("yuv420p".into());
    video_plan.max_width = Some(1920);
    video_plan.max_height = Some(1080);
    video_plan.fps = Some(60.0);
    video_plan.hdr_action = HdrAction::ToneMapSdr;

    let mut audio_plan = audio(StreamAction::Transcode, Some("aac"), Some(1));
    audio_plan.bitrate = Some("192k".into());
    audio_plan.channels = Some(2);

    let mut conversion = plan(
        video_plan,
        audio_plan,
        subs(SubtitleAction::Remove, None, None),
    );
    conversion.faststart = true;

    let chain = concat!(
        r"scale=w=min(iw\,1920):h=min(ih\,1080):force_original_aspect_ratio=decrease:force_divisible_by=2",
        ",fps=60",
        ",zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709",
        ",tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv",
    );

    #[rustfmt::skip]
    let expected = strings(&[
        "-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-progress", "pipe:1", "-nostats",
        "-i", INPUT,
        "-map", "0:0", "-c:v", "libx264", "-crf", "23", "-profile:v", "high", "-pix_fmt", "yuv420p",
        "-map", "0:1", "-c:a", "aac", "-b:a", "192k", "-ac", "2",
        "-sn",
        "-vf", chain,
        "-movflags", "+faststart",
        TEMP,
    ]);

    let actual =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert_eq!(actual, expected);
}

#[test]
fn copies_streams_without_reencoding_flags() {
    let conversion = plan(
        video(StreamAction::Copy, None, Some(0)),
        audio(StreamAction::Copy, None, Some(1)),
        subs(SubtitleAction::Remove, None, None),
    );
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");

    assert!(pair(&argv, "-c:v", "copy"), "{argv:?}");
    assert!(pair(&argv, "-c:a", "copy"), "{argv:?}");
    assert!(pair(&argv, "-map", "0:0"), "{argv:?}");
    assert!(pair(&argv, "-map", "0:1"), "{argv:?}");
    assert!(!flag(&argv, "-b:a"), "{argv:?}");
    assert!(!flag(&argv, "-ac"), "{argv:?}");
    assert!(!flag(&argv, "-vf"), "{argv:?}");
}

#[test]
fn removes_absent_stream_families() {
    let conversion = plan(
        video(StreamAction::Remove, None, None),
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");

    assert!(flag(&argv, "-vn"), "{argv:?}");
    assert!(flag(&argv, "-an"), "{argv:?}");
    assert!(flag(&argv, "-sn"), "{argv:?}");
    assert!(!flag(&argv, "-map"), "{argv:?}");

    // An audio-only plan keeps `-vn`/`-sn` and drops `-an`.
    let audio_only = plan(
        video(StreamAction::Remove, None, None),
        audio(StreamAction::Transcode, Some("libmp3lame"), Some(1)),
        subs(SubtitleAction::Remove, None, None),
    );
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &audio_only, &HostFilters::ALL).expect("plan must build");
    assert!(flag(&argv, "-vn"), "{argv:?}");
    assert!(flag(&argv, "-sn"), "{argv:?}");
    assert!(!flag(&argv, "-an"), "{argv:?}");
}

#[test]
fn emits_faststart_only_when_requested() {
    let mut conversion = plan(
        video(StreamAction::Copy, None, Some(0)),
        audio(StreamAction::Copy, None, Some(1)),
        subs(SubtitleAction::Copy, Some("mov_text"), Some(2)),
    );
    conversion.faststart = false;
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert!(!flag(&argv, "-movflags"), "{argv:?}");
    assert!(!flag(&argv, "+faststart"), "{argv:?}");

    conversion.faststart = true;
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert!(pair(&argv, "-movflags", "+faststart"), "{argv:?}");
    assert_eq!(argv.last().map(String::as_str), Some(TEMP), "{argv:?}");
}

#[test]
fn strips_metadata_only_when_requested() {
    let mut conversion = plan(
        video(StreamAction::Copy, None, Some(0)),
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );

    conversion.metadata = MetadataPolicy::Strip;
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert!(pair(&argv, "-map_metadata", "-1"), "{argv:?}");

    conversion.metadata = MetadataPolicy::Preserve;
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert!(!flag(&argv, "-map_metadata"), "{argv:?}");
    assert!(!flag(&argv, "-1"), "{argv:?}");
}

#[test]
fn emits_the_scale_filter_without_upscaling() {
    let mut video_plan = video(StreamAction::Transcode, Some("libx265"), Some(0));
    video_plan.max_width = Some(1920);
    video_plan.max_height = Some(1080);
    let conversion = plan(
        video_plan,
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );

    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert_eq!(
        arg_of(&argv, "-vf"),
        Some(
            r"scale=w=min(iw\,1920):h=min(ih\,1080):force_original_aspect_ratio=decrease:force_divisible_by=2"
        )
    );
}

#[test]
fn emits_hdr_chain_only_for_tone_map() {
    let mut video_plan = video(StreamAction::Transcode, Some("libx265"), Some(0));
    video_plan.hdr_action = HdrAction::ToneMapSdr;
    let conversion = plan(
        video_plan.clone(),
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );

    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    let chain = arg_of(&argv, "-vf").expect("chain must be emitted");
    assert_eq!(chain, tonemap_filter_chain());
    assert!(chain.contains("zscale"), "{argv:?}");
    assert!(chain.contains("tonemap=tonemap=hable"), "{argv:?}");

    video_plan.hdr_action = HdrAction::Preserve;
    let conversion = plan(
        video_plan,
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert!(!flag(&argv, "-vf"), "{argv:?}");
    assert!(!argv.iter().any(|arg| arg.contains("zscale")), "{argv:?}");
}

#[test]
fn exposes_the_required_filters_for_the_capability_gate() {
    assert_eq!(REQUIRED_FILTERS, ["tonemap", "scale", "format", "fps"]);
    assert!(tonemap_filter_chain().starts_with("zscale=t=linear"));
}

#[test]
fn tonemap_uses_zscale_when_capabilities_report_it() {
    let mut hdr_video = video(StreamAction::Transcode, Some("libx264"), Some(0));
    hdr_video.hdr_action = HdrAction::ToneMapSdr;
    let conversion = plan(
        hdr_video,
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    let filters = HostFilters {
        zscale: true,
        loudnorm: true,
    };
    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &filters).expect("plan must build");
    let chain = arg_of(&argv, "-vf").expect("-vf must be present");
    assert!(chain.contains("zscale"), "{argv:?}");
    assert_eq!(chain, tonemap_filter_chain());
}

#[test]
fn tonemap_falls_back_to_the_native_filter_without_zscale() {
    // Phase 1 made the curve part of the plan, so the native path carries
    // whatever the plan asks for. A plan written by 0.1.x names no curve and
    // keeps the hable default; asking for mobius explicitly still works, which
    // is the fallback phase 0 stabilized.
    let filters = HostFilters {
        zscale: false,
        loudnorm: true,
    };

    let mut hdr_video = video(StreamAction::Transcode, Some("libx264"), Some(0));
    hdr_video.hdr_action = HdrAction::ToneMapSdr;
    let conversion = plan(
        hdr_video,
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &filters).expect("plan must build");
    let chain = arg_of(&argv, "-vf").expect("-vf must be present");
    assert_eq!(chain, "tonemap=hable,format=yuv420p", "{argv:?}");
    assert!(!chain.contains("zscale"), "{argv:?}");

    let mut mobius_video = video(StreamAction::Transcode, Some("libx264"), Some(0));
    mobius_video.hdr_action = HdrAction::ToneMapSdr;
    mobius_video.tone_mapping = Some(ToneMappingOption::Mobius);
    let conversion = plan(
        mobius_video,
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &filters).expect("plan must build");
    let chain = arg_of(&argv, "-vf").expect("-vf must be present");
    assert_eq!(chain, "tonemap=mobius,format=yuv420p", "{argv:?}");
}

#[test]
fn required_filters_no_longer_contains_zscale() {
    assert!(!REQUIRED_FILTERS.contains(&"zscale"));
    assert_eq!(REQUIRED_FILTERS, ["tonemap", "scale", "format", "fps"]);
}

#[test]
fn rejects_unsupported_encoders() {
    #[rustfmt::skip]
    let cases: [(VideoPlan, AudioPlan, &str); 4] = [
        (video(StreamAction::Transcode, Some("h264_nvenc"), Some(0)),
         audio(StreamAction::Remove, None, None), "h264_nvenc video encoder"),
        (video(StreamAction::Transcode, Some("opus"), Some(0)),
         audio(StreamAction::Remove, None, None), "opus used as a video encoder"),
        (video(StreamAction::Remove, None, None),
         audio(StreamAction::Transcode, Some("libfdk_aac"), Some(1)), "libfdk_aac audio encoder"),
        (video(StreamAction::Remove, None, None),
         audio(StreamAction::Transcode, Some("mp3"), Some(1)),
         "mp3 is a codec name, not an encoder"),
    ];

    for (video_plan, audio_plan, case) in cases {
        let conversion = plan(
            video_plan,
            audio_plan,
            subs(SubtitleAction::Remove, None, None),
        );
        assert_plan_invalid(
            build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL),
            case,
        );
    }

    let bitmap_subtitles = plan(
        video(StreamAction::Copy, None, Some(0)),
        audio(StreamAction::Remove, None, None),
        subs(
            SubtitleAction::Transcode,
            Some("hdmv_pgs_subtitle"),
            Some(2),
        ),
    );
    assert_plan_invalid(
        build_ffmpeg_args(INPUT, TEMP, &bitmap_subtitles, &HostFilters::ALL),
        "bitmap subtitle",
    );
}

#[test]
fn rejects_unsafe_paths() {
    let conversion = plan(
        video(StreamAction::Copy, None, Some(0)),
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );

    assert_plan_invalid(
        build_ffmpeg_args("entrada.mp4", TEMP, &conversion, &HostFilters::ALL),
        "relative input",
    );
    assert_plan_invalid(
        build_ffmpeg_args(INPUT, "salida.part", &conversion, &HostFilters::ALL),
        "relative temp",
    );
    assert_plan_invalid(
        build_ffmpeg_args("", TEMP, &conversion, &HostFilters::ALL),
        "empty input",
    );
    assert_plan_invalid(
        build_ffmpeg_args(INPUT, "", &conversion, &HostFilters::ALL),
        "empty temp output",
    );
    assert_plan_invalid(
        build_ffmpeg_args("/a\0b.mp4", TEMP, &conversion, &HostFilters::ALL),
        "NUL in input",
    );
    assert_plan_invalid(
        build_ffmpeg_args(INPUT, "/a\0b.mp4", &conversion, &HostFilters::ALL),
        "NUL in temp output",
    );
    assert_plan_invalid(
        build_ffmpeg_args(INPUT, INPUT, &conversion, &HostFilters::ALL),
        "temp equal to input",
    );
}

#[test]
fn accepts_audio_normalization_after_phase_0() {
    // Phase 0 (BUG-001) lifted the v0.1 rule that refused every audio filter:
    // `normalize` now reaches FFmpeg as `loudnorm` instead of aborting the job.
    let mut audio_plan = audio(StreamAction::Transcode, Some("aac"), Some(1));
    audio_plan.normalize = true;
    let conversion = plan(
        video(StreamAction::Copy, None, Some(0)),
        audio_plan,
        subs(SubtitleAction::Remove, None, None),
    );

    let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
        .expect("normalize must no longer fail");
    let chain = arg_of(&argv, "-af").expect("normalize must emit an audio filter");
    assert!(chain.contains("loudnorm"), "got `{chain}`");
}

#[test]
fn never_repeats_the_input_flag() {
    let mut stripped = plan(
        video(StreamAction::Transcode, Some("libsvtav1"), Some(3)),
        audio(StreamAction::Copy, None, Some(4)),
        subs(SubtitleAction::Remove, None, None),
    );
    stripped.faststart = true;
    stripped.metadata = MetadataPolicy::Strip;

    let shapes = vec![
        plan(
            video(StreamAction::Transcode, Some("libx264"), Some(0)),
            audio(StreamAction::Transcode, Some("aac"), Some(1)),
            subs(SubtitleAction::Transcode, Some("mov_text"), Some(2)),
        ),
        plan(
            video(StreamAction::Copy, None, Some(0)),
            audio(StreamAction::Copy, None, Some(1)),
            subs(SubtitleAction::Copy, Some("srt"), Some(2)),
        ),
        plan(
            video(StreamAction::Remove, None, None),
            audio(StreamAction::Remove, None, None),
            subs(SubtitleAction::Remove, None, None),
        ),
        stripped,
    ];

    for conversion in shapes {
        let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
            .expect("plan must build");
        assert_eq!(
            argv.iter().filter(|arg| *arg == "-i").count(),
            1,
            "{argv:?}"
        );
        assert_eq!(argv.last().map(String::as_str), Some(TEMP), "{argv:?}");
    }
}

#[test]
fn emits_annex_a_levels_for_encoders_that_take_them() {
    // Verified against FFmpeg 8.0.1: libvpx-vp9 rejects `41` (exit 222) and
    // accepts `4.1`, while libx264 accepts the Annex A string.
    for encoder in ["libx264", "libvpx-vp9"] {
        let conversion = plan(
            video(StreamAction::Transcode, Some(encoder), Some(0)),
            audio(StreamAction::Remove, None, None),
            subs(SubtitleAction::Remove, None, None),
        );
        let mut conversion = conversion;
        conversion.video.level = Some(41);

        let argv = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
            .expect("plan must build");
        assert!(
            pair(&argv, "-level", "4.1"),
            "{encoder} must receive the Annex A level, got {argv:?}"
        );
        assert!(
            !argv.iter().any(|arg| arg == "41"),
            "{encoder} must never receive the raw level_idc: {argv:?}"
        );
    }
}

#[test]
fn sends_the_x265_level_through_x265_params() {
    let mut conversion = plan(
        video(StreamAction::Transcode, Some("libx265"), Some(0)),
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    conversion.video.level = Some(41);

    let argv =
        build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL).expect("plan must build");
    assert!(
        pair(&argv, "-x265-params", "level-idc=41"),
        "libx265 has no -level option, the level must travel in -x265-params: {argv:?}"
    );
    assert!(!argv.iter().any(|arg| arg == "-level"), "{argv:?}");
}

#[test]
fn rejects_a_level_the_encoder_cannot_honor() {
    // libsvtav1 accepts `-level` and ignores it, so v0.1 refuses instead of
    // silently dropping the preset constraint.
    let mut conversion = plan(
        video(StreamAction::Transcode, Some("libsvtav1"), Some(0)),
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    conversion.video.level = Some(41);

    let error = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
        .expect_err("libsvtav1 cannot honor a level");
    assert_eq!(error.code, ErrorCode::PlanInvalid);
    assert!(error.message.contains("libsvtav1"), "{}", error.message);
}

#[test]
fn rejects_an_unknown_level_idc() {
    let mut conversion = plan(
        video(StreamAction::Transcode, Some("libx264"), Some(0)),
        audio(StreamAction::Remove, None, None),
        subs(SubtitleAction::Remove, None, None),
    );
    conversion.video.level = Some(43);

    let error = build_ffmpeg_args(INPUT, TEMP, &conversion, &HostFilters::ALL)
        .expect_err("43 is not a level_idc");
    assert_eq!(error.code, ErrorCode::PlanInvalid);
}
