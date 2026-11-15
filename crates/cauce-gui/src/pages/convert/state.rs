//! Decision core of the Convertir page: no GTK type appears here, so the phase,
//! the enabled button, the single queue request and the folder expansion are all
//! tested without a display (`docs/ui-mvp.md` §4, `docs/contracts.md` §6).

use std::path::{Path, PathBuf};

use cauce_core::{
    Capabilities, ConversionPlan, JobError, MediaSnapshot, StreamAction, SubtitleAction,
};

use crate::i18n::{human_error, t, t_with, technical_details};

/// The action that unblocks a machine without FFmpeg (`docs/ui-mvp.md` §9).
pub const FFMPEG_COMMAND: &str = "sudo apt install ffmpeg";

/// The five states of `docs/ui-mvp.md` §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertPhase {
    Empty,
    Inspecting,
    Partial,
    Ready,
    Error,
}

/// One input the daemon could not probe, with the message to show the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedInput {
    pub path: String,
    pub message: String,
}

/// What one press of "Añadir a la cola" sends to `jobs.add`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRequest {
    pub inputs: Vec<String>,
    pub output_dir: String,
    pub preset_id: String,
}

/// Everything the Convertir page decides, with no GTK types.
#[derive(Debug, Default)]
pub struct ConvertState {
    accepted: Vec<MediaSnapshot>,
    rejected: Vec<RejectedInput>,
    pending: Vec<String>,
    output_dir: Option<String>,
    preset_id: Option<String>,
    error: Option<String>,
    error_details: Option<String>,
    blocking: Option<String>,
    queued: bool,
    confirmation: Option<usize>,
}

/// The exact data of the line above the CTA (`docs/ui-mvp.md` §4): how many files
/// will be queued, the preset and the folder, or which of the two is missing.
///
/// It carries data, never a rendered widget: the view adds the localized preset
/// name with [`SummaryLine::with_preset`] and prints [`SummaryLine::sentence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryLine {
    count: usize,
    preset: Option<String>,
    output_dir: Option<String>,
}

impl SummaryLine {
    /// How many accepted files will be queued.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The preset id the page chose, if any.
    pub fn preset(&self) -> Option<&str> {
        self.preset.as_deref()
    }

    /// The output folder the page chose, if any.
    pub fn output_dir(&self) -> Option<&str> {
        self.output_dir.as_deref()
    }

    /// Replaces the preset id with the ratified name of the current locale.
    pub fn with_preset(mut self, name: Option<&str>) -> Self {
        self.preset = name.map(str::to_owned);
        self
    }

    /// The sentence the page shows: the count in singular or plural, the preset
    /// and the folder, or the exact piece that is missing.
    pub fn sentence(&self) -> String {
        let count = match self.count {
            1 => t("convert.summary_one").to_owned(),
            n => t_with("convert.summary_many", "{n}", &n.to_string()),
        };
        let preset = match self.preset.as_deref() {
            Some(name) => t_with("convert.summary_preset", "{name}", name),
            None => t("convert.summary_missing_preset").to_owned(),
        };
        let folder = match self.output_dir.as_deref() {
            Some(path) => t_with("convert.summary_folder", "{path}", path),
            None => t("convert.summary_missing_folder").to_owned(),
        };
        format!("{count} · {preset} · {folder}")
    }
}

