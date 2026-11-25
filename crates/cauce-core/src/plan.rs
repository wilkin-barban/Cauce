//! `ConversionPlan`: the frozen decision of what to copy and what to re-encode.
//!
//! See `docs/contracts.md` §2.2. A plan is computed once, stored on the job and
//! replayed verbatim, so every field here is part of the persisted contract.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionPlan {
    pub container: String,
    pub video: VideoPlan,
    pub audio: AudioPlan,
    pub subtitles: SubtitlePlan,
    /// Only meaningful for MP4 and M4A destinations.
    pub faststart: bool,
    pub metadata: MetadataPolicy,
    /// Human-readable explanation of every copy/transcode decision.
    pub reasoning: Vec<String>,
    /// Every audio track of the output, in output order.
    ///
    /// Empty in a plan written by 0.1.x, which only had the single-track
    /// `audio` view; [`ConversionPlan::primary_audio`] covers that case.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_tracks: Vec<AudioTrackPlan>,
    /// Every subtitle track of the output; a burned-in track lives here with no
    /// output index. Empty in a plan written by 0.1.x.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitle_tracks: Vec<SubtitleTrackPlan>,
}

impl ConversionPlan {
    /// The track the legacy single-track fields describe, if the plan keeps one.
    ///
    /// Phase 0 plans carry no track list, so the view is derived from `audio`;
    /// phase 1 plans carry both and the list wins.
    pub fn primary_audio(&self) -> Option<AudioTrackPlan> {
        if let Some(track) = self.audio_tracks.first() {
            return Some(track.clone());
        }
        if self.audio.action == StreamAction::Remove {
            return None;
        }
        Some(AudioTrackPlan {
            source_index: self.audio.stream_index?,
            output_index: 0,
            action: self.audio.action,
            encoder: self.audio.encoder.clone(),
            bitrate: self.audio.bitrate.clone(),
            channels: self.audio.channels.unwrap_or(0),
            filter: self.audio.filter,
            language: None,
            title: None,
            is_default: true,
        })
    }

