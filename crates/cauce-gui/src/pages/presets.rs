//! The Presets page: the thirteen system presets and the one custom preset,
//! presented as explicit, navigable groups (`docs/ui-mvp.md` §6,
//! `docs/contracts.md` §2.3 and §6, `docs/presets.md`).
//!
//! The system catalog is immutable: its groups are read-only and offer no
//! affordance to edit or delete. The fifth group, «Personalizado», holds exactly
//! one preset the user owns; saving replaces it and there is no generic editor.
//!
//! The decision core ([`PresetFilter`], [`group_presets`], [`preset_essentials`],
//! [`preset_technical_detail`] and [`CustomDraft`]) contains no GTK type, so
//! `tests/pages_contract.rs` covers it without a display; the rest of this module
//! is only its rendering.
//!
//! The six chips of the navigator ([`filters_in_order`]) are what makes the
//! groups navigable: they sit above the split view, outside every scrolled area,
//! so «Personalizado» is on the first screen instead of at the end of a long
//! scroll (`odd/tasks/cauce-0.1.md`, L54).
//!
//! `docs/ui-mvp.md` §6 lists six group names for what `docs/contracts.md` §2.3
//! models as four `category` values, so each documented pair collapses into the
//! authoritative field: `tv` = Smart TV, `social` = Web y redes,
//! `archive` = Archivo y optimización, `audio` = Audio y música. The fifth group
//! is `custom` and always goes last.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use adw::glib;
use adw::gtk;
use adw::prelude::*;
use cauce_core::{
    CustomAudio, CustomBitrate, CustomContainer, CustomPreset, CustomQuality, CustomQualityLevel,
    CustomResolution, CustomSubtitles, CustomVideo, ErrorCode, JobError, Preset, SubtitlePolicy,
};
use libadwaita as adw;
use serde_json::json;

use crate::client::Client;
use crate::i18n::{
    hdr_label, human_error, localized_preset_description, localized_preset_name, resolution,
    subtitle_label, t, technical_details, yes_no,
};
use crate::pages::{
    ErrorSurface, clear_failure, error_details_label, error_surface, escaped, show_parts,
};

/// Documented group order: Smart TV, Web y redes, Archivo, Audio.
pub const CATEGORIES: [&str; 4] = ["tv", "social", "archive", "audio"];

/// Category of the one custom preset: its group always closes the list.
pub const CUSTOM_CATEGORY: &str = "custom";

/// Read-only notice, the only action the system groups do not offer.
const READ_ONLY: &str = "presets.read_only";

/// The name of the custom preset as a row and a panel title: «Mi preset»,
/// «My preset», «Meu preset». Deliberately not the group name, which stays on the
/// chip and on the category header (`odd/screenshots/07-presets-custom.png`).
const CUSTOM_ROW: &str = "presets.custom_row_name";

/// One sentence that explains the custom group.
const CUSTOM_HINT: &str = "presets.custom_hint";

/// The chips of the navigator above the list: one per group plus «Todas».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetFilter {
    All,
    Tv,
    Web,
    Archive,
    Audio,
    Custom,
}

impl PresetFilter {
    /// The `category` this chip keeps; `None` keeps the whole catalog.
    pub const fn category(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Tv => Some("tv"),
            Self::Web => Some("social"),
            Self::Archive => Some("archive"),
            Self::Audio => Some("audio"),
            Self::Custom => Some(CUSTOM_CATEGORY),
        }
    }

    /// Stable identifier of the chip, also the name of its `AdwToggle`.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Tv => "tv",
            Self::Web => "web",
            Self::Archive => "archive",
            Self::Audio => "audio",
            Self::Custom => "custom",
        }
    }

    /// The chip a toggle name stands for; «Todas» when the name is not ours.
    pub fn from_slug(slug: &str) -> Self {
        filters_in_order()
            .into_iter()
            .find(|filter| filter.slug() == slug)
            .unwrap_or(Self::All)
    }

    /// The label of the chip in the current locale. The four short names are not
    /// the group headers: a chip has to fit next to the other five.
    pub fn label(self) -> String {
        match self {
            Self::All => t("presets.filter_all").to_owned(),
            Self::Tv => category_label("tv"),
            Self::Web => t("presets.filter_web").to_owned(),
            Self::Archive => t("presets.filter_archive").to_owned(),
            Self::Audio => t("presets.filter_audio").to_owned(),
            Self::Custom => category_label(CUSTOM_CATEGORY),
        }
    }

    /// Whether this view always offers the «Personalizado» slot, which is how
    /// the one preset the user owns is created.
    const fn offers_custom_slot(self) -> bool {
        matches!(self, Self::All | Self::Custom)
    }
}

/// The chips in the documented order, «Todas» first and «Personalizado» last.
pub fn filters_in_order() -> [PresetFilter; 6] {
    [
        PresetFilter::All,
        PresetFilter::Tv,
        PresetFilter::Web,
        PresetFilter::Archive,
        PresetFilter::Audio,
        PresetFilter::Custom,
    ]
}

/// The presets that belong to `filter`, keeping the catalog order.
pub fn filter_presets(presets: &[Preset], filter: PresetFilter) -> Vec<&Preset> {
    match filter.category() {
        Some(category) => presets
            .iter()
            .filter(|preset| preset.category == category)
            .collect(),
        None => presets.iter().collect(),
    }
}

