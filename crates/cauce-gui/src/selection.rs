//! Per-file selection state (T8).
//!
//! Tracks `TrackSelection` and `CustomConversionOptions` for each input file.
//! Defaults are derived from the probe and preset. When a preset changes, only
//! fields not explicitly touched by the user are recomputed. Before queueing,
//! selections are validated against the media snapshot and preset.

use std::collections::BTreeMap;

use cauce_core::{
    AudioMixMode, CustomConversionOptions, MediaSnapshot, Preset, ResolutionTarget, SubtitleMode,
    SubtitlePolicy, SubtitleStream, ToneMappingOption, TrackSelection,
};

/// Error returned when selection validation fails before queueing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionError {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for SelectionError {}

/// Tracks which fields have been explicitly set by the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TouchedFields {
    pub audio_track: bool,
    pub audio_mix_mode: bool,
    pub subtitle_track: bool,
    pub subtitle_mode: bool,
    pub resolution: bool,
    pub crf: bool,
    pub tone_mapping: bool,
}

/// Selection record for a single input file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSelection {
    pub path: String,
    pub selection: TrackSelection,
    pub options: CustomConversionOptions,
    pub touched: TouchedFields,
}

impl FileSelection {
    /// Builds defaults from the probe and an optional preset.
    pub fn from_probe(snapshot: &MediaSnapshot, preset: Option<&Preset>) -> Self {
        let (audio_track_indices, audio_mix_mode) = default_audio(snapshot, preset);
        let (subtitle_track_index, subtitle_mode) = default_subtitles(snapshot, preset);

        let selection = TrackSelection {
            audio_track_indices,
            subtitle_track_index,
            subtitle_mode,
            audio_mix_mode,
        };

        let options = CustomConversionOptions {
            resolution: ResolutionTarget::Original,
            tone_mapping: ToneMappingOption::Hable,
            crf: None,
            video_bitrate_kbps: None,
            video_codec: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
        };

        Self {
            path: snapshot.path.clone(),
            selection,
            options,
            touched: TouchedFields::default(),
        }
    }

    /// Recomputes defaults when preset changes, for untouched fields only.
    pub fn recompute_preset_defaults(&mut self, snapshot: &MediaSnapshot, preset: &Preset) {
        if !self.touched.audio_track {
            let (indices, mix) = default_audio(snapshot, Some(preset));
            self.selection.audio_track_indices = indices;
            if !self.touched.audio_mix_mode {
                self.selection.audio_mix_mode = mix;
            }
        } else if !self.touched.audio_mix_mode {
            if let Some(&first_idx) = self.selection.audio_track_indices.first() {
                if let Some(stream) = snapshot.audio_streams.iter().find(|s| s.index == first_idx) {
                    self.selection.audio_mix_mode = if stream.channels >= 6 {
                        AudioMixMode::StereoDialogueBoost
                    } else {
                        AudioMixMode::Passthrough
                    };
                }
            }
        }

        if !self.touched.subtitle_mode {
            let (sub_idx, sub_mode) = default_subtitles(snapshot, Some(preset));
            self.selection.subtitle_mode = sub_mode;
            if !self.touched.subtitle_track {
                self.selection.subtitle_track_index = sub_idx;
            }
        } else if !self.touched.subtitle_track {
            let (sub_idx, _) = default_subtitles(snapshot, Some(preset));
            self.selection.subtitle_track_index = sub_idx;
        }

        if !self.touched.resolution {
            self.options.resolution = ResolutionTarget::Original;
        }

        if !self.touched.crf {
            self.options.crf = None;
        }

        if !self.touched.tone_mapping {
            self.options.tone_mapping = ToneMappingOption::Hable;
        }
    }

    pub fn set_audio_track(&mut self, index: u32, snapshot: Option<&MediaSnapshot>) {
        self.selection.audio_track_indices = vec![index];
        self.touched.audio_track = true;
        if !self.touched.audio_mix_mode {
            if let Some(snapshot) = snapshot {
                if let Some(stream) = snapshot.audio_streams.iter().find(|s| s.index == index) {
                    self.selection.audio_mix_mode = if stream.channels >= 6 {
                        AudioMixMode::StereoDialogueBoost
                    } else {
                        AudioMixMode::Passthrough
                    };
                }
            }
        }
    }

    pub fn set_audio_mix_mode(&mut self, mode: AudioMixMode) {
        self.selection.audio_mix_mode = mode;
        self.touched.audio_mix_mode = true;
    }

    pub fn set_subtitle_track(&mut self, index: Option<u32>) {
        self.selection.subtitle_track_index = index;
        self.touched.subtitle_track = true;
    }

    pub fn set_subtitle_mode(&mut self, mode: SubtitleMode) {
        self.selection.subtitle_mode = mode;
        self.touched.subtitle_mode = true;
    }

    pub fn set_resolution(&mut self, target: ResolutionTarget) {
        self.options.resolution = target;
        self.touched.resolution = true;
    }

    pub fn set_crf(&mut self, crf: Option<u8>) {
        self.options.crf = crf;
        self.touched.crf = true;
    }

    pub fn set_tone_mapping(&mut self, tm: ToneMappingOption) {
        self.options.tone_mapping = tm;
        self.touched.tone_mapping = true;
    }

