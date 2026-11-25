//! Compatibility engine: one `ConversionPlan` per snapshot and preset pair.
//!
//! See `docs/presets.md` ("Resolución en v0.1"), `docs/architecture.md` §11 and
//! the invariants of `docs/contracts.md` §3. The engine is pure — no I/O, no
//! processes, no clock — so the same input always resolves to the same plan.
//! Every decision lands in `reasoning[]`: the UI has to explain why a stream is
//! re-encoded instead of copied.

use crate::error::{ErrorCode, JobError};
use crate::media::{AudioStream, MediaSnapshot, SubtitleKind, SubtitleStream, VideoStream};
use crate::plan::{
    AudioFilter, AudioPlan, ConversionPlan, HdrAction, MetadataPolicy, StreamAction,
    SubtitleAction, SubtitlePlan, VideoPlan,
};
use crate::preset::{HdrPolicy, Preset, SubtitlePolicy};

/// CPU video encoders allowed in v0.1 (`presets.md`), keyed by normalized codec.
const CPU_VIDEO_ENCODERS: [(&str, &str); 4] = [
    ("h264", "libx264"),
    ("h265", "libx265"),
    ("av1", "libsvtav1"),
    ("vp9", "libvpx-vp9"),
];

/// Audio codecs a preset may name, plus `copy` (`presets.md`, "Resolución").
const AUDIO_CODECS: [&str; 6] = ["aac", "mp3", "flac", "alac", "pcm_s16le", "opus"];

/// Resolves the copy/transcode/remove decision for every stream family.
pub fn plan_conversion(
    snapshot: &MediaSnapshot,
    preset: &Preset,
) -> Result<ConversionPlan, JobError> {
    validate_preset(preset)?;

    let mut reasoning = Vec::new();
    let video = plan_video(snapshot, preset, &mut reasoning);
    let audio = plan_audio(snapshot, preset, &mut reasoning);
    let subtitles = plan_subtitles(snapshot, preset, &mut reasoning)?;

    if preset.hardware_preference == "auto" {
        reasoning.push(format!(
            "resolución: el preset `{}` declara `hardware_preference: auto`, \
             pero v0.1 solo tiene encoders CPU; la aceleración por GPU queda para v0.3.",
            preset.id
        ));
    }

    Ok(ConversionPlan {
        container: preset.container.clone(),
        video,
        audio,
        subtitles,
        faststart: preset.faststart,
        metadata: if preset.preserve_metadata {
            MetadataPolicy::Preserve
        } else {
            MetadataPolicy::Strip
        },
        reasoning,
        audio_tracks: Vec::new(),
        subtitle_tracks: Vec::new(),
    })
}

/// Rejects presets that v0.1 cannot honor, before any FFmpeg argv exists.
fn validate_preset(preset: &Preset) -> Result<(), JobError> {
    let invalid = |message: String| Err(JobError::new(ErrorCode::PlanInvalid, message, false));

    if preset.video_codec != "none" {
        let codec = normalized_video_codec(&preset.video_codec);
        if cpu_video_encoder(codec).is_none() {
            return invalid(format!(
                "el preset `{}` pide el códec de video `{}`, sin encoder CPU en v0.1",
                preset.id, preset.video_codec
            ));
        }
        if !container_carries_video(&preset.container, codec) {
            return invalid(format!(
                "el contenedor `{}` del preset `{}` no admite video `{codec}`",
                preset.container, preset.id
            ));
        }
    }
    if preset.audio_codec != "copy"
        && preset.audio_codec != "none"
        && !AUDIO_CODECS.contains(&preset.audio_codec.as_str())
    {
        return invalid(format!(
            "el preset `{}` pide el códec de audio `{}`, fuera del alcance de v0.1",
            preset.id, preset.audio_codec
        ));
    }
    if preset.subtitle_policy == SubtitlePolicy::BurnIn {
        return invalid(format!(
            "el preset `{}` pide subtítulos incrustados (burn-in), no implementado en v0.1",
            preset.id
        ));
    }
    Ok(())
}

