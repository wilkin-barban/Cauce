//! `Preset`: identity, presentation and constraints of one conversion target.
//!
//! See `docs/contracts.md` §2.3 and `docs/presets.md`. The 13 system presets are
//! data seeded from the canonical catalog; this module only defines the shape.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name_es: String,
    pub name_en: String,
    pub name_pt_br: String,
    pub description_es: String,
    pub description_en: String,
    pub description_pt_br: String,
    pub category: String,
    pub device_family: DeviceFamily,
    pub container: String,
    /// `none` for audio-only presets, which remove the video stream.
    pub video_codec: String,
    pub audio_codec: String,
    /// CRF for the CPU encoders supported in v0.1.
    pub video_quality: Option<u8>,
    /// FFmpeg bitrate token such as `192k`.
    pub audio_bitrate: Option<String>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_fps: Option<f64>,
    pub pixel_format: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub max_bit_depth: Option<u8>,
    pub hdr_policy: HdrPolicy,
    pub subtitle_policy: SubtitlePolicy,
    pub faststart: bool,
    pub preserve_metadata: bool,
    /// Kept as metadata in v0.1: every preset resolves to a CPU encoder.
    pub hardware_preference: String,
    pub audio_channels: Option<u8>,
    pub normalize_audio: bool,
    pub preferred_audio_language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceFamily {
    Generic,
    Samsung,
    Lg,
    AndroidTv,
    Web,
    Archive,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HdrPolicy {
    Preserve,
    ToneMapSdr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubtitlePolicy {
    PreserveCompatible,
    ConvertCompatible,
    Remove,
    BurnIn,
}

// ---------------------------------------------------------------------------
// The one custom preset (`docs/contracts.md` §2.3, `docs/data-model.md` §2.5).
//
// Cauce ships exactly one: saving replaces it and there is no generic editor.
// It is persisted as JSON under the `custom_preset` key of `settings`, and
// [`CustomPreset::to_preset`] turns it into the same `Preset` the thirteen system
// presets use, so `media.plan`, `jobs.add` and the executor need no special case.
// ---------------------------------------------------------------------------

use crate::compat::{container_carries_audio, container_carries_video};
use crate::error::{ErrorCode, JobError};

/// A custom preset that no rule rejects. The fields are exactly the seven the
/// editor offers and nothing else (`docs/contracts.md` §2.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomPreset {
    pub container: CustomContainer,
    pub video: CustomVideo,
    /// CRF 0-51 or one of the three named levels, resolved per encoder.
    pub quality: CustomQuality,
    pub resolution: CustomResolution,
    pub audio: CustomAudio,
    /// `None` when no bitrate applies: `audio: copy` and `audio: none`.
    pub bitrate: Option<CustomBitrate>,
    pub subtitles: CustomSubtitles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomContainer {
    Mp4,
    Mkv,
    Webm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomVideo {
    #[serde(rename = "h264")]
    H264,
    #[serde(rename = "h265")]
    H265,
    #[serde(rename = "vp9")]
    Vp9,
    #[serde(rename = "av1")]
    Av1,
    Copy,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomAudio {
    Aac,
    Mp3,
    Opus,
    Flac,
    Copy,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomSubtitles {
    Keep,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomResolution {
    Original,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "480p")]
    P480,
}

/// The three named quality levels; each resolves to a CRF per encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomQualityLevel {
    High,
    Medium,
    Low,
}

/// The audio bitrates the editor offers, in kbps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum CustomBitrate {
    K128,
    K192,
    K256,
    K320,
}

/// A CRF or a named level. On the wire it is `20` or `"high"`, so a client never
/// has to guess which encoder the level will be resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "QualityRepr", into = "QualityRepr")]
pub enum CustomQuality {
    Crf(u8),
    Level(CustomQualityLevel),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum QualityRepr {
    Crf(u8),
    Level(CustomQualityLevel),
}

impl CustomContainer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
        }
    }
}

impl CustomVideo {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
            Self::Vp9 => "vp9",
            Self::Av1 => "av1",
            Self::Copy => "copy",
            Self::None => "none",
        }
    }

    /// The codec a stream would carry; `None` for `copy` and for `none`, which
    /// name an action instead of a codec.
    pub const fn codec(self) -> Option<&'static str> {
        match self {
            Self::Copy | Self::None => None,
            other => Some(other.as_str()),
        }
    }
}

impl CustomAudio {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Flac => "flac",
            Self::Copy => "copy",
            Self::None => "none",
        }
    }

    pub const fn codec(self) -> Option<&'static str> {
        match self {
            Self::Copy | Self::None => None,
            other => Some(other.as_str()),
        }
    }
}