/// The name of the custom preset where a row or a panel title names it: distinct
/// from the group header, so the list row does not repeat what the header and the
/// chip already say.
pub fn custom_row_label() -> &'static str {
    t(CUSTOM_ROW)
}

/// The one heading pair the custom panel shows: its title and its description.
/// The editor below renders no heading of its own ([`custom_editor_heading`]),
/// which is the structural reason «Personalizado» and its sentence cannot appear
/// twice in the panel (`odd/screenshots/07-presets-custom.png`).
///
/// `None` means the panel shows no such line, so a caller never renders an empty
/// label: the pair is what `show_custom` writes, in this order.
pub fn custom_headers() -> (Option<&'static str>, Option<&'static str>) {
    (Some(custom_row_label()), Some(t(CUSTOM_HINT)))
}

/// The heading the editor's own preferences group renders: none, by design.
///
/// The panel header above already carries the title and the description of the
/// custom group; letting the editor repeat them is what duplicated them
/// (`odd/screenshots/07-presets-custom.png`). Returning `None` keeps that a data
/// decision the contract tests can hold, instead of a style tweak.
pub fn custom_editor_heading() -> (Option<&'static str>, Option<&'static str>) {
    (None, None)
}

/// Header of one category; an unknown category keeps its own name.
pub fn category_label(category: &str) -> String {
    match category {
        "tv" => t("presets.group_tv").to_owned(),
        "social" => t("presets.group_social").to_owned(),
        "archive" => t("presets.group_archive").to_owned(),
        "audio" => t("presets.group_audio").to_owned(),
        CUSTOM_CATEGORY => t("presets.group_custom").to_owned(),
        other => other.to_owned(),
    }
}

/// Groups the catalog by `category`, in the documented order, keeping catalog
/// order inside each group. Unknown categories come next, alphabetically, and
/// «Personalizado» always goes last: it is the group the user owns.
///
/// Generic over what the slice holds so a filtered view (`&[&Preset]`) and the
/// catalog itself (`&[Preset]`) both group without cloning.
pub fn group_presets<T: std::borrow::Borrow<Preset>>(presets: &[T]) -> Vec<(String, Vec<&Preset>)> {
    let mut buckets: BTreeMap<&str, Vec<&Preset>> = BTreeMap::new();
    for preset in presets {
        let preset: &Preset = std::borrow::Borrow::borrow(preset);
        buckets
            .entry(preset.category.as_str())
            .or_default()
            .push(preset);
    }

    let custom = buckets.remove(CUSTOM_CATEGORY);
    let mut groups: Vec<(String, Vec<&Preset>)> = Vec::with_capacity(buckets.len() + 1);
    for category in CATEGORIES {
        if let Some(items) = buckets.remove(category) {
            groups.push((category.to_owned(), items));
        }
    }
    // `BTreeMap` already yields the leftovers in alphabetical order.
    for (category, items) in buckets {
        groups.push((category.to_owned(), items));
    }
    if let Some(items) = custom {
        groups.push((CUSTOM_CATEGORY.to_owned(), items));
    }
    groups
}

/// Label/value rows of the detail pane. A field that the preset leaves unset
/// produces no row at all, so nothing is ever shown as an empty value. The label
/// is a catalog key: the row says whatever the current locale says.
///
/// The name is deliberately absent: the panel title already shows it, and the
/// capture showed it twice (`odd/tasks/cauce-0.1.md`, L43).
pub fn preset_detail(preset: &Preset) -> Vec<(String, String)> {
    let mut rows = Vec::with_capacity(13);
    rows.push((
        t("presets.detail_description").to_owned(),
        localized_preset_description(preset).to_owned(),
    ));
    rows.extend(preset_essentials(preset));
    rows.extend(preset_technical_detail(preset));
    rows
}

/// The five rows the detail pane shows without opening anything: what the file
/// will be, in the order a person reads it. A value the preset leaves unset is
/// said in words («Sin video», «Sin CRF», «Original»), never as an empty row.
pub fn preset_essentials(preset: &Preset) -> Vec<(String, String)> {
    vec![
        (
            t("presets.detail_container").to_owned(),
            preset.container.clone(),
        ),
        (
            t("presets.detail_video_codec").to_owned(),
            track_label(&preset.video_codec, "presets.custom_video_none"),
        ),
        (
            t("presets.detail_audio_codec").to_owned(),
            track_label(&preset.audio_codec, "presets.custom_audio_none"),
        ),
        (
            t("presets.detail_quality").to_owned(),
            match preset.video_quality {
                Some(crf) => crf.to_string(),
                None => t("presets.value_no_quality").to_owned(),
            },
        ),
        (
            t("presets.detail_resolution").to_owned(),
            resolution(preset.max_width, preset.max_height)
                .unwrap_or_else(|| t("presets.custom_resolution_original").to_owned()),
        ),
    ]
}

/// A codec as the user reads it: a copy keeps the source, a removed track says
/// so, and every other codec keeps its own name.
fn track_label(codec: &str, missing_key: &'static str) -> String {
    match codec {
        "copy" => t("presets.custom_copy").to_owned(),
        "none" => t(missing_key).to_owned(),
        other => other.to_owned(),
    }
}