fn plan_video(snapshot: &MediaSnapshot, preset: &Preset, reasoning: &mut Vec<String>) -> VideoPlan {
    if preset.video_codec == "none" {
        reasoning.push(format!(
            "video: remove — el preset `{}` es de solo audio (`video_codec: none`).",
            preset.id
        ));
        return removed_video();
    }

    let Some(stream) = snapshot.video_streams.first() else {
        reasoning.push("video: remove — la entrada no declara ninguna pista de video.".to_owned());
        return removed_video();
    };

    let hdr_action = if stream.is_hdr && preset.hdr_policy == HdrPolicy::ToneMapSdr {
        HdrAction::ToneMapSdr
    } else {
        HdrAction::Preserve
    };

    match copy_blocker(stream, preset) {
        None => {
            reasoning.push(format!(
                "video: copy — la pista `{}` {}×{} {} ya es compatible con `{}` en `{}` \
                 dentro de los límites del preset.",
                stream.codec,
                stream.width,
                stream.height,
                stream.pixel_format,
                preset.video_codec,
                preset.container
            ));
            VideoPlan {
                action: StreamAction::Copy,
                encoder: None,
                stream_index: Some(stream.index),
                hdr_action,
                ..removed_video()
            }
        }
        Some(blocker) => {
            let encoder = cpu_video_encoder(normalized_video_codec(&preset.video_codec))
                .expect("validated preset codec");
            let hdr_note = match hdr_action {
                HdrAction::ToneMapSdr => ", con tone mapping HDR→SDR",
                HdrAction::Preserve => "",
            };
            reasoning.push(format!(
                "video: transcode — {blocker}; se recodifica de `{}` a `{}` con `{encoder}` \
                 (CPU, CRF {}){hdr_note}.",
                stream.codec,
                preset.video_codec,
                preset
                    .video_quality
                    .map_or_else(|| "por defecto".to_owned(), |crf| crf.to_string())
            ));
            VideoPlan {
                action: StreamAction::Transcode,
                encoder: Some(encoder.to_owned()),
                quality: preset.video_quality,
                max_width: preset.max_width,
                max_height: preset.max_height,
                fps: preset.max_fps,
                pixel_format: preset.pixel_format.clone(),
                profile: preset.profile.clone(),
                level: preset.level,
                hdr_action,
                stream_index: Some(stream.index),
            }
        }
    }
}

/// A `remove` plan carries no stream, no encoder and no target limits.
fn removed_video() -> VideoPlan {
    VideoPlan {
        action: StreamAction::Remove,
        encoder: None,
        quality: None,
        max_width: None,
        max_height: None,
        fps: None,
        pixel_format: None,
        profile: None,
        level: None,
        hdr_action: HdrAction::Preserve,
        stream_index: None,
    }
}

/// Returns the first condition that forbids a stream copy, in preset order.
fn copy_blocker(stream: &VideoStream, preset: &Preset) -> Option<String> {
    let source = normalized_video_codec(&stream.codec);
    let target = normalized_video_codec(&preset.video_codec);
    if source != target {
        return Some(format!(
            "el códec de origen es `{}` y el destino pide `{}`",
            stream.codec, preset.video_codec
        ));
    }
    if !container_carries_video(&preset.container, target) {
        return Some(format!(
            "`{}` no transporta video `{target}`",
            preset.container
        ));
    }
    if let Some(max_width) = preset.max_width {
        if stream.width > max_width {
            return Some(format!(
                "el ancho {} supera el máximo {max_width}",
                stream.width
            ));
        }
    }
    if let Some(max_height) = preset.max_height {
        if stream.height > max_height {
            return Some(format!(
                "el alto {} supera el máximo {max_height}",
                stream.height
            ));
        }
    }
    if let Some(max_fps) = preset.max_fps {
        if stream.fps > max_fps {
            return Some(format!(
                "{:.3} fps supera el máximo {max_fps} fps",
                stream.fps
            ));
        }
    }
    if let Some(max_bit_depth) = preset.max_bit_depth {
        if stream.bit_depth > max_bit_depth {
            return Some(format!(
                "la profundidad de {} bits supera el máximo de {max_bit_depth} bits",
                stream.bit_depth
            ));
        }
    }
    if let Some(pixel_format) = &preset.pixel_format {
        if stream.pixel_format != *pixel_format {
            return Some(format!(
                "el formato de píxel `{}` no es `{pixel_format}`",
                stream.pixel_format
            ));
        }
    }
    if stream.is_hdr && preset.hdr_policy != HdrPolicy::Preserve {
        return Some("la fuente es HDR y el preset pide tone mapping a SDR".to_owned());
    }
    None
}

