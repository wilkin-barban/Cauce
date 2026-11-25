//! Per-file modal dialog for track selection and voice enhancement (T4).
//!
//! Presents audio track selection, voice enhancement and subtitle selection for a
//! single file (`docs/ui-mvp.md` §4, `odd/tasks/cauce-v0.2.0-remediacion-gui-tracks.md`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::gtk;
use adw::prelude::*;
use cauce_core::{AudioMixMode, MediaSnapshot, SubtitleMode};
use libadwaita as adw;

use crate::i18n::t;
use crate::pages::convert::state::base_name;
use crate::pages::convert::view::{audio_track_label, subtitle_track_label};
use crate::selection::FileSelection;

/// Presents a modal dialog to edit tracks and audio settings for a single file.
pub fn present(
    parent: &impl IsA<gtk::Widget>,
    snapshot: &MediaSnapshot,
    selection: &FileSelection,
    on_change: impl Fn(FileSelection) + 'static,
) -> adw::Dialog {
    let file_name = base_name(&snapshot.path);
    let dialog = adw::Dialog::new();
    dialog.set_title(&file_name);
    dialog.set_content_width(460);
    dialog.set_widget_name("track-dialog");

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    let title_widget = adw::WindowTitle::new(&file_name, "");
    header.set_title_widget(Some(&title_widget));

    let close_button = gtk::Button::with_label(t("convert.track_close"));
    close_button.set_widget_name("track-dialog-close");
    close_button.connect_clicked({
        let dialog = dialog.clone();
        move |_| {
            dialog.close();
        }
    });
    header.pack_end(&close_button);
    toolbar.add_top_bar(&header);

    let current = Rc::new(RefCell::new(selection.clone()));
    let on_change = Rc::new(on_change);
    let updating = Rc::new(Cell::new(false));

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_title(t("convert.track_dialog_title"));
    group.set_widget_name("track-dialog-group");

    // 1. Audio ComboRow (visible only when there is more than one audio stream)
    let audio_combo = adw::ComboRow::new();
    audio_combo.set_title(t("convert.track_audio"));
    audio_combo.set_widget_name("track-dialog-audio");

    let has_multiple_audio = snapshot.audio_streams.len() > 1;
    audio_combo.set_visible(has_multiple_audio);
    if has_multiple_audio {
        let labels: Vec<String> = snapshot
            .audio_streams
            .iter()
            .map(audio_track_label)
            .collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        audio_combo.set_model(Some(&gtk::StringList::new(&label_refs)));

        let selected_idx = selection
            .selection
            .audio_track_indices
            .first()
            .copied()
            .unwrap_or(snapshot.audio_streams[0].index);
        let pos = snapshot
            .audio_streams
            .iter()
            .position(|s| s.index == selected_idx)
            .unwrap_or(0);
        audio_combo.set_selected(pos as u32);
    }

    // 2. Voice enhancement row (switch disabled with subtitle when < 2 channels)
    let voice_row = adw::ActionRow::new();
    voice_row.set_title(t("convert.track_voice"));
    voice_row.set_widget_name("track-dialog-voice-row");

    let voice_switch = gtk::Switch::new();
    voice_switch.set_valign(gtk::Align::Center);
    voice_switch.set_widget_name("track-dialog-voice-switch");
    voice_row.add_suffix(&voice_switch);
    voice_row.set_activatable_widget(Some(&voice_switch));

    let selected_audio = if snapshot.audio_streams.is_empty() {
        None
    } else {
        let sel_idx = selection
            .selection
            .audio_track_indices
            .first()
            .copied()
            .unwrap_or(snapshot.audio_streams[0].index);
        snapshot
            .audio_streams
            .iter()
            .find(|s| s.index == sel_idx)
            .or_else(|| snapshot.audio_streams.first())
    };

    let channels = selected_audio.map(|s| s.channels).unwrap_or(0);
    if channels < 2 {
        voice_switch.set_sensitive(false);
        voice_switch.set_active(false);
        voice_row.set_subtitle(t("convert.voice_enhancement_mono"));
    } else {
        voice_switch.set_sensitive(true);
        voice_row.set_subtitle(t("convert.voice_enhancement_desc"));
        let is_boost = matches!(
            selection.selection.audio_mix_mode,
            AudioMixMode::StereoDialogueBoost | AudioMixMode::DualTrackUniversal
        );
        voice_switch.set_active(is_boost);
    }

    // 3. Subtitles ComboRow (first entry is always "Sin subtítulos" / SubtitleMode::Strip)
    let subtitle_combo = adw::ComboRow::new();
    subtitle_combo.set_title(t("convert.track_subtitles"));
    subtitle_combo.set_widget_name("track-dialog-subtitles");

    let mut sub_labels = vec![t("convert.track_none").to_owned()];
    for stream in &snapshot.subtitle_streams {
        sub_labels.push(subtitle_track_label(stream));
    }
    let sub_refs: Vec<&str> = sub_labels.iter().map(String::as_str).collect();
    subtitle_combo.set_model(Some(&gtk::StringList::new(&sub_refs)));

    if snapshot.subtitle_streams.is_empty() {
        subtitle_combo.set_sensitive(false);
        subtitle_combo.set_selected(0);
    } else {
        subtitle_combo.set_sensitive(true);
        let selected_pos = match selection.selection.subtitle_mode {
            SubtitleMode::Strip => 0,
            _ => selection
                .selection
                .subtitle_track_index
                .and_then(|idx| {
                    snapshot
                        .subtitle_streams
                        .iter()
                        .position(|s| s.index == idx)
                })
                .map(|pos| pos + 1)
                .unwrap_or(0),
        };
        subtitle_combo.set_selected(selected_pos as u32);
    }

    // Connect signals
    audio_combo.connect_selected_notify({
        let current = Rc::clone(&current);
        let on_change = Rc::clone(&on_change);
        let updating = Rc::clone(&updating);
        let snapshot = snapshot.clone();
        let voice_row = voice_row.clone();
        let voice_switch = voice_switch.clone();
        move |combo| {
            if updating.get() {
                return;
            }
            let idx = combo.selected() as usize;
            let Some(stream) = snapshot.audio_streams.get(idx) else {
                return;
            };
            let stream_idx = stream.index;
            {
                let mut sel = current.borrow_mut();
                sel.set_audio_track(stream_idx, Some(&snapshot));
            }
            updating.set(true);
            let sel = current.borrow();
            if stream.channels < 2 {
                voice_switch.set_sensitive(false);
                voice_switch.set_active(false);
                voice_row.set_subtitle(t("convert.voice_enhancement_mono"));
            } else {
                voice_switch.set_sensitive(true);
                voice_row.set_subtitle(t("convert.voice_enhancement_desc"));
                let is_boost = matches!(
                    sel.selection.audio_mix_mode,
                    AudioMixMode::StereoDialogueBoost | AudioMixMode::DualTrackUniversal
                );
                voice_switch.set_active(is_boost);
            }
            updating.set(false);
            on_change(current.borrow().clone());
        }
    });

    voice_switch.connect_active_notify({
        let current = Rc::clone(&current);
        let on_change = Rc::clone(&on_change);
        let updating = Rc::clone(&updating);
        move |switch| {
            if updating.get() {
                return;
            }
            let active = switch.is_active();
            let mode = if active {
                AudioMixMode::StereoDialogueBoost
            } else {
                AudioMixMode::Passthrough
            };
            current.borrow_mut().set_audio_mix_mode(mode);
            on_change(current.borrow().clone());
        }
    });

    subtitle_combo.connect_selected_notify({
        let current = Rc::clone(&current);
        let on_change = Rc::clone(&on_change);
        let updating = Rc::clone(&updating);
        let snapshot = snapshot.clone();
        move |combo| {
            if updating.get() {
                return;
            }
            let idx = combo.selected() as usize;
            if idx == 0 {
                let mut sel = current.borrow_mut();
                sel.set_subtitle_track(None);
                sel.set_subtitle_mode(SubtitleMode::Strip);
            } else if let Some(stream) = snapshot.subtitle_streams.get(idx - 1) {
                let mut sel = current.borrow_mut();
                sel.set_subtitle_track(Some(stream.index));
                if sel.selection.subtitle_mode == SubtitleMode::Strip {
                    sel.set_subtitle_mode(SubtitleMode::SoftEmbed);
                }
            }
            on_change(current.borrow().clone());
        }
    });

    group.add(&audio_combo);
    group.add(&voice_row);
    group.add(&subtitle_combo);
    page.add(&group);

    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
    dialog
}
