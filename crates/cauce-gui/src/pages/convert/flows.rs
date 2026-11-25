//! Interactions of the Convertir page: every daemon call and every widget signal.
//!
//! Each call is awaited through `glib::spawn_future_local`, so the GTK thread never
//! waits on the socket (`docs/architecture.md` §13). Adding to the queue is
//! `jobs.add` and nothing else: this page never calls `queue.resume` or
//! `jobs.resume`, that belongs to the Cola page (T17).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::{gio, glib, gtk, gtk::gdk};
use cauce_core::{Capabilities, ConversionPlan, Job, JobError, MediaSnapshot, Preset, Settings};
use libadwaita as adw;
use serde_json::json;

use super::state::{
    ConvertState, expand_dropped_folder, expand_dropped_paths, plan_details, plan_headline,
};
use super::view::{self, Ui, clear_plan, parent_window, text_line};
use crate::client::Client;
use crate::i18n::{human_error, localized_preset_name, t};

/// Connects every interaction of the page and loads the daemon's first answers.
///
/// A render is the repaint the row handlers call after the state changed; it is
/// stored behind a slot because the rows that carry the handler are built by the
/// very render the handler needs.
type Redraw = Rc<dyn Fn()>;
type RedrawSlot = Rc<RefCell<Option<Redraw>>>;