fn plan_audio(snapshot: &MediaSnapshot, preset: &Preset, reasoning: &mut Vec<String>) -> AudioPlan {
    // `audio_codec: none` drops the soundtrack (`docs/contracts.md` §2.2).
    if preset.audio_codec == "none" {
        reasoning.push(format!(
            "audio: remove — el preset `{}` descarta el audio (`audio_codec: none`).",
            preset.id
        ));
        return removed_audio();
    }

    let Some(stream) = select_stream(
        &snapshot.audio_streams,
        preset,
        "audio",
        reasoning,
        &|stream: &AudioStream| stream.index,
        &|stream: &AudioStream| stream.language.as_deref(),
        &|stream: &AudioStream| stream.is_default,
        &|stream: &AudioStream| {
            format!(
                "{}, {} canales",
                stream.language.as_deref().unwrap_or("sin idioma"),
                stream.channels
            )
        },
    ) else {
        reasoning.push("audio: remove — la entrada no declara ninguna pista de audio.".to_owned());
        return removed_audio();
    };

    if preset.audio_codec == "copy" {
        reasoning.push(format!(
            "audio: copy — el preset `{}` conserva el audio original (`{}`, {} canales).",
            preset.id, stream.codec, stream.channels
        ));
        return copied_audio(stream);
    }

    if stream.codec == preset.audio_codec {
        match preset.audio_channels {
            Some(max_channels) if stream.channels > max_channels => {}
            Some(max_channels) => {
                if stream.channels < max_channels {
                    reasoning.push(format!(
                        "audio: copy — la fuente ya es `{}` con {} canal(es) y el preset pide \
                         {max_channels}; no se hace upmix ni remuestreo silencioso en v0.1.",
                        stream.codec, stream.channels
                    ));
                } else {
                    reasoning.push(format!(
                        "audio: copy — la fuente ya es `{}` con {} canal(es), compatible con el preset.",
                        stream.codec, stream.channels
                    ));
                }
                return copied_audio(stream);
            }
            None => {
                reasoning.push(format!(
                    "audio: copy — la fuente ya es `{}` y el preset no limita canales.",
                    stream.codec
                ));
                return copied_audio(stream);
            }
        }
    }

    let why = if stream.codec != preset.audio_codec {
        format!(
            "el códec de origen es `{}` y el destino pide `{}`",
            stream.codec, preset.audio_codec
        )
    } else {
        format!(
            "la fuente tiene {} canales y el preset admite {}",
            stream.channels,
            preset.audio_channels.unwrap_or(stream.channels)
        )
    };
    let channels_note = preset
        .audio_channels
        .map_or_else(String::new, |channels| format!(" y {channels} canales"));
    let encoder = cpu_audio_encoder(&preset.audio_codec)
        .expect("validate_preset rejects unsupported audio codecs");
    reasoning.push(format!(
        "audio: transcode — {why}; se recodifica a `{}`{channels_note} con `{encoder}`.",
        preset.audio_codec
    ));

    let filter = downmix_filter(stream, preset);
    if filter.is_some() {
        reasoning.push(format!(
            "audio: downmix — la fuente tiene {} canales y el destino es estéreo; se aplica 
             `pan=stereo` con realce del canal central (diálogo) para que la voz no se pierda en 
             parlantes de dos canales.",
            stream.channels
        ));
    }

    AudioPlan {
        action: StreamAction::Transcode,
        encoder: Some(encoder.to_owned()),
        bitrate: preset.audio_bitrate.clone(),
        channels: preset.audio_channels,
        stream_index: Some(stream.index),
        normalize: preset.normalize_audio,
        filter,
    }
}

