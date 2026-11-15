//! Output path resolution contracts (task T8 / C-016).
//!
//! `resolve_output_path` and `temp_path` are pure: they never touch the
//! filesystem, so a "taken" path is simply one listed in `reserved`, the set of
//! `output_reservations` owned by active jobs (`docs/contracts.md` §3, §9;
//! `docs/architecture.md` §12, §15).

use cauce_core::{DuplicatePolicy, ErrorCode, JobError, resolve_output_path, temp_path};

const VIDEO_DIR: &str = "/home/wilkin/Vídeos";
const OUTPUT_DIR: &str = "/home/wilkin/Vídeos/convertidos";
const INPUT: &str = "/home/wilkin/Vídeos/holiday.mp4";

fn reserved(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
}

fn code(error: &JobError) -> ErrorCode {
    error.code
}

#[test]
fn resolves_a_suffixed_output() {
    let path = resolve_output_path(INPUT, OUTPUT_DIR, "mp4", DuplicatePolicy::Suffix, &[])
        .expect("a free destination must resolve");

    assert_eq!(path, format!("{OUTPUT_DIR}/holiday.mp4"));
    assert_ne!(path, INPUT);
}

#[test]
fn increments_the_suffix_while_reserved() {
    let first = format!("{OUTPUT_DIR}/holiday.mp4");
    let second = format!("{OUTPUT_DIR}/holiday (1).mp4");
    let taken = reserved(&[&first, &second]);

    let path = resolve_output_path(INPUT, OUTPUT_DIR, "mp4", DuplicatePolicy::Suffix, &taken)
        .expect("the next free suffix must resolve");

    assert_eq!(path, format!("{OUTPUT_DIR}/holiday (2).mp4"));
}

#[test]
fn skips_when_the_destination_is_reserved() {
    let taken = reserved(&[&format!("{OUTPUT_DIR}/holiday.mp4")]);

    let error = resolve_output_path(INPUT, OUTPUT_DIR, "mp4", DuplicatePolicy::Skip, &taken)
        .expect_err("skip must refuse a taken destination");

    assert_eq!(code(&error), ErrorCode::OutputCollision);
    assert!(error.retryable, "a collision is retryable: {error:?}");
    assert!(
        error.message.contains("existe"),
        "the message names the existing destination: {}",
        error.message
    );
    assert!(
        error.message.contains("omitir"),
        "the message names the skip policy: {}",
        error.message
    );
}

#[test]
fn overwrites_only_on_request() {
    let taken = reserved(&[&format!("{OUTPUT_DIR}/holiday.mp4")]);

    let path = resolve_output_path(INPUT, OUTPUT_DIR, "mp4", DuplicatePolicy::Overwrite, &taken)
        .expect("overwrite must keep the taken destination");

    assert_eq!(path, format!("{OUTPUT_DIR}/holiday.mp4"));
}

#[test]
fn never_returns_the_input_path() {
    for policy in [
        DuplicatePolicy::Suffix,
        DuplicatePolicy::Skip,
        DuplicatePolicy::Overwrite,
    ] {
        let error = resolve_output_path(INPUT, VIDEO_DIR, "mp4", policy, &[])
            .expect_err("the destination may never be the input");

        assert_eq!(code(&error), ErrorCode::InvalidInput, "policy {policy:?}");
        assert!(
            error.message.contains("entrada"),
            "the message names the input: {}",
            error.message
        );
    }
}

#[test]
fn rejects_relative_and_unsafe_paths() {
    let cases: &[(&str, &str, &str)] = &[
        ("holiday.mp4", OUTPUT_DIR, "mp4"),
        (INPUT, "convertidos", "mp4"),
        ("/home/wilkin/Vídeos/holi\0day.mp4", OUTPUT_DIR, "mp4"),
        (INPUT, "/home/wilkin/Vídeos/converti\0dos", "mp4"),
        (INPUT, "", "mp4"),
        (INPUT, OUTPUT_DIR, ""),
        (INPUT, OUTPUT_DIR, "tar.gz"),
        (INPUT, OUTPUT_DIR, "m/p4"),
        (INPUT, OUTPUT_DIR, "mp\u{0}4"),
        ("/", OUTPUT_DIR, "mp4"),
    ];

    for (input, output_dir, extension) in cases {
        let error = resolve_output_path(input, output_dir, extension, DuplicatePolicy::Suffix, &[])
            .expect_err("unsafe path components must be rejected");

        assert_eq!(
            code(&error),
            ErrorCode::InvalidInput,
            "input `{input}`, dir `{output_dir}`, extension `{extension}`"
        );
    }
}

#[test]
fn builds_a_temp_next_to_the_destination() {
    let path = temp_path("/home/wilkin/Vídeos/convertidos/holiday.mp4", "7f3a-11ee")
        .expect("a well formed destination has a temporary");

    assert_eq!(
        path,
        "/home/wilkin/Vídeos/convertidos/.cauce-7f3a-11ee.part.mp4"
    );

    let nested = temp_path("/home/wilkin/Vídeos/copia.webm", "job.1").expect("nested destination");
    assert_eq!(nested, "/home/wilkin/Vídeos/.cauce-job.1.part.webm");

    let no_parent = temp_path("holiday.mp4", "7f3a").expect_err("a bare name has no folder");
    assert_eq!(code(&no_parent), ErrorCode::InvalidInput);

    let no_extension = temp_path("/home/wilkin/Vídeos/holiday", "7f3a")
        .expect_err("the temporary needs the destination extension");
    assert_eq!(code(&no_extension), ErrorCode::InvalidInput);
}

#[test]
fn rejects_unsafe_job_ids() {
    for job_id in ["", " ", "a/b", "a b", "../escape", "job\t1", "job\n1", "ñ"] {
        let error = temp_path("/home/wilkin/Vídeos/convertidos/holiday.mp4", job_id)
            .expect_err("unsafe job ids must be rejected");

        assert_eq!(code(&error), ErrorCode::InvalidInput, "job id `{job_id}`");
    }
}