pub fn wire(client: &Rc<Client>, ui: &Ui, state: &Rc<RefCell<ConvertState>>) {
    let presets: Rc<RefCell<Vec<Preset>>> = Rc::new(RefCell::new(Vec::new()));
    // One removal per row and the plan preview both repaint, and the removal is
    // built into the rows that repaint, so the two live behind a slot the same way
    // the Cola page keeps its row handlers.
    let redraw: RedrawSlot = Rc::new(RefCell::new(None));
    let remove = removal_handle(state, &redraw);
    let raw_refresh = view::refresh(ui, state, &presets, &remove);
    let updating = Rc::new(std::cell::Cell::new(false));
    let refresh: Rc<dyn Fn()> = {
        let raw_refresh = Rc::clone(&raw_refresh);
        let updating = Rc::clone(&updating);
        Rc::new(move || {
            updating.set(true);
            raw_refresh();
            updating.set(false);
        })
    };
    let run_plan = plan_handle(ui, client, state);
    *redraw.borrow_mut() = Some({
        let (refresh, run_plan) = (Rc::clone(&refresh), Rc::clone(&run_plan));
        Rc::new(move || {
            refresh();
            run_plan();
        })
    });
    let run_inspect = inspect_handle(client, state, &refresh, &run_plan);
    let add_paths = add_handle(state, &refresh, &run_inspect);
    let run_queue = queue_handle(client, state, &refresh, &run_plan);
    let reload_presets = presets_handle(ui, client, state, &presets, &refresh, &run_plan);
    let open_files = picker_handle(ui, &add_paths, false);
    let open_folder = picker_handle(ui, &add_paths, true);

    ui.clear_all.connect_clicked({
        let (state, refresh, run_plan) =
            (Rc::clone(state), Rc::clone(&refresh), Rc::clone(&run_plan));
        move |_| {
            state.borrow_mut().clear_all();
            refresh();
            run_plan();
        }
    });
    ui.open_queue.connect_clicked(|button| {
        crate::pages::open_section(button, "queue");
    });

    for button in [&ui.drop_card, &ui.add_files] {
        let open_files = Rc::clone(&open_files);
        button.connect_clicked(move |_| open_files());
    }
    ui.add_folder.connect_clicked({
        let open_folder = Rc::clone(&open_folder);
        move |_| open_folder()
    });
    ui.queue_button.connect_clicked({
        let run_queue = Rc::clone(&run_queue);
        move |_| run_queue()
    });
    ui.output_button
        .connect_clicked(output_handle(ui, client, state, &refresh, &run_plan));
    ui.preset_combo.connect_selected_notify({
        let (state, presets) = (Rc::clone(state), Rc::clone(&presets));
        let (refresh, run_plan) = (Rc::clone(&refresh), Rc::clone(&run_plan));
        move |dropdown| {
            let selected_preset = usize::try_from(dropdown.selected())
                .ok()
                .and_then(|index| presets.borrow().get(index).cloned());
            let id = selected_preset.as_ref().map(|preset| preset.id.clone());
            let mut s = state.borrow_mut();
            s.set_preset(id);
            if let Some(p) = selected_preset {
                s.apply_preset(&p);
            }
            drop(s);
            refresh();
            run_plan();
        }
    });

    ui.voice_switch.connect_active_notify({
        let state = Rc::clone(state);
        let run_plan = Rc::clone(&run_plan);
        let updating = Rc::clone(&updating);
        move |switch| {
            if updating.get() {
                return;
            }
            let active = switch.is_active();
            let mode = if active {
                cauce_core::AudioMixMode::StereoDialogueBoost
            } else {
                cauce_core::AudioMixMode::Passthrough
            };
            let Ok(mut s) = state.try_borrow_mut() else {
                return;
            };
            if let Some(sel) = s.current_selection_mut() {
                if sel.selection.audio_mix_mode != mode {
                    sel.set_audio_mix_mode(mode);
                    drop(s);
                    run_plan();
                }
            }
        }
    });

    ui.audio_dropdown.connect_selected_notify({
        let state = Rc::clone(state);
        let updating = Rc::clone(&updating);
        let refresh = Rc::clone(&refresh);
        let run_plan = Rc::clone(&run_plan);
        move |dropdown| {
            if updating.get() {
                return;
            }
            let index = dropdown.selected() as usize;
            let Ok(mut s) = state.try_borrow_mut() else {
                return;
            };
            let first_snap = s.accepted().first().cloned();
            if let Some(snap) = first_snap {
                if let Some(stream) = snap.audio_streams.get(index) {
                    let stream_idx = stream.index;
                    if let Some(sel) = s.current_selection_mut() {
                        if sel.selection.audio_track_indices.first() != Some(&stream_idx) {
                            sel.set_audio_track(stream_idx, Some(&snap));
                            drop(s);
                            refresh();
                            run_plan();
                        }
                    }
                }
            }
        }
    });

    ui.subtitle_dropdown.connect_selected_notify({
        let state = Rc::clone(state);
        let updating = Rc::clone(&updating);
        let run_plan = Rc::clone(&run_plan);
        move |dropdown| {
            if updating.get() {
                return;
            }
            let index = dropdown.selected() as usize;
            let Ok(mut s) = state.try_borrow_mut() else {
                return;
            };
            let first_snap = s.accepted().first().cloned();
            if let Some(snap) = first_snap {
                if let Some(stream) = snap.subtitle_streams.get(index) {
                    let stream_idx = stream.index;
                    if let Some(sel) = s.current_selection_mut() {
                        if sel.selection.subtitle_track_index != Some(stream_idx) {
                            sel.set_subtitle_track(Some(stream_idx));
                            drop(s);
                            run_plan();
                        }
                    }
                }
            }
        }
    });

    ui.subtitle_soft_radio.connect_toggled({
        let state = Rc::clone(state);
        let updating = Rc::clone(&updating);
        let run_plan = Rc::clone(&run_plan);
        move |radio| {
            if updating.get() {
                return;
            }
            if radio.is_active() {
                let Ok(mut s) = state.try_borrow_mut() else {
                    return;
                };
                if let Some(sel) = s.current_selection_mut() {
                    if sel.selection.subtitle_mode != cauce_core::SubtitleMode::SoftEmbed {
                        sel.set_subtitle_mode(cauce_core::SubtitleMode::SoftEmbed);
                        drop(s);
                        run_plan();
                    }
                }
            }
        }
    });

    ui.subtitle_burn_radio.connect_toggled({
        let state = Rc::clone(state);
        let updating = Rc::clone(&updating);
        let run_plan = Rc::clone(&run_plan);
        move |radio| {
            if updating.get() {
                return;
            }
            if radio.is_active() {
                let Ok(mut s) = state.try_borrow_mut() else {
                    return;
                };
                if let Some(sel) = s.current_selection_mut() {
                    if sel.selection.subtitle_mode != cauce_core::SubtitleMode::BurnIn {
                        sel.set_subtitle_mode(cauce_core::SubtitleMode::BurnIn);
                        drop(s);
                        run_plan();
                    }
                }
            }
        }
    });

    // The whole card is the target. A desktop drop may carry several files and
    // several folders at once, so every `GFile` of the list is kept and each
    // folder is expanded one level (`docs/ui-mvp.md` §4).
    let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    target.connect_enter({
        let card = ui.drop_card.clone();
        move |_, _, _| {
            view::set_drag_over(&card, true);
            gdk::DragAction::COPY
        }
    });
    target.connect_motion(|_, _, _| gdk::DragAction::COPY);
    target.connect_leave({
        let card = ui.drop_card.clone();
        move |_| view::set_drag_over(&card, false)
    });
    target.connect_drop({
        let card = ui.drop_card.clone();
        let add_paths = Rc::clone(&add_paths);
        move |_, value, _, _| {
            view::set_drag_over(&card, false);
            let Ok(files) = value.get::<gdk::FileList>() else {
                return false;
            };
            let dropped: Vec<std::path::PathBuf> = files
                .files()
                .into_iter()
                .filter_map(|file| file.path())
                .collect();
            let paths = expand_dropped_paths(dropped);
            if paths.is_empty() {
                return false;
            }
            add_paths(paths);
            true
        }
    });
    ui.drop_card.add_controller(target);

    let shortcuts = gtk::ShortcutController::new();
    for (trigger, action) in [
        ("<Control>o", Rc::clone(&open_files)),
        ("<Control><Shift>o", Rc::clone(&open_folder)),
    ] {
        let Some(trigger) = gtk::ShortcutTrigger::parse_string(trigger) else {
            continue;
        };
        let callback = gtk::CallbackAction::new(move |_, _| {
            action();
            glib::Propagation::Stop
        });
        shortcuts.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(callback)));
    }
    ui.toolbar.add_controller(shortcuts);

    // The Presets page owns the custom preset: when it saves or deletes one, the
    // combo here is rebuilt from `presets.list` without a restart.
    crate::pages::on_presets_change({
        let reload_presets = Rc::clone(&reload_presets);
        Rc::new(move || reload_presets())
    });

    bootstrap(client, state, &reload_presets, &refresh, &run_plan);
    refresh();
}