/// The technical rows only: what the collapsed «Detalles técnicos» shows. The
/// five essentials are already on screen, so they never repeat here.
pub fn preset_technical_detail(preset: &Preset) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::with_capacity(9);
    let mut push = |key: &'static str, value: String| rows.push((t(key).to_owned(), value));

    if let Some(bitrate) = preset.audio_bitrate.as_ref() {
        push("presets.detail_audio_bitrate", bitrate.clone());
    }
    if let Some(fps) = preset.max_fps {
        push("presets.detail_fps", fps_label(fps));
    }
    if let Some(profile) = preset.profile.as_ref() {
        push("presets.detail_profile", profile.clone());
    }
    if let Some(level) = preset.level {
        push("presets.detail_level", level.to_string());
    }
    if let Some(bits) = preset.max_bit_depth {
        push(
            "presets.detail_bit_depth",
            t("presets.value_bits").replace("{n}", &bits.to_string()),
        );
    }
    push(
        "presets.detail_hdr",
        hdr_label(preset.hdr_policy).to_owned(),
    );
    push(
        "presets.detail_subtitles",
        subtitle_label(preset.subtitle_policy).to_owned(),
    );
    push(
        "presets.detail_faststart",
        yes_no(preset.faststart).to_owned(),
    );
    push(
        "presets.detail_metadata",
        match preset.preserve_metadata {
            true => t("presets.value_metadata_kept").to_owned(),
            false => t("presets.value_metadata_dropped").to_owned(),
        },
    );

    rows
}

/// Whole frame rates read as `60`, fractional ones keep their decimals.
fn fps_label(fps: f64) -> String {
    match fps.fract() == 0.0 {
        true => format!("{fps:.0}"),
        false => format!("{fps}"),
    }
}

// ---------------------------------------------------------------------------
// The compact editor's state: the seven choices the user can make, as the values
// behind each `AdwComboRow`. Keeping it a plain struct is what lets the contract
// tests drive the save path without a display.
// ---------------------------------------------------------------------------

/// The seven choices of the compact editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomDraft {
    pub container: CustomContainer,
    pub video: CustomVideo,
    pub quality: CustomQualityLevel,
    pub resolution: CustomResolution,
    pub audio: CustomAudio,
    pub bitrate: CustomBitrate,
    pub subtitles: CustomSubtitles,
}

impl CustomDraft {
    /// The editor opens on a legal target: MP4 with H.264 and AAC keeps every
    /// generator of players happy, and nothing in it needs a decision first.
    pub const fn defaults() -> Self {
        Self {
            container: CustomContainer::Mp4,
            video: CustomVideo::H264,
            quality: CustomQualityLevel::High,
            resolution: CustomResolution::Original,
            audio: CustomAudio::Aac,
            bitrate: CustomBitrate::K192,
            subtitles: CustomSubtitles::Keep,
        }
    }

    /// The `presets.set_custom` argument. The bitrate is dropped when the audio is
    /// copied or removed: a bitrate with no track to encode is rejected by the
    /// domain, so the editor never sends one.
    pub fn to_custom(self) -> CustomPreset {
        CustomPreset {
            container: self.container,
            video: self.video,
            quality: CustomQuality::Level(self.quality),
            resolution: self.resolution,
            audio: self.audio,
            bitrate: match self.audio {
                CustomAudio::Copy | CustomAudio::None => None,
                _ => Some(self.bitrate),
            },
            subtitles: self.subtitles,
        }
    }

    /// Reopens the editor on a saved preset. Fields the compact editor cannot
    /// represent fall back to their default: a CRF that is not one of the three
    /// levels shows the closest one, and saving then writes that level's CRF.
    pub fn from_preset(preset: &Preset) -> Self {
        let defaults = Self::defaults();
        let video = match preset.video_codec.as_str() {
            "h264" => CustomVideo::H264,
            "h265" | "hevc" => CustomVideo::H265,
            "vp9" => CustomVideo::Vp9,
            "av1" => CustomVideo::Av1,
            "copy" => CustomVideo::Copy,
            "none" => CustomVideo::None,
            _ => defaults.video,
        };
        Self {
            container: match preset.container.as_str() {
                "mp4" => CustomContainer::Mp4,
                "mkv" => CustomContainer::Mkv,
                "webm" => CustomContainer::Webm,
                _ => defaults.container,
            },
            video,
            quality: closest_level(preset.video_quality, video).unwrap_or(defaults.quality),
            resolution: match (preset.max_width, preset.max_height) {
                (Some(1920), Some(1080)) => CustomResolution::P1080,
                (Some(1280), Some(720)) => CustomResolution::P720,
                (Some(854), Some(480)) => CustomResolution::P480,
                (None, None) => CustomResolution::Original,
                _ => defaults.resolution,
            },
            audio: match preset.audio_codec.as_str() {
                "aac" => CustomAudio::Aac,
                "mp3" => CustomAudio::Mp3,
                "opus" => CustomAudio::Opus,
                "flac" => CustomAudio::Flac,
                "copy" => CustomAudio::Copy,
                "none" => CustomAudio::None,
                _ => defaults.audio,
            },
            bitrate: preset
                .audio_bitrate
                .as_deref()
                .and_then(|token| token.trim_end_matches('k').parse::<u16>().ok())
                .and_then(|kbps| CustomBitrate::try_from(kbps).ok())
                .unwrap_or(defaults.bitrate),
            subtitles: match preset.subtitle_policy {
                SubtitlePolicy::Remove | SubtitlePolicy::BurnIn => CustomSubtitles::Remove,
                _ => CustomSubtitles::Keep,
            },
        }
    }
}