    /// Same as [`ConversionPlan::primary_audio`] for subtitles.
    pub fn primary_subtitles(&self) -> Option<SubtitleTrackPlan> {
        if let Some(track) = self.subtitle_tracks.first() {
            return Some(track.clone());
        }
        match self.subtitles.action {
            SubtitleAction::Remove | SubtitleAction::BurnIn => None,
            SubtitleAction::Copy | SubtitleAction::Transcode => Some(SubtitleTrackPlan {
                source_index: self
                    .subtitles
                    .source_index
                    .or(self.subtitles.stream_index)?,
                output_index: Some(0),
                action: self.subtitles.action,
                encoder: self.subtitles.encoder.clone(),
                language: None,
                title: None,
                is_default: true,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoPlan {
    pub action: StreamAction,
    pub encoder: Option<String>,
    /// CRF for the CPU encoders supported in v0.1.
    pub quality: Option<u8>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub fps: Option<f64>,
    pub pixel_format: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub hdr_action: HdrAction,
    pub stream_index: Option<u32>,
    /// Curve used when `hdr_action` maps HDR. `None` keeps the phase 0 chain
    /// (`hable`), and the field is skipped when absent so a plan written by
    /// 0.1.x still serializes byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone_mapping: Option<ToneMappingOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioPlan {
    pub action: StreamAction,
    pub encoder: Option<String>,
    /// FFmpeg bitrate token such as `192k`.
    pub bitrate: Option<String>,
    pub channels: Option<u8>,
    pub stream_index: Option<u32>,
    pub normalize: bool,
    /// Audio filter the builder is allowed to emit, if any.
    ///
    /// Absent from every plan written before phase 0, and skipped when absent,
    /// so a persisted v0.1 plan still round-trips byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<AudioFilter>,
}

/// The audio filters the builder may emit.
///
/// v0.1 refused every audio filter, which left no way to fix a centre channel
/// lost in a downmix. Phase 0 opens exactly this enum — never free-form filter
/// text — so the allowlist stays closed (`docs/architecture-phases-0-and-1.md`
/// §2 BUG-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFilter {
    /// Fold a >= 6 channel source into stereo with a front-centre boost, so the
    /// dialogue stays intelligible on laptops, phones and headphones.
    StereoDialogueBoost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitlePlan {
    pub action: SubtitleAction,
    pub encoder: Option<String>,
    pub stream_index: Option<u32>,
    pub source_index: Option<u32>,
}

/// Action applied to one stream family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamAction {
    Copy,
    Transcode,
    Remove,
}

/// Subtitle action, including the burn-in case that v0.1 does not implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleAction {
    Copy,
    Transcode,
    Remove,
    BurnIn,
}

/// How HDR content is handled when the target is not HDR capable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HdrAction {
    Preserve,
    ToneMapSdr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataPolicy {
    Preserve,
    Strip,
}

// ---------------------------------------------------------------------------
// Explicit selection and custom options (phase 1, `docs/architecture-phases-0-and-1.md`
// §3.1.1). These types are the shape of `track_selection` and `custom_options`
// on the wire; absence of the whole field means "the preset decides".
// ---------------------------------------------------------------------------

/// What the user wants converted, as explicit choices.
///
/// `audio_track_indices` are input stream indices in the order they must be
/// included, not output order. An empty list is only legal for
/// [`AudioMixMode::Passthrough`], where it means "keep whatever the preset
/// picks" — the 0.1.x behaviour this field has to preserve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "TrackSelectionRepr")]
pub struct TrackSelection {
    pub audio_track_indices: Vec<u32>,
    pub subtitle_track_index: Option<u32>,
    pub subtitle_mode: SubtitleMode,
    pub audio_mix_mode: AudioMixMode,
}

/// Wire shape of [`TrackSelection`]. The public type is built through `TryFrom`
/// so the cross-field rule runs at the edge, next to the rest of the contract
/// validation, instead of surfacing later as an impossible plan.
#[derive(Deserialize)]
struct TrackSelectionRepr {
    #[serde(default)]
    audio_track_indices: Vec<u32>,
    #[serde(default)]
    subtitle_track_index: Option<u32>,
    subtitle_mode: SubtitleMode,
    audio_mix_mode: AudioMixMode,
}

/// Rejection of a `track_selection` that cannot describe a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSelectionRejected(pub String);

impl std::fmt::Display for TrackSelectionRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TrackSelectionRejected {}

impl TryFrom<TrackSelectionRepr> for TrackSelection {
    type Error = TrackSelectionRejected;

    fn try_from(repr: TrackSelectionRepr) -> Result<Self, Self::Error> {
        if repr.audio_mix_mode != AudioMixMode::Passthrough && repr.audio_track_indices.is_empty() {
            return Err(TrackSelectionRejected(format!(
                "audio_track_indices must name at least one source track when the mix mode is \
                 `{:?}`; only `Passthrough` may leave the list empty",
                repr.audio_mix_mode
            )));
        }
        if repr.audio_track_indices.len() > MAX_AUDIO_TRACKS {
            return Err(TrackSelectionRejected(format!(
                "audio_track_indices accepts at most {MAX_AUDIO_TRACKS} tracks, got {}",
                repr.audio_track_indices.len()
            )));
        }
        let mut seen = repr.audio_track_indices.clone();
        seen.sort_unstable();
        let duplicates = seen.windows(2).any(|pair| pair[0] == pair[1]);
        if duplicates {
            return Err(TrackSelectionRejected(
                "audio_track_indices must not repeat a source track".to_owned(),
            ));
        }
        Ok(Self {
            audio_track_indices: repr.audio_track_indices,
            subtitle_track_index: repr.subtitle_track_index,
            subtitle_mode: repr.subtitle_mode,
            audio_mix_mode: repr.audio_mix_mode,
        })
    }
}

/// `DualTrackUniversal` emits two tracks and the benchmark needs one, so four
/// is already generous; the bound keeps a hostile payload from allocating a
/// plan with thousands of entries.
pub const MAX_AUDIO_TRACKS: usize = 4;

/// How the selected subtitles reach the video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    /// Switchable track inside the container: `mov_text` in MP4, `copy` in MKV.
    SoftEmbed,
    /// Permanently painted into the frames. Expensive, and never implicit.
    BurnIn,
    /// No subtitles at all.
    Strip,
}

/// How the selected audio reaches the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioMixMode {
    /// Copy, or transcode preserving the source channel layout.
    Passthrough,
    /// Fold a surround source into stereo with a front-centre boost.
    StereoDialogueBoost,
    /// Two tracks: boosted stereo as default plus a 5.1 AC-3 for home cinema.
    DualTrackUniversal,
}

/// One audio track of the output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrackPlan {
    /// Input stream index (`-map 0:<source_index>`).
    pub source_index: u32,
    /// Position of this track among the audio streams of the output.
    pub output_index: u32,
    pub action: StreamAction,
    pub encoder: Option<String>,
    /// FFmpeg bitrate token such as `448k`.
    pub bitrate: Option<String>,
    pub channels: u8,
    /// Closed filter the builder is allowed to emit, never free-form text.
    pub filter: Option<AudioFilter>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
}