/// The labels of the preset combo, in the order `presets.list` returns them. The
/// combo index is the index of the same slice, so the selected id is always the
/// preset the daemon planned for.
pub fn preset_choices(presets: &[Preset]) -> Vec<String> {
    presets
        .iter()
        .map(|preset| localized_preset_name(preset).to_owned())
        .collect()
}

/// `presets.list`, applied to the combo. The current choice survives a reload
/// when the daemon still offers it; otherwise the first preset wins.
fn presets_handle(
    ui: &Ui,
    client: &Rc<Client>,
    state: &Rc<RefCell<ConvertState>>,
    presets: &Rc<RefCell<Vec<Preset>>>,
    refresh: &Rc<dyn Fn()>,
    run_plan: &Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let (ui, client, state) = (ui.clone(), Rc::clone(client), Rc::clone(state));
    let (presets, refresh, run_plan) =
        (Rc::clone(presets), Rc::clone(refresh), Rc::clone(run_plan));
    Rc::new(move || {
        let (ui, client, state) = (ui.clone(), Rc::clone(&client), Rc::clone(&state));
        let (presets, refresh, run_plan) = (
            Rc::clone(&presets),
            Rc::clone(&refresh),
            Rc::clone(&run_plan),
        );
        glib::spawn_future_local(async move {
            let list = client
                .call("presets.list", json!({}))
                .await
                .ok()
                .and_then(|value| serde_json::from_value::<Vec<Preset>>(value).ok());
            let Some(list) = list else {
                return;
            };
            let wanted = state.borrow().preset_id().map(str::to_owned);
            let index = wanted
                .and_then(|id| list.iter().position(|preset| preset.id == id))
                .unwrap_or(0);
            let choices = preset_choices(&list);
            let labels: Vec<&str> = choices.iter().map(String::as_str).collect();
            ui.preset_combo
                .set_model(Some(&gtk::StringList::new(&labels)));
            let first_preset = list.first().cloned();
            let first = first_preset.as_ref().map(|preset| preset.id.clone());
            *presets.borrow_mut() = list;
            let mut s = state.borrow_mut();
            s.set_preset(first);
            if let Some(p) = first_preset {
                s.apply_preset(&p);
            }
            drop(s);
            ui.preset_combo
                .set_selected(u32::try_from(index).unwrap_or(0));
            refresh();
            run_plan();
        });
    })
}