/// The level whose CRF for `video` is closest to `crf`; nothing when the stream
/// carries no quality at all.
fn closest_level(crf: Option<u8>, video: CustomVideo) -> Option<CustomQualityLevel> {
    let crf = crf?;
    CustomQualityLevel::ALL
        .into_iter()
        .filter_map(|level| {
            let candidate = CustomQuality::Level(level).crf_for(video)?;
            Some((candidate.abs_diff(crf), level))
        })
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, level)| level)
}

// ---------------------------------------------------------------------------
// Rendering.
// ---------------------------------------------------------------------------

/// Every widget the page needs after construction.
#[derive(Clone)]
struct Ui {
    page: adw::NavigationPage,
    stack: gtk::Stack,
    list: gtk::ListBox,
    /// The navigator above the list: it stays put while the rows scroll.
    filters: adw::ToggleGroup,
    error: adw::StatusPage,
    error_expander: gtk::Expander,
    error_details: gtk::Label,
    retry: gtk::Button,
    title: gtk::Label,
    description: gtk::Label,
    essentials: gtk::ListBox,
    technical: gtk::Expander,
    detail_list: gtk::ListBox,
    notice: gtk::Label,
    editor: Editor,
}

/// The compact editor of the one custom preset.
#[derive(Clone)]
struct Editor {
    /// Everything the editor shows; hidden while a system preset is selected.
    area: gtk::Box,
    container: adw::ComboRow,
    video: adw::ComboRow,
    quality: adw::ComboRow,
    resolution: adw::ComboRow,
    audio: adw::ComboRow,
    bitrate: adw::ComboRow,
    subtitles: adw::ComboRow,
    save: gtk::Button,
    delete: gtk::Button,
    status: gtk::Label,
    failure: ErrorSurface,
}

const CONTAINERS: [CustomContainer; 3] = [
    CustomContainer::Mp4,
    CustomContainer::Mkv,
    CustomContainer::Webm,
];
const VIDEOS: [CustomVideo; 6] = [
    CustomVideo::H264,
    CustomVideo::H265,
    CustomVideo::Vp9,
    CustomVideo::Av1,
    CustomVideo::Copy,
    CustomVideo::None,
];
const RESOLUTIONS: [CustomResolution; 4] = [
    CustomResolution::Original,
    CustomResolution::P1080,
    CustomResolution::P720,
    CustomResolution::P480,
];
const AUDIOS: [CustomAudio; 6] = [
    CustomAudio::Aac,
    CustomAudio::Mp3,
    CustomAudio::Opus,
    CustomAudio::Flac,
    CustomAudio::Copy,
    CustomAudio::None,
];
const SUBTITLES: [CustomSubtitles; 2] = [CustomSubtitles::Keep, CustomSubtitles::Remove];

fn container_label(container: CustomContainer) -> String {
    container.as_str().to_uppercase()
}

fn video_label(video: CustomVideo) -> String {
    match video {
        CustomVideo::Copy => t("presets.custom_copy").to_owned(),
        CustomVideo::None => t("presets.custom_video_none").to_owned(),
        other => other.as_str().to_owned(),
    }
}

fn audio_label(audio: CustomAudio) -> String {
    match audio {
        CustomAudio::Copy => t("presets.custom_copy").to_owned(),
        CustomAudio::None => t("presets.custom_audio_none").to_owned(),
        other => other.as_str().to_owned(),
    }
}

fn quality_label(level: CustomQualityLevel) -> String {
    match level {
        CustomQualityLevel::High => t("presets.custom_quality_high").to_owned(),
        CustomQualityLevel::Medium => t("presets.custom_quality_medium").to_owned(),
        CustomQualityLevel::Low => t("presets.custom_quality_low").to_owned(),
    }
}

fn resolution_label(value: CustomResolution) -> String {
    match value {
        CustomResolution::Original => t("presets.custom_resolution_original").to_owned(),
        other => other.as_str().to_owned(),
    }
}

fn bitrate_label(bitrate: CustomBitrate) -> String {
    t("presets.custom_kbps").replace("{n}", &bitrate.kbps().to_string())
}

fn subtitles_label(value: CustomSubtitles) -> String {
    match value {
        CustomSubtitles::Keep => t("presets.custom_subtitles_keep").to_owned(),
        CustomSubtitles::Remove => t("presets.custom_subtitles_remove").to_owned(),
    }
}

/// Builds the page, asks the daemon for the catalog and wires the editor.
pub fn page(client: &Rc<Client>) -> adw::NavigationPage {
    let ui = build();
    let presets: Rc<RefCell<Vec<Preset>>> = Rc::new(RefCell::new(Vec::new()));

    ui.list.connect_row_selected({
        let (ui, presets) = (ui.clone(), Rc::clone(&presets));
        move |_, row| {
            let Some(row) = row else {
                return;
            };
            let id = row.widget_name();
            let found = presets
                .borrow()
                .iter()
                .find(|preset| preset.id == id.as_str())
                .cloned();
            match found {
                Some(preset) if preset.category == CUSTOM_CATEGORY => {
                    show_custom(&ui, Some(&preset))
                }
                Some(preset) => show_system(&ui, &preset),
                // The placeholder row of an empty «Personalizado» group.
                None if id.as_str() == CustomPreset::ID => show_custom(&ui, None),
                None => {}
            }
        }
    });
    ui.retry.connect_clicked({
        let (client, ui, presets) = (Rc::clone(client), ui.clone(), Rc::clone(&presets));
        move |_| load(&client, &ui, &presets, None)
    });
    ui.filters.connect_active_name_notify({
        let (ui, presets) = (ui.clone(), Rc::clone(&presets));
        move |group| {
            let filter =
                PresetFilter::from_slug(group.active_name().as_deref().unwrap_or_default());
            // A chip is a view, not a selection: the list opens on its first row,
            // and «Personalizado» opens the editor inline when there is one.
            render(&ui, &presets, None, filter);
        }
    });

    wire_editor(client, &ui, &presets);
    load(client, &ui, &presets, None);
    ui.page
}