/// One subtitle track of the output.
///
/// A burned-in track is pixels, not a stream, so `output_index` is `None` for
/// [`SubtitleAction::BurnIn`] and the video filter chain carries it instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleTrackPlan {
    pub source_index: u32,
    pub output_index: Option<u32>,
    pub action: SubtitleAction,
    pub encoder: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
}

/// Parameters the user may override on top of the preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomConversionOptions {
    pub resolution: ResolutionTarget,
    #[serde(deserialize_with = "crate::validate::crf")]
    pub crf: Option<u8>,
    #[serde(deserialize_with = "crate::validate::optional_positive_u32")]
    pub video_bitrate_kbps: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    #[serde(deserialize_with = "crate::validate::optional_positive_u32")]
    pub audio_bitrate_kbps: Option<u32>,
    pub tone_mapping: ToneMappingOption,
}

impl Default for CustomConversionOptions {
    fn default() -> Self {
        Self {
            resolution: ResolutionTarget::Original,
            crf: None,
            video_bitrate_kbps: None,
            video_codec: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            tone_mapping: ToneMappingOption::Hable,
        }
    }
}

/// Target size of the output, expressed as a limit and never as an upscale.
///
/// Serializes as the tagged form (`"original"`, `{"fixed": ...}`,
/// `{"max_height": 1080}`). Deserialization also accepts the `"1080p"` shorthand
/// the IPC example of `docs/architecture-phases-0-and-1.md` §3.5 uses, because a
/// client should not have to spell a tagged enum to say "full HD".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTarget {
    /// Keep the source size.
    Original,
    /// Exact box; the planner still refuses to upscale.
    Fixed { width: u32, height: u32 },
    /// Cap the height and keep the aspect ratio (`1080` for 1080p).
    MaxHeight(u32),
}

impl<'de> Deserialize<'de> for ResolutionTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match ResolutionRepr::deserialize(deserializer)? {
            ResolutionRepr::Tagged(ResolutionTagged::Original) => Ok(Self::Original),
            ResolutionRepr::Tagged(ResolutionTagged::Fixed { width, height }) => {
                require_positive_resolution(width, height).map_err(de::Error::custom)?;
                Ok(Self::Fixed { width, height })
            }
            ResolutionRepr::Tagged(ResolutionTagged::MaxHeight(height)) => {
                require_positive_resolution(height, height).map_err(de::Error::custom)?;
                Ok(Self::MaxHeight(height))
            }
            ResolutionRepr::Shorthand(text) => {
                parse_resolution_shorthand(&text).map_err(de::Error::custom)
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ResolutionRepr {
    Shorthand(String),
    Tagged(ResolutionTagged),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResolutionTagged {
    Original,
    Fixed { width: u32, height: u32 },
    MaxHeight(u32),
}

/// A zero dimension is never a legal target, and it has to be rejected with an
/// actionable message: `untagged` would otherwise flatten every field error into
/// "did not match any variant".
fn require_positive_resolution(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err(
            "dimensions must be greater than zero; omit the field to keep the source size"
                .to_owned(),
        );
    }
    Ok(())
}

/// `original`, `1080p`, `720p`, `480p`. Anything else is a contract error.
fn parse_resolution_shorthand(text: &str) -> Result<ResolutionTarget, String> {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized == "original" {
        return Ok(ResolutionTarget::Original);
    }
    if let Some(digits) = normalized.strip_suffix('p')
        && let Ok(height) = digits.parse::<u32>()
    {
        if height == 0 {
            return Err("a resolution must be greater than zero".to_owned());
        }
        return Ok(ResolutionTarget::MaxHeight(height));
    }
    Err(format!(
        "`{text}` is not a resolution: use `original`, `<height>p` or the tagged form"
    ))
}

/// HDR→SDR operator. Whether to map at all is [`HdrAction`]'s decision; this
/// only chooses the curve once the decision is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToneMappingOption {
    /// Film-like shoulder; the default and the one the benchmark uses.
    Hable,
    /// Gentler roll-off, and the fallback that needs no `zscale`.
    Mobius,
    /// Simple global operator.
    Reinhard,
}

impl ToneMappingOption {
    /// Name FFmpeg's `tonemap` filter expects.
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Hable => "hable",
            Self::Mobius => "mobius",
            Self::Reinhard => "reinhard",
        }
    }
}
