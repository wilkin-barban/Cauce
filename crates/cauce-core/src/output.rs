//! Output path resolution, temporary naming and post-conversion validation.
//!
//! Task T8 / C-016. Everything here is pure: no filesystem, no processes and no
//! clock, so the daemon can decide a destination and judge a temporary file
//! without racing the real tree. The invariants come from `docs/contracts.md` §3
//! and §9 and `docs/architecture.md` §12 and §15:
//!
//! - the destination is never the input;
//! - `suffix` never reuses a reserved path, `skip` reports a collision and
//!   `overwrite` only returns a taken path when the user asked for it;
//! - the temporary lives in the destination folder, carries the job id and is
//!   never published until it validates.

use std::path::Path;

use crate::error::{ErrorCode, JobError};
use crate::media::MediaSnapshot;
use crate::plan::{ConversionPlan, StreamAction, SubtitleAction};
use crate::settings::DuplicatePolicy;

/// Minimum duration tolerance, in milliseconds (`architecture.md` §12).
const MIN_DURATION_TOLERANCE_MS: u64 = 2_000;
/// Relative duration tolerance, in percent.
const DURATION_TOLERANCE_PERCENT: u64 = 3;

/// Resolves the published destination for `input`.
///
/// `reserved` holds the output paths already reserved by active jobs
/// (`output_reservations`); a path is "taken" only when it appears there.
pub fn resolve_output_path(
    input: &str,
    output_dir: &str,
    extension: &str,
    policy: DuplicatePolicy,
    reserved: &[String],
) -> Result<String, JobError> {
    require_absolute(input, "La ruta de entrada")?;
    require_absolute(output_dir, "La carpeta de salida")?;
    if extension.is_empty()
        || extension.contains('/')
        || extension.contains('.')
        || extension.contains('\0')
    {
        return Err(invalid(format!(
            "La extensión `{extension}` no es válida: no puede estar vacía ni contener `/`, `.` o bytes nulos."
        )));
    }

    let stem = Path::new(input)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(strip_leading_dot)
        .unwrap_or("");
    if stem.is_empty() {
        return Err(invalid(format!(
            "No se pudo derivar un nombre de archivo a partir de la entrada `{input}`."
        )));
    }

    let dir = output_dir.trim_end_matches('/');
    let candidate = match policy {
        DuplicatePolicy::Overwrite => join(dir, &file_name(stem, extension, None)),
        DuplicatePolicy::Skip if taken(&join(dir, &file_name(stem, extension, None)), reserved) => {
            return Err(JobError::new(
                ErrorCode::OutputCollision,
                format!(
                    "El destino `{}` ya existe y la política de duplicados es omitir; \
                     cambiá la carpeta de salida o la política.",
                    join(dir, &file_name(stem, extension, None))
                ),
                true,
            ));
        }
        DuplicatePolicy::Skip => join(dir, &file_name(stem, extension, None)),
        DuplicatePolicy::Suffix => {
            let mut index: usize = 0;
            let mut current = join(dir, &file_name(stem, extension, None));
            while taken(&current, reserved) {
                index += 1;
                let suffix = u32::try_from(index).unwrap_or(u32::MAX);
                current = join(dir, &file_name(stem, extension, Some(suffix)));
            }
            current
        }
    };

    if candidate == input {
        return Err(invalid(
            "La ruta de salida no puede ser igual a la de entrada: elegí otra carpeta o nombre.",
        ));
    }

    Ok(candidate)
}