impl CustomResolution {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
            Self::P480 => "480p",
        }
    }

    /// The `max_width`/`max_height` pair of `Preset`; `original` sets no limit.
    pub const fn max_dimensions(self) -> (Option<u32>, Option<u32>) {
        match self {
            Self::Original => (None, None),
            Self::P1080 => (Some(1920), Some(1080)),
            Self::P720 => (Some(1280), Some(720)),
            Self::P480 => (Some(854), Some(480)),
        }
    }
}

impl CustomBitrate {
    /// Every bitrate the editor offers, in ascending order.
    pub const ALL: [Self; 4] = [Self::K128, Self::K192, Self::K256, Self::K320];

    pub const fn kbps(self) -> u16 {
        match self {
            Self::K128 => 128,
            Self::K192 => 192,
            Self::K256 => 256,
            Self::K320 => 320,
        }
    }

    /// The FFmpeg token `Preset::audio_bitrate` carries, such as `192k`.
    pub fn token(self) -> String {
        format!("{}k", self.kbps())
    }
}

impl TryFrom<u16> for CustomBitrate {
    type Error = String;

    fn try_from(kbps: u16) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|bitrate| bitrate.kbps() == kbps)
            .ok_or_else(|| format!("`{kbps}` no es un bitrate de 128, 192, 256 ni 320 kbps"))
    }
}

impl From<CustomBitrate> for u16 {
    fn from(bitrate: CustomBitrate) -> Self {
        bitrate.kbps()
    }
}

impl CustomQualityLevel {
    /// The three levels, from the best quality to the smallest file.
    pub const ALL: [Self; 3] = [Self::High, Self::Medium, Self::Low];
}

impl CustomQuality {
    /// The CRF this quality means for `video`; `None` when the stream is copied
    /// or removed, because neither carries a quality of its own.
    pub const fn crf_for(self, video: CustomVideo) -> Option<u8> {
        match self {
            Self::Crf(crf) => match video.codec() {
                Some(_) => Some(crf),
                None => None,
            },
            Self::Level(level) => match video {
                CustomVideo::H264 => Some(match level {
                    CustomQualityLevel::High => 18,
                    CustomQualityLevel::Medium => 23,
                    CustomQualityLevel::Low => 28,
                }),
                CustomVideo::H265 => Some(match level {
                    CustomQualityLevel::High => 20,
                    CustomQualityLevel::Medium => 26,
                    CustomQualityLevel::Low => 30,
                }),
                CustomVideo::Vp9 => Some(match level {
                    CustomQualityLevel::High => 24,
                    CustomQualityLevel::Medium => 32,
                    CustomQualityLevel::Low => 38,
                }),
                CustomVideo::Av1 => Some(match level {
                    CustomQualityLevel::High => 24,
                    CustomQualityLevel::Medium => 30,
                    CustomQualityLevel::Low => 36,
                }),
                CustomVideo::Copy | CustomVideo::None => None,
            },
        }
    }
}

/// The CRF scale of `presets.md`: every CPU encoder in v0.1 accepts 0-51.
pub const CRF_RANGE: std::ops::RangeInclusive<u8> = 0..=51;

impl TryFrom<QualityRepr> for CustomQuality {
    type Error = String;

    fn try_from(repr: QualityRepr) -> Result<Self, Self::Error> {
        match repr {
            QualityRepr::Crf(crf) if CRF_RANGE.contains(&crf) => Ok(Self::Crf(crf)),
            QualityRepr::Crf(crf) => Err(format!(
                "el CRF `{crf}` queda fuera de 0-{}, el rango de `presets.md`",
                CRF_RANGE.end()
            )),
            QualityRepr::Level(level) => Ok(Self::Level(level)),
        }
    }
}

impl From<CustomQuality> for QualityRepr {
    fn from(quality: CustomQuality) -> Self {
        match quality {
            CustomQuality::Crf(crf) => Self::Crf(crf),
            CustomQuality::Level(level) => Self::Level(level),
        }
    }
}

impl CustomPreset {
    /// The only id a custom preset ever has: there is exactly one.
    pub const ID: &'static str = "custom-1";