impl ConvertState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The state `docs/ui-mvp.md` §4 asks the page to show.
    pub fn phase(&self) -> ConvertPhase {
        if self.blocking.is_some() || self.error.is_some() {
            ConvertPhase::Error
        } else if !self.pending.is_empty() {
            ConvertPhase::Inspecting
        } else if self.accepted.is_empty() && self.rejected.is_empty() {
            ConvertPhase::Empty
        } else if self.rejected.is_empty() {
            ConvertPhase::Ready
        } else {
            ConvertPhase::Partial
        }
    }

    /// Enabled only when inputs, folder, preset and a usable state are all there.
    pub fn can_queue(&self) -> bool {
        !self.queued
            && self.blocking.is_none()
            && self.error.is_none()
            && self.pending.is_empty()
            && !self.accepted.is_empty()
            && self.output_dir.is_some()
            && self.preset_id.is_some()
    }

    pub fn accepted(&self) -> &[MediaSnapshot] {
        &self.accepted
    }

    /// How many accepted files the header counts.
    pub fn accepted_count(&self) -> usize {
        self.accepted.len()
    }

    /// The bytes of the accepted files: what the header announces as the total.
    pub fn accepted_total_bytes(&self) -> u64 {
        self.accepted
            .iter()
            .map(|snapshot| snapshot.size_bytes)
            .sum()
    }

    /// The visible header of the selection: «Seleccionados (2)».
    pub fn selection_heading(&self) -> String {
        match self.accepted.len() {
            1 => t("convert.selection_heading_one").to_owned(),
            n => t_with("convert.selection_heading_many", "{n}", &n.to_string()),
        }
    }

    /// The exact data of the line the page shows above the CTA.
    pub fn summary_line(&self) -> SummaryLine {
        SummaryLine {
            count: self.accepted.len(),
            preset: self.preset_id.clone(),
            output_dir: self.output_dir.clone(),
        }
    }

    /// How many jobs the last successful `jobs.add` queued, if it did.
    pub fn confirmation(&self) -> Option<usize> {
        self.confirmation
    }

    /// The confirmation of `docs/ui-mvp.md` §4: what happened to the selection,
    /// with the count in singular or plural.
    pub fn confirmation_sentence(&self) -> String {
        match self.confirmation {
            Some(1) => t("convert.added_one").to_owned(),
            Some(n) => t_with("convert.added_many", "{n}", &n.to_string()),
            None => String::new(),
        }
    }
    pub fn rejected(&self) -> &[RejectedInput] {
        &self.rejected
    }
    pub fn output_dir(&self) -> Option<&str> {
        self.output_dir.as_deref()
    }
    pub fn preset_id(&self) -> Option<&str> {
        self.preset_id.as_deref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    /// The code and the daemon's message, for the collapsed disclosure only.
    pub fn error_details(&self) -> Option<&str> {
        self.error_details.as_deref()
    }
    /// The message of a hard block, such as a missing FFmpeg.
    pub fn blocking(&self) -> Option<&str> {
        self.blocking.as_deref()
    }

    /// Queues new paths for inspection, ignoring the ones already known.
    pub fn add_inputs(&mut self, paths: Vec<String>) {
        self.queued = false;
        self.error = None;
        self.confirmation = None;
        for path in paths {
            let known = self.accepted.iter().any(|known| known.path == path)
                || self.rejected.iter().any(|known| known.path == path)
                || self.pending.contains(&path);
            if !known {
                self.pending.push(path);
            }
        }
    }

    /// Records a successful probe, leaving the file out of the pending list.
    pub fn accept_probe(&mut self, snapshot: MediaSnapshot) {
        self.queued = false;
        self.retire(&snapshot.path);
        self.rejected.retain(|known| known.path != snapshot.path);
        if !self
            .accepted
            .iter()
            .any(|known| known.path == snapshot.path)
        {
            self.accepted.push(snapshot);
        }
    }

    /// Records a failed probe: the file is listed, the accepted ones stay.
    pub fn reject(&mut self, path: String, message: String) {
        self.queued = false;
        self.retire(&path);
        if !self.rejected.iter().any(|known| known.path == path) {
            self.rejected.push(RejectedInput { path, message });
        }
    }

    /// Moves every still-pending path to the rejected list with `message`.
    pub fn fail_pending(&mut self, message: &str) {
        self.queued = false;
        for path in std::mem::take(&mut self.pending) {
            self.reject(path, message.to_owned());
        }
    }

    pub fn set_output_dir(&mut self, dir: Option<String>) {
        self.queued = false;
        self.output_dir = dir.filter(|dir| !dir.trim().is_empty());
    }

    pub fn set_preset(&mut self, preset_id: Option<String>) {
        self.queued = false;
        self.preset_id = preset_id;
    }

    /// Surfaces a concrete failure inside the page (`docs/ui-mvp.md` §9).
    ///
    /// The message is a sentence Cauce already wrote, so there is no technical
    /// part to hide.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.queued = false;
        self.error = Some(message.into());
        self.error_details = None;
    }

    /// Surfaces a daemon failure: the visible text is the human sentence and the
    /// code plus the daemon's message wait inside the disclosure
    /// (`docs/contracts.md` §8).
    pub fn set_failure(&mut self, error: &JobError) {
        self.queued = false;
        self.error = Some(human_error(error.code, &error.message));
        self.error_details = Some(technical_details(error.code, &error.message));
    }

    /// A missing encoder or filter blocks the page before anything is queued.
    pub fn apply_capabilities(&mut self, capabilities: &Capabilities) {
        self.blocking = if capabilities.ffmpeg_missing.is_empty() {
            None
        } else {
            Some(
                t("convert.blocking")
                    .replace("{command}", FFMPEG_COMMAND)
                    .replace("{missing}", &capabilities.ffmpeg_missing.join(", ")),
            )
        };
    }

    /// Hands the queue request to the caller exactly once.
    pub fn take_queue_request(&mut self) -> Option<QueueRequest> {
        if !self.can_queue() {
            return None;
        }
        let request = QueueRequest {
            inputs: self
                .accepted
                .iter()
                .map(|known| known.path.clone())
                .collect(),
            output_dir: self.output_dir.clone()?,
            preset_id: self.preset_id.clone()?,
        };
        self.queued = true;
        Some(request)
    }

    /// Empties the selection after a successful `jobs.add`, keeping folder/preset.
    pub fn clear_all(&mut self) {
        self.accepted.clear();
        self.rejected.clear();
        self.pending.clear();
        self.error = None;
        self.error_details = None;
        self.queued = false;
        self.confirmation = None;
    }

    /// Removes one accepted file, the row's own button. Reports the snapshot that
    /// left so the caller never has to search for it again.
    pub fn removal(&mut self, path: &str) -> Option<MediaSnapshot> {
        let index = self
            .accepted
            .iter()
            .position(|snapshot| snapshot.path == path)?;
        self.queued = false;
        self.confirmation = None;
        Some(self.accepted.remove(index))
    }

    /// Empties the selection after a successful `jobs.add` and records how many
    /// jobs were queued, so the page explains where they went instead of silently
    /// dropping the list.
    pub fn confirm_queue(&mut self, count: usize) {
        self.clear_all();
        self.confirmation = Some(count);
    }

    fn retire(&mut self, path: &str) {
        match self.pending.iter().position(|pending| pending == path) {
            Some(index) => {
                self.pending.remove(index);
            }
            // The daemon canonicalizes paths, so the answer may not match the
            // literal string we sent: retire the oldest pending entry instead.
            None if !self.pending.is_empty() => {
                self.pending.remove(0);
            }
            None => {}
        }
    }
}