/// Save and Delete: the only two writes of the page, both on `presets.set_custom`
/// and `presets.clear_custom`.
fn wire_editor(client: &Rc<Client>, ui: &Ui, presets: &Rc<RefCell<Vec<Preset>>>) {
    ui.editor.save.connect_clicked({
        let (client, ui, presets) = (Rc::clone(client), ui.clone(), Rc::clone(presets));
        move |_| {
            let custom = ui.editor.draft().to_custom();
            // The domain judges the combination before the daemon is bothered: an
            // invalid one is an inline sentence, not a round trip.
            if let Err(error) = custom.validate() {
                fail_custom(&ui, &error);
                return;
            }
            let Ok(params) = serde_json::to_value(&custom) else {
                return;
            };
            let (client, ui, presets) = (Rc::clone(&client), ui.clone(), Rc::clone(&presets));
            glib::spawn_future_local(async move {
                match client.call("presets.set_custom", params).await {
                    Ok(_) => {
                        clear_failure(&ui.editor.failure);
                        let (reload_client, reload_ui, reload_presets) =
                            (Rc::clone(&client), ui.clone(), Rc::clone(&presets));
                        // The reload selects the saved preset, which resets the
                        // editor: the confirmation is written after it.
                        apply_answer(
                            reload_client,
                            reload_ui,
                            reload_presets,
                            Some(CustomPreset::ID),
                        )
                        .await;
                        ui.editor.status.set_label(t("presets.custom_saved"));
                        ui.editor.status.set_visible(true);
                        // Convertir reads `presets.list`, so it has to hear about it.
                        crate::pages::presets_changed();
                    }
                    Err(error) => {
                        ui.editor.status.set_visible(false);
                        fail_custom(&ui, &error);
                    }
                }
            });
        }
    });

    ui.editor.delete.connect_clicked({
        let (client, ui, presets) = (Rc::clone(client), ui.clone(), Rc::clone(presets));
        move |_| {
            let (client, ui, presets) = (Rc::clone(&client), ui.clone(), Rc::clone(&presets));
            glib::spawn_future_local(async move {
                match client.call("presets.clear_custom", json!({})).await {
                    Ok(_) => {
                        clear_failure(&ui.editor.failure);
                        let (reload_client, reload_ui, reload_presets) =
                            (Rc::clone(&client), ui.clone(), Rc::clone(&presets));
                        apply_answer(
                            reload_client,
                            reload_ui,
                            reload_presets,
                            Some(CustomPreset::ID),
                        )
                        .await;
                        ui.editor.status.set_label(t("presets.custom_deleted"));
                        ui.editor.status.set_visible(true);
                        crate::pages::presets_changed();
                    }
                    Err(error) => {
                        ui.editor.status.set_visible(false);
                        fail_custom(&ui, &error);
                    }
                }
            });
        }
    });
}

fn build() -> Ui {
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        t("section.presets_title"),
        t("presets.subtitle"),
    )));

    let loading = adw::StatusPage::new();
    loading.set_icon_name(Some("starred-symbolic"));
    loading.set_title(t("presets.loading_title"));
    loading.set_description(Some(t("presets.loading_description")));

    // One human sentence and, behind a closed disclosure, the code and the
    // daemon's own message (`docs/contracts.md` §8).
    let error = adw::StatusPage::new();
    error.set_icon_name(Some("dialog-warning-symbolic"));
    error.set_title(t("presets.error_title"));
    let retry = gtk::Button::with_label(t("presets.retry"));
    retry.add_css_class("pill");
    let error_details = error_details_label();
    let error_expander = gtk::Expander::new(Some(t("error.technical_details")));
    error_expander.set_expanded(false);
    error_expander.set_child(Some(&error_details));
    let error_body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    error_body.set_halign(gtk::Align::Center);
    error_body.append(&error_expander);
    error_body.append(&retry);
    error.set_child(Some(&error_body));

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    let sidebar = scrolled(&list);
    // The list never scrolls sideways: a row is one line and the panel is narrow,
    // so a horizontal cut would only hide the end of a name (`L43`).
    sidebar.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

    let title = gtk::Label::new(None);
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    title.set_wrap(true);
    let description = gtk::Label::new(None);
    description.add_css_class("dim-label");
    description.set_xalign(0.0);
    description.set_wrap(true);

    // What the panel shows at a glance: five rows, always in the same order.
    let essentials = gtk::ListBox::new();
    essentials.add_css_class("boxed-list");
    essentials.set_selection_mode(gtk::SelectionMode::None);
    essentials.set_widget_name("preset-essentials");

    // Everything else lives behind this closed disclosure, so the panel opens with
    // the name, the sentence that explains the preset and those five values.
    let detail_list = gtk::ListBox::new();
    detail_list.add_css_class("boxed-list");
    detail_list.set_selection_mode(gtk::SelectionMode::None);
    let technical = gtk::Expander::new(Some(t("error.technical_details")));
    technical.set_expanded(false);
    technical.set_child(Some(&detail_list));

    let notice = gtk::Label::new(Some(t(READ_ONLY)));
    notice.add_css_class("caption");
    notice.add_css_class("dim-label");
    notice.set_xalign(0.0);
    notice.set_wrap(true);

    let editor = build_editor();
    let detail = gtk::Box::new(gtk::Orientation::Vertical, 12);
    detail.set_margin_top(18);
    detail.set_margin_bottom(18);
    detail.set_margin_start(18);
    detail.set_margin_end(18);
    detail.append(&title);
    detail.append(&description);
    detail.append(&editor.area);
    detail.append(&essentials);
    detail.append(&technical);
    detail.append(&notice);

    let split = adw::OverlaySplitView::new();
    split.set_sidebar(Some(&sidebar));
    split.set_content(Some(&scrolled(&detail)));
    split.set_min_sidebar_width(220.0);
    split.set_max_sidebar_width(320.0);

    // The navigator sits above the split, outside every scrolled area, so
    // «Personalizado» is on the first screen and the chips never scroll away.
    let filters = filter_bar();
    let ready = gtk::Box::new(gtk::Orientation::Vertical, 0);
    ready.append(&chips(&filters));
    ready.append(&split);

    let stack = gtk::Stack::new();
    stack.add_named(&loading, Some("loading"));
    stack.add_named(&error, Some("error"));
    stack.add_named(&ready, Some("ready"));
    stack.set_visible_child_name("loading");

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));

    let page = adw::NavigationPage::new(&toolbar, t("section.presets_title"));
    page.set_tag(Some("presets"));
    Ui {
        page,
        stack,
        list,
        filters,
        error,
        error_expander,
        error_details,
        retry,
        title,
        description,
        essentials,
        technical,
        detail_list,
        notice,
        editor,
    }
}