    /// Rejects every combination the selected container cannot carry.
    ///
    /// The rules and why they exist:
    ///
    /// 1. `container` must carry `video` when it is a real codec, using the same
    ///    table the plan engine uses (`presets.md`): MP4 has no VP9, WebM has no
    ///    H.264/H.265.
    /// 2. `container` must carry `audio` for the same reason: WebM only carries
    ///    Opus among the codecs Cauce offers.
    /// 3. A `bitrate` with `audio: copy` contradicts itself (a copy keeps the
    ///    original bitrate) and with `audio: none` it has no track to describe.
    /// 4. `subtitles: keep` needs both a picture and a soundtrack: without
    ///    `video` there is nothing to attach them to, and without `audio` the
    ///    result would be a silent container whose only payload is text. v0.1 has
    ///    no "subtitles only" target, so both cases are refused instead of
    ///    producing a file the presets never promise.
    pub fn validate(&self) -> Result<(), JobError> {
        if let Some(codec) = self.video.codec() {
            if !container_carries_video(self.container.as_str(), codec) {
                return Err(invalid(format!(
                    "el contenedor `{}` no transporta video `{codec}`: elegí otro contenedor o \
                     otro códec de video",
                    self.container.as_str()
                )));
            }
        }
        if let Some(codec) = self.audio.codec() {
            if !container_carries_audio(self.container.as_str(), codec) {
                return Err(invalid(format!(
                    "el contenedor `{}` no transporta audio `{codec}`: elegí otro contenedor o \
                     otro códec de audio",
                    self.container.as_str()
                )));
            }
        }
        if let Some(bitrate) = self.bitrate {
            let reason = match self.audio {
                CustomAudio::None => "no hay pista de audio que codificar",
                CustomAudio::Copy => "la pista original conserva su propio bitrate",
                _ => "",
            };
            if !reason.is_empty() {
                return Err(invalid(format!(
                    "un `bitrate` de {} kbps no aplica con `audio: {}`: {reason}",
                    bitrate.kbps(),
                    self.audio.as_str()
                )));
            }
        }
        if self.subtitles == CustomSubtitles::Keep {
            let reason = match (self.video, self.audio) {
                (CustomVideo::None, _) => {
                    Some("sin pista de video no hay imagen a la que asociar los subtítulos")
                }
                (_, CustomAudio::None) => Some("sin pista de audio el resultado sería solo texto"),
                _ => None,
            };
            if let Some(reason) = reason {
                return Err(invalid(format!(
                    "`subtitles: keep` necesita audio y video: {reason}; elegí `subtitles: remove`"
                )));
            }
        }
        Ok(())
    }

    /// Converts the custom preset into the `Preset` type, validating first, so an
    /// invalid combination can never reach `media.plan`, `jobs.add` or FFmpeg.
    pub fn to_preset(&self) -> Result<Preset, JobError> {
        self.validate()?;
        let (max_width, max_height) = self.resolution.max_dimensions();
        // A copied or removed track carries no bitrate: `Preset::audio_bitrate`
        // only names the bitrate of a re-encode.
        let audio_bitrate = self
            .audio
            .codec()
            .map(|_| self.bitrate.unwrap_or(CustomBitrate::K192).token());
        Ok(Preset {
            id: Self::ID.to_owned(),
            name_es: "Personalizado".to_owned(),
            name_en: "Custom".to_owned(),
            name_pt_br: "Personalizado".to_owned(),
            description_es: "Tu propio preset: contenedor, códecs, calidad y resolución elegidos \
                             por vos."
                .to_owned(),
            description_en: "Your own preset: container, codecs, quality and resolution chosen \
                             by you."
                .to_owned(),
            description_pt_br: "Seu próprio preset: contêiner, codecs, qualidade e resolução \
                                escolhidos por você."
                .to_owned(),
            // `custom` is the fifth group of the Presets page (`docs/ui-mvp.md` §6).
            category: "custom".to_owned(),
            device_family: DeviceFamily::Generic,
            container: self.container.as_str().to_owned(),
            video_codec: self.video.as_str().to_owned(),
            audio_codec: self.audio.as_str().to_owned(),
            video_quality: self.quality.crf_for(self.video),
            audio_bitrate,
            max_width,
            max_height,
            max_fps: None,
            pixel_format: None,
            profile: None,
            level: None,
            max_bit_depth: None,
            // The custom preset adds no policy the user did not ask for.
            hdr_policy: HdrPolicy::Preserve,
            subtitle_policy: match self.subtitles {
                CustomSubtitles::Keep => SubtitlePolicy::PreserveCompatible,
                CustomSubtitles::Remove => SubtitlePolicy::Remove,
            },
            // `presets.md`: faststart only has an effect in MP4/M4A.
            faststart: self.container == CustomContainer::Mp4,
            preserve_metadata: true,
            hardware_preference: "cpu".to_owned(),
            audio_channels: None,
            normalize_audio: false,
            preferred_audio_language: None,
        })
    }
}

/// A rejected combination is a request error, never retryable.
fn invalid(message: String) -> JobError {
    JobError::new(ErrorCode::InvalidInput, message, false)
}