/// Previews the plan of the first accepted input, with every `reasoning[]` line.
fn plan_handle(ui: &Ui, client: &Rc<Client>, state: &Rc<RefCell<ConvertState>>) -> Rc<dyn Fn()> {
    let (ui, client, state) = (ui.clone(), Rc::clone(client), Rc::clone(state));
    Rc::new(move || {
        clear_plan(&ui);
        let (snapshot, preset_id, track_selection, custom_options) = {
            let state = state.borrow();
            let sel = state.current_selection();
            (
                state.accepted().first().cloned(),
                state.preset_id().map(str::to_owned),
                sel.map(|s| s.selection.clone()),
                sel.map(|s| s.options.clone()),
            )
        };
        let (Some(snapshot), Some(preset_id)) = (snapshot, preset_id) else {
            return;
        };
        let (ui, client) = (ui.clone(), Rc::clone(&client));
        glib::spawn_future_local(async move {
            let mut params = json!({ "snapshot": snapshot, "preset_id": preset_id });
            if let Some(sel) = track_selection {
                if let Ok(v) = serde_json::to_value(sel) {
                    params["track_selection"] = v;
                }
            }
            if let Some(opt) = custom_options {
                if let Ok(v) = serde_json::to_value(opt) {
                    params["custom_options"] = v;
                }
            }
            let Ok(value) = client.call("media.plan", params).await else {
                return;
            };
            let Ok(plan) = serde_json::from_value::<ConversionPlan>(value) else {
                return;
            };
            // One visible line and a closed disclosure with the full summary and
            // every `reasoning[]` line (`L43`).
            let details = plan_details(&plan);
            ui.plan_summary_line.set_label(&plan_headline(&plan));
            ui.plan_details_line
                .set_label(details.first().map(String::as_str).unwrap_or_default());
            for reason in details.iter().skip(1) {
                let line = text_line("dim-label");
                line.set_label(reason);
                ui.plan_reasons.append(&line);
            }
            ui.plan_box.set_visible(true);
        });
    })
}

/// Inspects the pending paths; a failed batch goes to the rejected list.
fn inspect_handle(
    client: &Rc<Client>,
    state: &Rc<RefCell<ConvertState>>,
    refresh: &Rc<dyn Fn()>,
    run_plan: &Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let (client, state) = (Rc::clone(client), Rc::clone(state));
    let (refresh, run_plan) = (Rc::clone(refresh), Rc::clone(run_plan));
    Rc::new(move || {
        let pending = state.borrow().pending_paths().to_vec();
        if pending.is_empty() {
            return;
        }
        let (client, state) = (Rc::clone(&client), Rc::clone(&state));
        let (refresh, run_plan) = (Rc::clone(&refresh), Rc::clone(&run_plan));
        glib::spawn_future_local(async move {
            let params = json!({ "paths": pending });
            let failure = match client.call("media.inspect", params).await {
                Ok(value) => match serde_json::from_value::<Vec<MediaSnapshot>>(value) {
                    Ok(snapshots) => {
                        let mut state = state.borrow_mut();
                        for snapshot in snapshots {
                            state.accept_probe(snapshot);
                        }
                        None
                    }
                    Err(error) => {
                        Some(t("convert.unreadable").replace("{error}", &error.to_string()))
                    }
                },
                Err(error) => Some(human_error(error.code, &error.message)),
            };
            let message = failure.unwrap_or_else(|| t("convert.daemon_no_data").to_owned());
            state.borrow_mut().fail_pending(&message);
            refresh();
            run_plan();
        });
    })
}