    /// Validates this file's selection against the snapshot and preset before queueing.
    pub fn validate(
        &self,
        snapshot: &MediaSnapshot,
        preset: &Preset,
    ) -> Result<(), SelectionError> {
        if preset.video_codec == "none" && self.selection.subtitle_mode == SubtitleMode::BurnIn {
            return Err(SelectionError {
                path: self.path.clone(),
                message: format!(
                    "el preset `{}` no tiene video, no se pueden incrustar subtítulos (burn-in)",
                    preset.id
                ),
            });
        }

        for &audio_idx in &self.selection.audio_track_indices {
            if !snapshot.audio_streams.iter().any(|s| s.index == audio_idx) {
                return Err(SelectionError {
                    path: self.path.clone(),
                    message: format!(
                        "la pista de audio con índice {audio_idx} no existe en el archivo"
                    ),
                });
            }
        }

        if self.selection.subtitle_mode != SubtitleMode::Strip {
            match self.selection.subtitle_track_index {
                Some(sub_idx) => {
                    if !snapshot.subtitle_streams.iter().any(|s| s.index == sub_idx) {
                        return Err(SelectionError {
                            path: self.path.clone(),
                            message: format!(
                                "la pista de subtítulos con índice {sub_idx} no existe en el archivo"
                            ),
                        });
                    }
                }
                None => {
                    return Err(SelectionError {
                        path: self.path.clone(),
                        message: "se seleccionaron subtítulos pero no se indicó ninguna pista"
                            .to_owned(),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Collection of per-file selections keyed by path.
#[derive(Debug, Clone, Default)]
pub struct SelectionTable {
    entries: BTreeMap<String, FileSelection>,
}

impl SelectionTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, path: &str) -> Option<&FileSelection> {
        self.entries.get(path)
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut FileSelection> {
        self.entries.get_mut(path)
    }

    pub fn insert(&mut self, selection: FileSelection) {
        self.entries.insert(selection.path.clone(), selection);
    }

    pub fn register(
        &mut self,
        snapshot: &MediaSnapshot,
        preset: Option<&Preset>,
    ) -> &mut FileSelection {
        self.entries
            .entry(snapshot.path.clone())
            .or_insert_with(|| FileSelection::from_probe(snapshot, preset))
    }

    pub fn remove(&mut self, path: &str) -> Option<FileSelection> {
        self.entries.remove(path)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &FileSelection)> {
        self.entries.iter()
    }

    pub fn apply_preset(&mut self, preset: &Preset, snapshots: &[MediaSnapshot]) {
        for selection in self.entries.values_mut() {
            if let Some(snapshot) = snapshots.iter().find(|s| s.path == selection.path) {
                selection.recompute_preset_defaults(snapshot, preset);
            }
        }
    }

    pub fn validate(
        &self,
        snapshots: &[MediaSnapshot],
        preset: &Preset,
    ) -> Result<(), SelectionError> {
        for selection in self.entries.values() {
            let snapshot = snapshots
                .iter()
                .find(|s| s.path == selection.path)
                .ok_or_else(|| SelectionError {
                    path: selection.path.clone(),
                    message: "no se encontró el snapshot del archivo".to_owned(),
                })?;
            selection.validate(snapshot, preset)?;
        }
        Ok(())
    }
}

fn default_audio(snapshot: &MediaSnapshot, preset: Option<&Preset>) -> (Vec<u32>, AudioMixMode) {
    if snapshot.audio_streams.is_empty() {
        return (Vec::new(), AudioMixMode::Passthrough);
    }

    let chosen = if let Some(pref) = preset.and_then(|p| p.preferred_audio_language.as_deref()) {
        snapshot
            .audio_streams
            .iter()
            .find(|s| language_matches(s.language.as_deref(), pref))
            .or_else(|| snapshot.audio_streams.iter().find(|s| s.is_default))
            .or_else(|| snapshot.audio_streams.first())
    } else {
        snapshot
            .audio_streams
            .iter()
            .find(|s| s.is_default)
            .or_else(|| snapshot.audio_streams.first())
    };

    match chosen {
        Some(stream) => {
            let mix = if stream.channels >= 6 {
                AudioMixMode::StereoDialogueBoost
            } else {
                AudioMixMode::Passthrough
            };
            (vec![stream.index], mix)
        }
        None => (Vec::new(), AudioMixMode::Passthrough),
    }
}

fn default_subtitles(
    snapshot: &MediaSnapshot,
    preset: Option<&Preset>,
) -> (Option<u32>, SubtitleMode) {
    if snapshot.subtitle_streams.is_empty() {
        return (None, SubtitleMode::Strip);
    }

    if let Some(preset) = preset {
        if preset.video_codec == "none" || preset.subtitle_policy == SubtitlePolicy::Remove {
            return (None, SubtitleMode::Strip);
        }
    }

    let chosen: Option<&SubtitleStream> = snapshot
        .subtitle_streams
        .iter()
        .find(|s| s.is_default)
        .or_else(|| snapshot.subtitle_streams.first());

    match chosen {
        Some(stream) => (Some(stream.index), SubtitleMode::SoftEmbed),
        None => (None, SubtitleMode::Strip),
    }
}

fn language_matches(stream_lang: Option<&str>, target: &str) -> bool {
    let Some(lang) = stream_lang else {
        return false;
    };
    let l1 = lang.trim().to_ascii_lowercase();
    let l2 = target.trim().to_ascii_lowercase();
    l1 == l2 || l1.starts_with(&l2) || l2.starts_with(&l1)
}