/// Six chips in the documented order, «Todas» selected first.
fn filter_bar() -> adw::ToggleGroup {
    let group = adw::ToggleGroup::new();
    group.set_widget_name("preset-filters");
    group.set_homogeneous(true);
    for filter in filters_in_order() {
        let toggle = adw::Toggle::new();
        toggle.set_name(Some(filter.slug()));
        toggle.set_label(Some(&filter.label()));
        group.add(toggle);
    }
    group.set_active_name(Some(PresetFilter::All.slug()));
    group
}

/// The strip that holds the chips: its own horizontal scroller so a narrow window
/// clips nothing, and never part of the list it filters.
fn chips(group: &adw::ToggleGroup) -> gtk::ScrolledWindow {
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    strip.set_margin_top(6);
    strip.set_margin_bottom(6);
    strip.set_margin_start(12);
    strip.set_margin_end(12);
    strip.set_halign(gtk::Align::Center);
    strip.append(group);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    scroller.set_propagate_natural_height(true);
    scroller.set_vexpand(false);
    scroller.set_child(Some(&strip));
    scroller
}

/// The seven combo rows, Save and Delete, all hidden until «Personalizado» is
/// selected. Every row carries its own label from the catalogs.
fn build_editor() -> Editor {
    let group = adw::PreferencesGroup::new();
    // The seven rows and nothing else: the heading and the sentence belong to the
    // panel above, which shows them once ([`custom_editor_heading`]).
    let (heading, description) = custom_editor_heading();
    if let Some(heading) = heading {
        group.set_title(heading);
    }
    if let Some(description) = description {
        group.set_description(Some(description));
    }

    let container = combo("presets.detail_container");
    let video = combo("presets.detail_video_codec");
    let quality = combo("presets.detail_quality");
    let resolution = combo("presets.detail_resolution");
    let audio = combo("presets.detail_audio_codec");
    let bitrate = combo("presets.detail_audio_bitrate");
    let subtitles = combo("presets.detail_subtitles");
    for row in [
        &container,
        &video,
        &quality,
        &resolution,
        &audio,
        &bitrate,
        &subtitles,
    ] {
        group.add(row);
    }

    let save = gtk::Button::with_label(t("presets.custom_save"));
    save.add_css_class("suggested-action");
    save.add_css_class("pill");
    let delete = gtk::Button::with_label(t("presets.custom_delete"));
    delete.add_css_class("destructive-action");
    delete.add_css_class("pill");
    let status = gtk::Label::new(None);
    status.add_css_class("caption");
    status.add_css_class("dim-label");
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_visible(false);
    let failure = error_surface("presets-custom");

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.append(&save);
    actions.append(&delete);

    let area = gtk::Box::new(gtk::Orientation::Vertical, 8);
    area.append(&group);
    area.append(&actions);
    area.append(&failure.area);
    area.append(&status);
    area.set_visible(false);

    Editor {
        area,
        container,
        video,
        quality,
        resolution,
        audio,
        bitrate,
        subtitles,
        save,
        delete,
        status,
        failure,
    }
}

fn combo(title_key: &'static str) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title(t(title_key));
    row
}

impl Editor {
    /// Reads the seven rows as the draft they represent.
    fn draft(&self) -> CustomDraft {
        let defaults = CustomDraft::defaults();
        CustomDraft {
            container: pick(&CONTAINERS, &self.container).unwrap_or(defaults.container),
            video: pick(&VIDEOS, &self.video).unwrap_or(defaults.video),
            quality: pick(&CustomQualityLevel::ALL, &self.quality).unwrap_or(defaults.quality),
            resolution: pick(&RESOLUTIONS, &self.resolution).unwrap_or(defaults.resolution),
            audio: pick(&AUDIOS, &self.audio).unwrap_or(defaults.audio),
            bitrate: pick(&CustomBitrate::ALL, &self.bitrate).unwrap_or(defaults.bitrate),
            subtitles: pick(&SUBTITLES, &self.subtitles).unwrap_or(defaults.subtitles),
        }
    }