/// Builds `<dir>/.cauce-<job_id>.part.<extension>` for `output_path`.
///
/// Living in the destination folder keeps the final publish a same-filesystem
/// rename, and the job id keeps two concurrent jobs apart.
pub fn temp_path(output_path: &str, job_id: &str) -> Result<String, JobError> {
    if !is_safe_job_id(job_id) {
        return Err(invalid(format!(
            "El identificador de trabajo `{job_id}` no es válido: solo se aceptan letras, \
             dígitos, `.`, `_` y `-`."
        )));
    }

    require_absolute(output_path, "La ruta de salida")?;
    let path = Path::new(output_path);
    let dir = path
        .parent()
        .and_then(|parent| parent.to_str())
        .filter(|parent| !parent.is_empty())
        .ok_or_else(|| {
            invalid(format!(
                "La ruta de salida `{output_path}` no tiene una carpeta donde alojar el temporal."
            ))
        })?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| {
            invalid(format!(
                "La ruta de salida `{output_path}` no tiene extensión para el temporal."
            ))
        })?;

    Ok(join(
        dir.trim_end_matches('/'),
        &format!(".cauce-{job_id}.part.{extension}"),
    ))
}

/// Checks the probe of the temporary against the plan and the source
/// (`docs/architecture.md` §12). The first broken rule wins and its message
/// names the rule, so the UI can explain the failure.
pub fn validate_output(
    plan: &ConversionPlan,
    source: &MediaSnapshot,
    output: &MediaSnapshot,
) -> Result<(), JobError> {
    if output.size_bytes == 0 {
        return Err(failed(
            "El archivo de salida está vacío (0 bytes); no se publica nada.",
        ));
    }

    if source.duration_ms > 0 {
        let tolerance = duration_tolerance_ms(source.duration_ms);
        let actual = output.duration_ms;
        let difference = actual.abs_diff(source.duration_ms);
        if difference > tolerance {
            return Err(failed(format!(
                "La duración de la salida ({actual} ms) se aleja {difference} ms de la entrada \
                 ({}) y la tolerancia es de {tolerance} ms.",
                source.duration_ms
            )));
        }
    }

    check_family(
        plan.video.action == StreamAction::Remove,
        output.video_streams.len(),
        "video",
    )?;
    check_family(
        plan.audio.action == StreamAction::Remove,
        output.audio_streams.len(),
        "audio",
    )?;
    match plan.subtitles.action {
        SubtitleAction::Remove => check_family(true, output.subtitle_streams.len(), "subtítulos")?,
        SubtitleAction::Copy | SubtitleAction::Transcode => {
            check_family(false, output.subtitle_streams.len(), "subtítulos")?;
        }
        // v0.1 never burns in subtitles: a burned-in track is pixels, not a stream.
        SubtitleAction::BurnIn => {}
    }

    if matches!(
        plan.video.action,
        StreamAction::Transcode | StreamAction::Copy
    ) {
        let expected = expected_video_codec(plan, source);
        let actual = output
            .video_streams
            .first()
            .map(|stream| stream.codec.as_str());
        if let (Some(expected), Some(actual)) = (expected, actual) {
            if !same_codec(expected, actual) {
                return Err(failed(format!(
                    "El códec de video de la salida es `{actual}` y `{expected}` era el esperado."
                )));
            }
        }
    }

    if matches!(
        plan.audio.action,
        StreamAction::Transcode | StreamAction::Copy
    ) {
        let expected = expected_audio_codec(plan, source);
        let actual = output
            .audio_streams
            .first()
            .map(|stream| stream.codec.as_str());
        if let (Some(expected), Some(actual)) = (expected, actual) {
            if !same_codec(expected, actual) {
                return Err(failed(format!(
                    "El códec de audio de la salida es `{actual}` y `{expected}` era el esperado."
                )));
            }
        }
    }

    if let Some(first) = output.video_streams.first() {
        let max_width = plan.video.max_width;
        let max_height = plan.video.max_height;
        if max_width.is_some_and(|max| first.width > max)
            || max_height.is_some_and(|max| first.height > max)
        {
            return Err(failed(format!(
                "La resolución de la salida ({}×{}) supera el límite del plan ({}×{}).",
                first.width,
                first.height,
                max_width.map_or("sin límite".to_owned(), |max| max.to_string()),
                max_height.map_or("sin límite".to_owned(), |max| max.to_string()),
            )));
        }
    }

    Ok(())
}