/// A stereo target fed by a >= 6 channel source loses the centre channel unless
/// the downmix is explicit (`docs/architecture-phases-0-and-1.md` §2 BUG-001).
fn downmix_filter(stream: &AudioStream, preset: &Preset) -> Option<AudioFilter> {
    match preset.audio_channels {
        Some(2) if stream.channels >= 6 => Some(AudioFilter::StereoDialogueBoost),
        _ => None,
    }
}

/// Documented priority order for every stream family: the preset's preferred
/// language, then the default disposition, then the first track.
///
/// v0.1 took `.first()` unconditionally, so an international release always lost
/// the user's language without saying so
/// (`docs/architecture-phases-0-and-1.md` §2 BUG-002).
#[allow(clippy::too_many_arguments)]
fn select_stream<'a, T>(
    streams: &'a [T],
    preset: &Preset,
    family: &str,
    reasoning: &mut Vec<String>,
    index: &dyn Fn(&T) -> u32,
    language: &dyn Fn(&T) -> Option<&str>,
    is_default: &dyn Fn(&T) -> bool,
    describe: &dyn Fn(&T) -> String,
) -> Option<&'a T> {
    let first = streams.first()?;
    let preferred = preset
        .preferred_audio_language
        .as_deref()
        .and_then(normalized_language);

    let (chosen, why) = match &preferred {
        Some(preferred) => {
            let matched = streams.iter().find(|stream| {
                language(stream).and_then(normalized_language).as_deref()
                    == Some(preferred.as_str())
            });
            match matched {
                Some(stream) => (
                    stream,
                    format!("por coincidir con el idioma preferido `{preferred}`"),
                ),
                None => match streams.iter().find(|stream| is_default(stream)) {
                    Some(stream) => (
                        stream,
                        format!("por ser la pista por defecto; ninguna coincide con `{preferred}`"),
                    ),
                    None => (
                        first,
                        format!("por ser la primera pista; ninguna coincide con `{preferred}`"),
                    ),
                },
            }
        }
        None => match streams.iter().find(|stream| is_default(stream)) {
            Some(stream) => (stream, "por ser la pista por defecto".to_owned()),
            None => (first, "por ser la primera pista".to_owned()),
        },
    };

    // A single track needs no explanation; a real choice always leaves a trail.
    if streams.len() > 1 || preferred.is_some() {
        reasoning.push(format!(
            "{family}: selección — pista {} ({}) {why}.",
            index(chosen),
            describe(chosen)
        ));
    }
    Some(chosen)
}

/// ISO 639 aliases the selector folds together: every three-letter code maps to
/// its two-letter equivalent, including the legacy bibliographic forms (`fre`,
/// `ger`, `dut`, …). A region subtag is dropped before comparing, so `es-419`,
/// `es-ES` and `spa` all mean Spanish (`docs/architecture-phases-0-and-1.md` §2
/// BUG-002).
const LANGUAGE_ALIASES: [(&str, &str); 40] = [
    ("eng", "en"),
    ("spa", "es"),
    ("fra", "fr"),
    ("fre", "fr"),
    ("deu", "de"),
    ("ger", "de"),
    ("ita", "it"),
    ("jpn", "ja"),
    ("por", "pt"),
    ("zho", "zh"),
    ("chi", "zh"),
    ("kor", "ko"),
    ("rus", "ru"),
    ("ara", "ar"),
    ("hin", "hi"),
    ("tur", "tr"),
    ("pol", "pl"),
    ("nld", "nl"),
    ("dut", "nl"),
    ("swe", "sv"),
    ("nor", "no"),
    ("dan", "da"),
    ("fin", "fi"),
    ("ces", "cs"),
    ("cze", "cs"),
    ("ell", "el"),
    ("gre", "el"),
    ("heb", "he"),
    ("tha", "th"),
    ("vie", "vi"),
    ("ukr", "uk"),
    ("ron", "ro"),
    ("rum", "ro"),
    ("hun", "hu"),
    ("ind", "id"),
    ("msa", "ms"),
    ("may", "ms"),
    ("cat", "ca"),
    ("fas", "fa"),
    ("per", "fa"),
];