    /// Writes a draft into the seven rows, labels included.
    fn apply(&self, draft: CustomDraft) {
        fill(
            &self.container,
            &CONTAINERS,
            draft.container,
            container_label,
        );
        fill(&self.video, &VIDEOS, draft.video, video_label);
        fill(
            &self.quality,
            &CustomQualityLevel::ALL,
            draft.quality,
            quality_label,
        );
        fill(
            &self.resolution,
            &RESOLUTIONS,
            draft.resolution,
            resolution_label,
        );
        fill(&self.audio, &AUDIOS, draft.audio, audio_label);
        fill(
            &self.bitrate,
            &CustomBitrate::ALL,
            draft.bitrate,
            bitrate_label,
        );
        fill(
            &self.subtitles,
            &SUBTITLES,
            draft.subtitles,
            subtitles_label,
        );
    }
}

/// The value behind a combo row, or nothing when the model and the choices
/// disagree (which only a locale rebuild can cause, and never silently).
fn pick<T: Copy>(values: &[T], row: &adw::ComboRow) -> Option<T> {
    usize::try_from(row.selected())
        .ok()
        .and_then(|index| values.get(index).copied())
}

/// Sets the labels of `row` and selects `selected` in one step.
fn fill<T: Copy + PartialEq>(
    row: &adw::ComboRow,
    values: &[T],
    selected: T,
    label: fn(T) -> String,
) {
    let labels: Vec<String> = values.iter().copied().map(label).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    row.set_model(Some(&gtk::StringList::new(&refs)));
    let index = values
        .iter()
        .position(|value| *value == selected)
        .unwrap_or(0);
    row.set_selected(u32::try_from(index).unwrap_or(0));
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scrolled.set_child(Some(child));
    scrolled.set_vexpand(true);
    scrolled
}

/// `presets.list`, awaited off the GTK thread; failure is a page, not a crash.
fn load(
    client: &Rc<Client>,
    ui: &Ui,
    presets: &Rc<RefCell<Vec<Preset>>>,
    select: Option<&'static str>,
) {
    ui.stack.set_visible_child_name("loading");
    let (client, ui, presets) = (Rc::clone(client), ui.clone(), Rc::clone(presets));
    glib::spawn_future_local(apply_answer(client, ui, presets, select));
}

async fn apply_answer(
    client: Rc<Client>,
    ui: Ui,
    presets: Rc<RefCell<Vec<Preset>>>,
    select: Option<&'static str>,
) {
    let list = match client.call("presets.list", json!({})).await {
        Ok(value) => match serde_json::from_value::<Vec<Preset>>(value) {
            Ok(list) => list,
            Err(error) => {
                let message = t("presets.unreadable").replace("{error}", &error.to_string());
                fail_message(&ui, &message);
                return;
            }
        },
        Err(error) => {
            fail(&ui, &error);
            return;
        }
    };
    *presets.borrow_mut() = list;
    render(&ui, &presets, select, current_filter(&ui));
    ui.stack.set_visible_child_name("ready");
}

/// The chip the navigator has selected; «Todas» when it has none.
fn current_filter(ui: &Ui) -> PresetFilter {
    PresetFilter::from_slug(ui.filters.active_name().as_deref().unwrap_or_default())
}

/// A daemon failure: the human sentence is the visible description and the raw
/// text waits inside the disclosure.
fn fail(ui: &Ui, error: &JobError) {
    ui.error
        .set_description(Some(&human_error(error.code, &error.message)));
    ui.error_details
        .set_label(&technical_details(error.code, &error.message));
    ui.error_expander.set_visible(true);
    ui.error_expander.set_expanded(false);
    ui.stack.set_visible_child_name("error");
}

/// A sentence Cauce already wrote, with no technical part.
fn fail_message(ui: &Ui, message: &str) {
    ui.error.set_description(Some(message));
    ui.error_details.set_label("");
    ui.error_expander.set_visible(false);
    ui.stack.set_visible_child_name("error");
}

/// The editor's own failure: one human sentence and the reason behind the
/// disclosure. The reason comes from `CustomPreset::validate`.
fn fail_custom(ui: &Ui, error: &JobError) {
    // A rejected combination is the editor's own sentence; anything else (a dead
    // daemon, an unreadable answer) keeps the code's human message.
    let message = match error.code {
        ErrorCode::InvalidInput => t("presets.custom_invalid").to_owned(),
        other => human_error(other, &error.message),
    };
    show_parts(
        &ui.editor.failure,
        &message,
        Some(&technical_details(error.code, &error.message)),
    );
}