/// `max(2 s, 3 %)`, truncated so the tolerance is never wider than declared.
fn duration_tolerance_ms(source_duration_ms: u64) -> u64 {
    (source_duration_ms * DURATION_TOLERANCE_PERCENT / 100).max(MIN_DURATION_TOLERANCE_MS)
}

fn check_family(removed: bool, streams: usize, family: &str) -> Result<(), JobError> {
    if removed && streams > 0 {
        return Err(failed(format!(
            "El plan eliminó {family} y la salida conserva {streams} pista(s)."
        )));
    }
    if !removed && streams == 0 {
        return Err(failed(format!(
            "El plan declaró {family} y la salida no tiene ninguna pista."
        )));
    }
    Ok(())
}

/// Codec the temporary must carry for video: the encoder family when the plan
/// re-encodes, the first source stream when the plan copies.
fn expected_video_codec<'a>(
    plan: &'a ConversionPlan,
    source: &'a MediaSnapshot,
) -> Option<&'a str> {
    match plan.video.action {
        StreamAction::Transcode => plan
            .video
            .encoder
            .as_deref()
            .and_then(video_codec_for_encoder),
        StreamAction::Copy => source
            .video_streams
            .first()
            .map(|stream| stream.codec.as_str()),
        StreamAction::Remove => None,
    }
}

fn expected_audio_codec<'a>(
    plan: &'a ConversionPlan,
    source: &'a MediaSnapshot,
) -> Option<&'a str> {
    match plan.audio.action {
        StreamAction::Transcode => plan
            .audio
            .encoder
            .as_deref()
            .and_then(audio_codec_for_encoder),
        StreamAction::Copy => source
            .audio_streams
            .first()
            .map(|stream| stream.codec.as_str()),
        StreamAction::Remove => None,
    }
}

/// CPU video encoders of v0.1 mapped to the codec they produce.
fn video_codec_for_encoder(encoder: &str) -> Option<&'static str> {
    Some(match encoder {
        "libx264" => "h264",
        "libx265" => "hevc",
        "libsvtav1" => "av1",
        "libvpx-vp9" => "vp9",
        _ => return None,
    })
}

/// CPU audio encoders of v0.1 mapped to the codec they produce.
fn audio_codec_for_encoder(encoder: &str) -> Option<&'static str> {
    Some(match encoder {
        "aac" => "aac",
        "libmp3lame" => "mp3",
        "libopus" => "opus",
        "flac" => "flac",
        "alac" => "alac",
        "pcm_s16le" => "pcm_s16le",
        _ => return None,
    })
}

fn same_codec(expected: &str, actual: &str) -> bool {
    expected.eq_ignore_ascii_case(actual)
}

fn require_absolute(path: &str, label: &str) -> Result<(), JobError> {
    if path.is_empty() || path.contains('\0') || !path.starts_with('/') {
        return Err(invalid(format!(
            "{label} `{path}` no es válida: debe ser una ruta absoluta, sin bytes nulos."
        )));
    }
    Ok(())
}

fn is_safe_job_id(job_id: &str) -> bool {
    !job_id.is_empty()
        && job_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

fn strip_leading_dot(stem: &str) -> &str {
    stem.strip_prefix('.').unwrap_or(stem)
}

fn file_name(stem: &str, extension: &str, suffix: Option<u32>) -> String {
    match suffix {
        Some(suffix) => format!("{stem} ({suffix}).{extension}"),
        None => format!("{stem}.{extension}"),
    }
}

/// Joins a directory and a file name with exactly one separator.
fn join(dir: &str, name: &str) -> String {
    format!("{dir}/{name}")
}

fn taken(candidate: &str, reserved: &[String]) -> bool {
    reserved.iter().any(|path| path == candidate)
}

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::InvalidInput, message, false)
}

fn failed(message: impl Into<String>) -> JobError {
    JobError::new(ErrorCode::OutputValidationFailed, message, true)
}