/// Two-letter form of `code`, or `None` when it names no known language.
fn normalized_language(code: &str) -> Option<String> {
    let lowered = code.trim().to_ascii_lowercase();
    let base = lowered.split(['-', '_']).next().unwrap_or_default();
    if base.is_empty() || matches!(base, "und" | "unk" | "unknown" | "zxx" | "mul") {
        return None;
    }
    let canonical = LANGUAGE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == base)
        .or_else(|| {
            LANGUAGE_ALIASES
                .iter()
                .find(|(_, canonical)| *canonical == base)
        })
        .map(|(_, canonical)| *canonical)
        .unwrap_or(base);
    Some(canonical.to_owned())
}

fn copied_audio(stream: &AudioStream) -> AudioPlan {
    AudioPlan {
        action: StreamAction::Copy,
        encoder: None,
        bitrate: None,
        channels: None,
        stream_index: Some(stream.index),
        normalize: false,
        filter: None,
    }
}

fn removed_audio() -> AudioPlan {
    AudioPlan {
        action: StreamAction::Remove,
        encoder: None,
        bitrate: None,
        channels: None,
        stream_index: None,
        normalize: false,
        filter: None,
    }
}

fn plan_subtitles(
    snapshot: &MediaSnapshot,
    preset: &Preset,
    reasoning: &mut Vec<String>,
) -> Result<SubtitlePlan, JobError> {
    if preset.subtitle_policy == SubtitlePolicy::Remove {
        reasoning.push(format!(
            "subtítulos: remove — el preset `{}` no transporta subtítulos (`subtitle_policy: remove`).",
            preset.id
        ));
        return Ok(removed_subtitles());
    }

    let Some(selected) = select_stream(
        &snapshot.subtitle_streams,
        preset,
        "subtítulos",
        reasoning,
        &|stream: &SubtitleStream| stream.index,
        &|stream: &SubtitleStream| stream.language.as_deref(),
        &|stream: &SubtitleStream| stream.is_default,
        &|stream: &SubtitleStream| {
            stream
                .language
                .clone()
                .unwrap_or_else(|| "sin idioma".to_owned())
        },
    ) else {
        reasoning
            .push("subtítulos: remove — la entrada no declara pistas de subtítulos.".to_owned());
        return Ok(removed_subtitles());
    };

    let Some(target) = target_subtitle_codec(&preset.container) else {
        reasoning.push(format!(
            "subtítulos: remove — el contenedor `{}` no admite subtítulos aunque el preset `{}` los solicitaba; se descartan para no interrumpir la conversión.",
            preset.container, preset.id
        ));
        return Ok(removed_subtitles());
    };

    if container_carries_subtitle(&preset.container, &selected.codec) {
        reasoning.push(format!(
            "subtítulos: copy — la pista seleccionada ya es compatible con `{}`.",
            preset.container
        ));
        return Ok(SubtitlePlan {
            action: SubtitleAction::Copy,
            encoder: None,
            stream_index: Some(selected.index),
            source_index: None,
        });
    }

    if selected.kind == SubtitleKind::Bitmap {
        return Ok(removed_bitmap_subtitles(selected, preset, reasoning));
    }

    reasoning.push(format!(
        "subtítulos: transcode — `{}` no transporta `{}`; las pistas de texto se convierten a `{target}`.",
        preset.container, selected.codec
    ));
    if matches!(selected.codec.as_str(), "ass" | "ssa") && target == "mov_text" {
        reasoning.push(format!(
            "subtítulos: aviso — los estilos visuales de `{}` no se conservan al convertir a `mov_text`.",
            selected.codec
        ));
    }
    Ok(SubtitlePlan {
        action: SubtitleAction::Transcode,
        encoder: Some(target.to_owned()),
        stream_index: None,
        source_index: Some(selected.index),
    })
}