/// Rebuilds the list for `filter`, adds the «Personalizado» slot and selects
/// `select` (or the first row when that id is not in this view).
fn render(ui: &Ui, presets: &Rc<RefCell<Vec<Preset>>>, select: Option<&str>, filter: PresetFilter) {
    while let Some(child) = ui.list.first_child() {
        ui.list.remove(&child);
    }
    let presets = presets.borrow();
    let visible = filter_presets(&presets, filter);
    let mut custom_defined = false;
    for (category, items) in group_presets(&visible) {
        ui.list.append(&header_row(&category));
        for preset in items {
            if preset.category == CUSTOM_CATEGORY {
                custom_defined = true;
            }
            ui.list.append(&preset_row(preset));
        }
    }
    if !custom_defined && filter.offers_custom_slot() {
        // The group always exists in those views: without it there would be no way
        // to create the one preset the user is allowed to own.
        ui.list.append(&header_row(CUSTOM_CATEGORY));
        ui.list.append(&placeholder_row());
    }
    let wanted = select.unwrap_or_default();
    let target = row_named(&ui.list, wanted).or_else(|| first_row(&ui.list));
    if let Some(row) = target {
        ui.list.select_row(Some(&row));
    }
}

fn first_row(list: &gtk::ListBox) -> Option<gtk::ListBoxRow> {
    let mut child = list.first_child();
    while let Some(widget) = child {
        if let Ok(row) = widget.clone().downcast::<gtk::ListBoxRow>() {
            if row.is_selectable() {
                return Some(row);
            }
        }
        child = widget.next_sibling();
    }
    None
}

fn row_named(list: &gtk::ListBox, name: &str) -> Option<gtk::ListBoxRow> {
    let mut child = list.first_child();
    while let Some(widget) = child {
        if let Ok(row) = widget.clone().downcast::<gtk::ListBoxRow>() {
            if row.is_selectable() && row.widget_name() == name {
                return Some(row);
            }
        }
        child = widget.next_sibling();
    }
    None
}

/// A group header: not a row the user can select, only a section title.
fn header_row(category: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    let label = gtk::Label::new(Some(&category_label(category)));
    label.add_css_class("heading");
    label.set_xalign(0.0);
    label.set_margin_top(12);
    label.set_margin_bottom(4);
    label.set_margin_start(12);
    label.set_margin_end(12);
    row.set_child(Some(&label));
    row
}

fn preset_row(preset: &Preset) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    // The one preset the user owns does not carry the group name as a row label:
    // the header above the row already says «Personalizado»
    // (`odd/screenshots/07-presets-custom.png`).
    let name = match preset.category.as_str() {
        CUSTOM_CATEGORY => custom_row_label(),
        _ => localized_preset_name(preset),
    };
    row.set_title(&escaped(name));
    // One line per row: a wrapped title made the selected row taller than the rest
    // and pushed the list out of alignment (`L43`).
    row.set_title_lines(1);
    row.set_subtitle(&escaped(&format!(
        "{} · {}",
        preset.container.to_uppercase(),
        codec_summary(preset)
    )));
    row.set_subtitle_lines(1);
    row.set_tooltip_text(Some(name));
    // The id is the row's widget name, so the selection handler needs no map.
    row.set_widget_name(&preset.id);
    row
}

/// The row of an empty «Personalizado» group: it opens the editor on defaults.
fn placeholder_row() -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&escaped(custom_row_label()));
    row.set_title_lines(1);
    row.set_subtitle(&escaped(t(CUSTOM_HINT)));
    row.set_subtitle_lines(1);
    row.set_widget_name(CustomPreset::ID);
    row
}

fn codec_summary(preset: &Preset) -> String {
    match preset.video_codec.as_str() {
        "none" => t("presets.value_audio_only").replace("{codec}", &preset.audio_codec),
        video => format!("{video} + {}", preset.audio_codec),
    }
}

/// The detail pane of a system preset: name, one sentence, the five essentials
/// and, behind the closed disclosure, the rest of the technical values.
fn show_system(ui: &Ui, preset: &Preset) {
    ui.editor.area.set_visible(false);
    ui.notice.set_visible(true);
    ui.title.set_label(localized_preset_name(preset));
    ui.description
        .set_label(localized_preset_description(preset));
    fill_detail(ui, preset);
}

/// The detail pane of the custom preset: one title, one description and the
/// editor. The heading pair comes from [`custom_headers`] in both states — no
/// saved preset and a saved one — so the words are never said twice.
fn show_custom(ui: &Ui, preset: Option<&Preset>) {
    ui.editor.area.set_visible(true);
    ui.notice.set_visible(false);
    ui.editor.status.set_visible(false);
    clear_failure(&ui.editor.failure);
    let (title, description) = custom_headers();
    ui.title.set_label(title.unwrap_or_default());
    ui.description.set_label(description.unwrap_or_default());
    match preset {
        Some(preset) => {
            fill_detail(ui, preset);
            ui.editor.apply(CustomDraft::from_preset(preset));
            ui.editor.delete.set_sensitive(true);
        }
        None => {
            ui.essentials.set_visible(false);
            ui.technical.set_visible(false);
            ui.editor.apply(CustomDraft::defaults());
            ui.editor.delete.set_sensitive(false);
        }
    }
}

fn fill_detail(ui: &Ui, preset: &Preset) {
    fill_rows(&ui.essentials, preset_essentials(preset));
    fill_rows(&ui.detail_list, preset_technical_detail(preset));
    ui.essentials.set_visible(true);
    ui.technical.set_visible(true);
    // A new selection opens the disclosure closed, so the panel never inherits the
    // height of the last expansion (`L43`).
    ui.technical.set_expanded(false);
}

fn fill_rows(list: &gtk::ListBox, rows: Vec<(String, String)>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for (label, value) in rows {
        let row = adw::ActionRow::new();
        row.set_title(&escaped(&label));
        row.set_subtitle(&escaped(&value));
        row.set_subtitle_lines(0);
        list.append(&row);
    }
}