/// Removes one accepted file and repaints; the plan is previewed again for the
/// file that is now first.
fn removal_handle(state: &Rc<RefCell<ConvertState>>, redraw: &RedrawSlot) -> Rc<dyn Fn(String)> {
    let (state, redraw) = (Rc::clone(state), Rc::clone(redraw));
    Rc::new(move |path: String| {
        state.borrow_mut().removal(&path);
        if let Some(redraw) = redraw.borrow().as_ref() {
            redraw();
        }
    })
}

fn add_handle(
    state: &Rc<RefCell<ConvertState>>,
    refresh: &Rc<dyn Fn()>,
    run_inspect: &Rc<dyn Fn()>,
) -> Rc<dyn Fn(Vec<String>)> {
    let (state, refresh, run_inspect) =
        (Rc::clone(state), Rc::clone(refresh), Rc::clone(run_inspect));
    Rc::new(move |paths: Vec<String>| {
        state.borrow_mut().add_inputs(paths);
        refresh();
        run_inspect();
    })
}

/// `jobs.add` with every accepted input, then a confirmation that names the count
/// and points at the queue: the selection is never cleared in silence.
fn queue_handle(
    client: &Rc<Client>,
    state: &Rc<RefCell<ConvertState>>,
    refresh: &Rc<dyn Fn()>,
    run_plan: &Rc<dyn Fn()>,
) -> Rc<dyn Fn()> {
    let (client, state) = (Rc::clone(client), Rc::clone(state));
    let (refresh, run_plan) = (Rc::clone(refresh), Rc::clone(run_plan));
    Rc::new(move || {
        let requests = state.borrow_mut().take_queue_requests();
        if requests.is_empty() {
            return;
        }
        refresh();
        let (client, state) = (Rc::clone(&client), Rc::clone(&state));
        let (refresh, run_plan) = (Rc::clone(&refresh), Rc::clone(&run_plan));
        glib::spawn_future_local(async move {
            let mut queued = 0usize;
            let mut first_error: Option<JobError> = None;
            for request in requests {
                let mut params = json!({
                    "inputs": request.inputs,
                    "output_dir": request.output_dir,
                    "preset_id": request.preset_id,
                });
                if let Some(sel) = request.track_selection {
                    if let Ok(v) = serde_json::to_value(sel) {
                        params["track_selection"] = v;
                    }
                }
                if let Some(opt) = request.custom_options {
                    if let Ok(v) = serde_json::to_value(opt) {
                        params["custom_options"] = v;
                    }
                }
                match client.call("jobs.add", params).await {
                    Ok(value) => {
                        queued += serde_json::from_value::<Vec<Job>>(value)
                            .map_or(request.inputs.len(), |jobs| jobs.len());
                    }
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
            }
            {
                let mut state = state.borrow_mut();
                state.confirm_queue(queued);
                if let Some(error) = first_error {
                    state.set_failure(&error);
                }
            }
            run_plan();
            refresh();
        });
    })
}

/// `Ctrl+O` picks files, `Ctrl+Shift+O` a folder; both feed `add_paths`.
fn picker_handle(ui: &Ui, add_paths: &Rc<dyn Fn(Vec<String>)>, folder: bool) -> Rc<dyn Fn()> {
    let (drop_card, add_paths) = (ui.drop_card.clone(), Rc::clone(add_paths));
    Rc::new(move || {
        let dialog = gtk::FileDialog::new();
        dialog.set_title(match folder {
            true => t("convert.add_folder"),
            false => t("convert.add_files"),
        });
        let parent = parent_window(&drop_card);
        let add_paths = Rc::clone(&add_paths);
        glib::spawn_future_local(async move {
            let paths = match folder {
                true => {
                    let Ok(chosen) = dialog.select_folder_future(parent.as_ref()).await else {
                        return;
                    };
                    chosen
                        .path()
                        .map(|path| expand_dropped_folder(&path))
                        .unwrap_or_default()
                }
                false => {
                    let Ok(files) = dialog.open_multiple_future(parent.as_ref()).await else {
                        return;
                    };
                    (0..files.n_items())
                        .filter_map(|index| files.item(index))
                        .filter_map(|item| item.downcast::<gio::File>().ok())
                        .filter_map(|file| file.path())
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect()
                }
            };
            if !paths.is_empty() {
                add_paths(paths);
            }
        });
    })
}

/// Chooses the output folder, records it for Ajustes and previews the plan again.
fn output_handle(
    ui: &Ui,
    client: &Rc<Client>,
    state: &Rc<RefCell<ConvertState>>,
    refresh: &Rc<dyn Fn()>,
    run_plan: &Rc<dyn Fn()>,
) -> impl Fn(&gtk::Button) + 'static {
    let (drop_card, client, state) = (ui.drop_card.clone(), Rc::clone(client), Rc::clone(state));
    let (refresh, run_plan) = (Rc::clone(refresh), Rc::clone(run_plan));
    move |_| {
        let dialog = gtk::FileDialog::new();
        dialog.set_title(t("convert.folder_dialog"));
        let parent = parent_window(&drop_card);
        let (client, state) = (Rc::clone(&client), Rc::clone(&state));
        let (refresh, run_plan) = (Rc::clone(&refresh), Rc::clone(&run_plan));
        glib::spawn_future_local(async move {
            let Ok(folder) = dialog.select_folder_future(parent.as_ref()).await else {
                return;
            };
            let Some(path) = folder.path() else {
                return;
            };
            let chosen = path.to_string_lossy().into_owned();
            state.borrow_mut().set_output_dir(Some(chosen.clone()));
            // The folder is persisted and shared, so Ajustes shows the folder the
            // user picked here instead of its own stale `settings.get` (`L43`).
            crate::pages::set_output_dir(&chosen);
            refresh();
            run_plan();
            let params = json!({ "output_dir": chosen });
            if let Err(error) = client.call("settings.set", params).await {
                state.borrow_mut().set_failure(&error);
                refresh();
            }
        });
    }
}