/// v0.1 has no OCR: a bitmap track that needs to become text is dropped, not degraded.
fn removed_bitmap_subtitles(
    stream: &SubtitleStream,
    preset: &Preset,
    reasoning: &mut Vec<String>,
) -> SubtitlePlan {
    reasoning.push(format!(
        "subtítulos: remove — la pista `{}` es incompatible con el contenedor `{}`; v0.1 no tiene OCR y el quemado (burn-in) no está disponible en la Fase 0.",
        stream.codec, preset.container
    ));
    removed_subtitles()
}

fn removed_subtitles() -> SubtitlePlan {
    SubtitlePlan {
        action: SubtitleAction::Remove,
        encoder: None,
        stream_index: None,
        source_index: None,
    }
}

/// `hevc`/`h265` and `h264`/`avc` name the same family as far as a copy goes.
fn normalized_video_codec(codec: &str) -> &str {
    match codec {
        "hevc" | "h265" => "h265",
        "avc" | "h264" => "h264",
        other => other,
    }
}

fn cpu_audio_encoder(codec: &str) -> Option<&'static str> {
    match codec {
        "aac" => Some("aac"),
        "mp3" => Some("libmp3lame"),
        "flac" => Some("flac"),
        "alac" => Some("alac"),
        "pcm_s16le" => Some("pcm_s16le"),
        "opus" => Some("libopus"),
        _ => None,
    }
}

fn cpu_video_encoder(codec: &str) -> Option<&'static str> {
    CPU_VIDEO_ENCODERS
        .iter()
        .find(|(family, _)| *family == codec)
        .map(|(_, encoder)| *encoder)
}

pub(crate) fn container_carries_video(container: &str, codec: &str) -> bool {
    match container {
        "mp4" => matches!(codec, "h264" | "h265" | "av1"),
        "mkv" => matches!(codec, "h264" | "h265" | "av1" | "vp9"),
        "webm" => matches!(codec, "vp9" | "av1"),
        _ => false,
    }
}

/// Whether `container` can carry `codec` audio. It is the table the plan engine
/// and [`crate::preset::CustomPreset::validate`] share, so a custom preset and a
/// system preset can never disagree about what a container accepts.
pub(crate) fn container_carries_audio(container: &str, codec: &str) -> bool {
    match container {
        "mp4" | "m4a" => matches!(codec, "aac" | "mp3" | "alac"),
        "mkv" => matches!(
            codec,
            "aac" | "mp3" | "opus" | "flac" | "alac" | "pcm_s16le"
        ),
        "webm" => codec == "opus",
        _ => false,
    }
}

fn container_carries_subtitle(container: &str, codec: &str) -> bool {
    match container {
        "mp4" | "m4a" => codec == "mov_text",
        "mkv" => matches!(
            codec,
            "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" | "dvd_subtitle" | "hdmv_pgs_subtitle"
        ),
        "webm" => codec == "webvtt",
        _ => false,
    }
}

/// Text codec a container needs when subtitles have to be re-encoded.
fn target_subtitle_codec(container: &str) -> Option<&'static str> {
    match container {
        "mp4" | "m4a" => Some("mov_text"),
        "mkv" => Some("srt"),
        "webm" => Some("webvtt"),
        _ => None,
    }
}