/// Every regular file directly inside `dir`: one level, no symlink following.
pub fn expand_dropped_folder(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .collect();
    paths.sort();
    paths
}

/// Every path one drop contributes: files as they came, and each folder expanded
/// one level without following symlinks (`docs/ui-mvp.md` §4 and
/// `docs/contracts.md` §9). A single drop may carry several files and several
/// folders at once, so the result is the concatenation in the order the desktop
/// sent them.
pub fn expand_dropped_paths(paths: Vec<PathBuf>) -> Vec<String> {
    let mut expanded: Vec<String> = Vec::new();
    for path in paths {
        if path.is_dir() {
            expanded.extend(expand_dropped_folder(&path));
        } else {
            expanded.push(path.to_string_lossy().into_owned());
        }
    }
    expanded
}

/// Last path segment, which is all the UI ever shows (`docs/ui-mvp.md` §4).
pub fn base_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

/// One line per file: container, duration, resolution and codecs.
pub fn summarize(snapshot: &MediaSnapshot) -> String {
    let mut parts = vec![
        snapshot.container.to_uppercase(),
        duration_text(snapshot.duration_ms),
    ];
    match snapshot.video_streams.first() {
        Some(video) => parts.push(format!("{}×{}", video.width, video.height)),
        None => parts.push(t("convert.audio_only").to_owned()),
    }
    let codecs: Vec<&str> = snapshot
        .video_streams
        .iter()
        .map(|stream| stream.codec.as_str())
        .chain(
            snapshot
                .audio_streams
                .iter()
                .map(|stream| stream.codec.as_str()),
        )
        .collect();
    if !codecs.is_empty() {
        parts.push(codecs.join(" + "));
    }
    parts.join(" · ")
}

fn duration_text(duration_ms: u64) -> String {
    let seconds = duration_ms / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The plan preview sentence of `docs/ui-mvp.md` §4, built from the plan itself.
pub fn plan_summary(plan: &ConversionPlan) -> String {
    let video = match plan.video.action {
        StreamAction::Copy => t("convert.summary_video_copy"),
        StreamAction::Transcode => t("convert.summary_video_transcode"),
        StreamAction::Remove => t("convert.summary_video_remove"),
    };
    let audio = match plan.audio.action {
        StreamAction::Copy => t("convert.summary_audio_copy"),
        StreamAction::Transcode => t("convert.summary_audio_transcode"),
        StreamAction::Remove => t("convert.summary_audio_remove"),
    };
    let subtitles = match plan.subtitles.action {
        SubtitleAction::Copy => t("convert.summary_subtitles_copy"),
        SubtitleAction::Transcode => t("convert.summary_subtitles_transcode"),
        SubtitleAction::Remove => t("convert.summary_subtitles_remove"),
        SubtitleAction::BurnIn => t("convert.summary_subtitles_burn_in"),
    };
    format!("{video} · {audio} · {subtitles} → {}", plan.container)
}

/// The single line the page shows without expanding anything: the summary is a
/// sentence, so it never grows the page in height (`L43`).
pub fn plan_headline(plan: &ConversionPlan) -> String {
    plan_summary(plan)
}

/// Everything the "Detalles" disclosure holds: the full summary first and then
/// every `reasoning[]` line the daemon sent, in its own order.
pub fn plan_details(plan: &ConversionPlan) -> Vec<String> {
    let mut lines = Vec::with_capacity(plan.reasoning.len() + 1);
    lines.push(plan_headline(plan));
    lines.extend(plan.reasoning.iter().cloned());
    lines
}

impl ConvertState {
    /// Paths still waiting for an inspection answer.
    pub fn pending_paths(&self) -> &[String] {
        &self.pending
    }
}