/// Capabilities, presets and the saved output folder, each tolerated on failure.
fn bootstrap(
    client: &Rc<Client>,
    state: &Rc<RefCell<ConvertState>>,
    reload_presets: &Rc<dyn Fn()>,
    refresh: &Rc<dyn Fn()>,
    run_plan: &Rc<dyn Fn()>,
) {
    // The catalog is not awaited here: one path fills the combo, so a saved
    // custom preset and the first load can never disagree.
    reload_presets();
    let (client, state) = (Rc::clone(client), Rc::clone(state));
    let (refresh, run_plan) = (Rc::clone(refresh), Rc::clone(run_plan));
    glib::spawn_future_local(async move {
        let capabilities = client
            .call("system.capabilities", json!({}))
            .await
            .ok()
            .and_then(|value| serde_json::from_value::<Capabilities>(value).ok());
        if let Some(capabilities) = capabilities {
            state.borrow_mut().apply_capabilities(&capabilities);
            refresh();
        }
        let settings = client
            .call("settings.get", json!({}))
            .await
            .ok()
            .and_then(|value| serde_json::from_value::<Settings>(value).ok());
        if let Some(settings) = settings {
            // A folder chosen here in this session wins over the daemon's snapshot,
            // which may be one write behind (`L43`).
            let dir = crate::pages::output_dir().unwrap_or(settings.output_dir);
            crate::pages::set_output_dir(&dir);
            state.borrow_mut().set_output_dir(Some(dir));
            refresh();
            run_plan();
        }
    });
}
