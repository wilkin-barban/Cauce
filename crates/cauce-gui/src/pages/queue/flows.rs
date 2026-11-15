//! Interactions of the Cola page: every daemon call and every event subscription.
//!
//! Each call is awaited through `glib::spawn_future_local`, so the GTK thread never
//! waits on the socket (`docs/architecture.md` §13). Nothing polls on a timer: the
//! list is rebuilt from `jobs.list` on page build and on every `job.updated` and
//! `job.progress` event, while `queue.updated` carries the paused flag
//! (`docs/ui-mvp.md` §5 and §12, `docs/contracts.md` §7).
//!
//! `jobs.list` answers can arrive slower than the 2 Hz of `job.progress`, so the
//! refresh coalesces: at most one request is in flight and a burst of events
//! schedules at most one follow-up.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::{glib, gtk};
use cauce_core::{ErrorCode, Job, JobState, Preset, Progress};
use libadwaita as adw;
use serde_json::{Value, json};

use super::state::{JobAction, QueueState};
use super::view::{self, Row, Ui};
use crate::client::{self, Client};
use crate::i18n::{t, t_with};
use crate::pages::convert::base_name;

/// Catalog key of the message the banner and the error page show when the socket
/// is gone (`docs/ui-mvp.md` §9 and §12). The key, not the sentence.
pub const OFFLINE: &str = "banner.daemon_down";

/// Applies one action to one job: the action, its id and its base name.
type Act = Rc<dyn Fn(JobAction, String, String)>;

/// Opens the output folder (`true`) or the output file (`false`) of a job.
use super::view::Open;

type Render = Rc<dyn Fn()>;
type Refresh = Rc<dyn Fn()>;

/// Connects every interaction of the page and loads the daemon's first answers.
pub fn wire(client: &Rc<Client>, ui: &Ui) {
    let state = Rc::new(RefCell::new(QueueState::new()));
    let presets: Rc<RefCell<Vec<Preset>>> = Rc::new(RefCell::new(Vec::new()));
    let rows: Rc<RefCell<Vec<(String, Row)>>> = Rc::new(RefCell::new(Vec::new()));
    let in_flight = Rc::new(Cell::new(false));
    let queued = Rc::new(Cell::new(false));
    // The row buttons call these two, and rendering builds the buttons, so the
    // handlers are stored until they exist: no cycle, no `unsafe`.
    let act: Rc<RefCell<Option<Act>>> = Rc::new(RefCell::new(None));
    let open: Rc<RefCell<Option<Open>>> = Rc::new(RefCell::new(None));

    let render = build_render(ui, &state, &presets, &rows, &act, &open);
    let refresh = build_refresh(client, &state, &render, &in_flight, &queued);
    *act.borrow_mut() = Some(build_act(ui, client, &state, &refresh));
    *open.borrow_mut() = Some(view::build_open(ui));

    ui.queue_toggle.connect_clicked({
        let toggle = toggle_handle(ui, client, &state, &render);
        move |_| toggle()
    });
    ui.retry_button.connect_clicked({
        let refresh = Rc::clone(&refresh);
        move |_| refresh()
    });

    subscribe(client, &state, &render, &refresh);
    bootstrap(client, &presets);
    refresh();
}

fn preset_name(presets: &[Preset], id: &str) -> String {
    presets
        .iter()
        .find(|preset| preset.id == id)
        .map(|preset| preset.name_es.clone())
        .unwrap_or_else(|| id.to_owned())
}

/// What one row needs from the state.
struct Plan {
    job: Job,
    preset: String,
    actions: Vec<JobAction>,
}

/// Redraws the page; rows are reused unless the set of jobs changed, so keyboard
/// focus survives a progress event.
fn build_render(
    ui: &Ui,
    state: &Rc<RefCell<QueueState>>,
    presets: &Rc<RefCell<Vec<Preset>>>,
    rows: &Rc<RefCell<Vec<(String, Row)>>>,
    act: &Rc<RefCell<Option<Act>>>,
    open: &Rc<RefCell<Option<Open>>>,
) -> Render {
    let (ui, state, presets, rows) = (
        ui.clone(),
        Rc::clone(state),
        Rc::clone(presets),
        Rc::clone(rows),
    );
    let (act, open) = (Rc::clone(act), Rc::clone(open));
    Rc::new(move || {
        let (plans, changed) = {
            let state = state.borrow();
            let presets = presets.borrow();
            let plans: Vec<Plan> = state
                .jobs()
                .iter()
                .map(|job| Plan {
                    job: job.clone(),
                    preset: preset_name(&presets, &job.preset_id),
                    actions: state.allowed_actions(&job.id),
                })
                .collect();
            let current = rows.borrow();
            let changed = current.len() != plans.len()
                || current
                    .iter()
                    .zip(plans.iter())
                    .any(|((id, _), plan)| *id != plan.job.id);
            (plans, changed)
        };
        if changed {
            let mut built = Vec::with_capacity(plans.len());
            for plan in plans {
                let row = view::row(&plan.job, &plan.preset);
                for action in plan.actions {
                    let button = action_button(action, &plan.job, &act);
                    row.actions.append(&button);
                }
                if plan.job.state == JobState::Completed {
                    open_buttons(&row, &plan.job, &open);
                }
                built.push((plan.job.id.clone(), row));
            }
            view::clear_list(&ui);
            for (_, row) in built.iter() {
                ui.jobs_list.append(&row.row);
            }
            *rows.borrow_mut() = built;
        } else {
            let current = rows.borrow();
            for ((_, row), plan) in current.iter().zip(plans.iter()) {
                view::update_row(row, &plan.job);
            }
        }
        view::show_phase(&ui, &state.borrow());
    })
}

/// One icon-only button per allowed action, with its accessible name.
///
/// The visible glyph is never the name: the accessible label and the tooltip are
/// the translated action, so a card that shows only a ✕ still announces "Eliminar
/// de la cola" to assistive technology (`odd/tasks/cauce-0.1.md`, L43).
fn action_button(action: JobAction, job: &Job, act: &Rc<RefCell<Option<Act>>>) -> gtk::Button {
    let button = gtk::Button::from_icon_name(action.icon());
    button.add_css_class("flat");
    button.set_tooltip_text(Some(action.label()));
    button.update_property(&[gtk::accessible::Property::Label(action.label())]);
    let (act, id, name) = (Rc::clone(act), job.id.clone(), base_name(&job.input_path));
    button.connect_clicked(move |_| {
        let handler = act.borrow().clone();
        if let Some(handler) = handler {
            handler(action, id.clone(), name.clone());
        }
    });
    button
}

/// Open folder / open file for a finished job: text buttons with the same action
/// weight as the queue toggle, so they read as buttons and not as grey text. The
/// label is the accessible name and no shell is involved (`L43`).
fn open_buttons(row: &Row, job: &Job, open: &Rc<RefCell<Option<Open>>>) {
    for (key, folder) in [("queue.open_folder", true), ("queue.open_file", false)] {
        let label = t(key);
        let button = gtk::Button::with_label(label);
        button.add_css_class("pill");
        button.update_property(&[gtk::accessible::Property::Label(label)]);
        let (open, job) = (Rc::clone(open), job.clone());
        button.connect_clicked(move |_| {
            let handler = open.borrow().clone();
            if let Some(handler) = handler {
                handler(&job, folder);
            }
        });
        row.actions.append(&button);
    }
}

/// `jobs.list`, coalesced so a burst of events costs one call.
fn build_refresh(
    client: &Rc<Client>,
    state: &Rc<RefCell<QueueState>>,
    render: &Render,
    in_flight: &Rc<Cell<bool>>,
    queued: &Rc<Cell<bool>>,
) -> Refresh {
    let (client, state) = (Rc::clone(client), Rc::clone(state));
    let (render, in_flight, queued) = (Rc::clone(render), Rc::clone(in_flight), Rc::clone(queued));
    Rc::new(move || {
        if in_flight.replace(true) {
            queued.set(true);
            return;
        }
        let (client, state) = (Rc::clone(&client), Rc::clone(&state));
        let (render, in_flight, queued) = (
            Rc::clone(&render),
            Rc::clone(&in_flight),
            Rc::clone(&queued),
        );
        glib::spawn_future_local(async move {
            loop {
                queued.set(false);
                match client.call("jobs.list", json!({})).await {
                    Ok(value) => match serde_json::from_value::<Vec<Job>>(value) {
                        Ok(jobs) => state.borrow_mut().apply_jobs(jobs),
                        Err(error) => state.borrow_mut().set_error(
                            t("queue.unreadable").replace("{error}", &error.to_string()),
                        ),
                    },
                    Err(error) => state.borrow_mut().set_error(error.message),
                }
                render();
                in_flight.set(false);
                if !queued.replace(false) {
                    break;
                }
                in_flight.set(true);
            }
        });
    })
}

/// One action press: confirmation first when the action is destructive.
fn build_act(
    ui: &Ui,
    client: &Rc<Client>,
    state: &Rc<RefCell<QueueState>>,
    refresh: &Refresh,
) -> Act {
    let (ui, client, state) = (ui.clone(), Rc::clone(client), Rc::clone(state));
    let refresh = Rc::clone(refresh);
    Rc::new(move |action: JobAction, id: String, name: String| {
        let (ui_for_run, client, state) = (ui.clone(), Rc::clone(&client), Rc::clone(&state));
        let (refresh, name_for_run) = (Rc::clone(&refresh), name.clone());
        let run: Rc<dyn Fn()> = Rc::new(move || {
            let (ui, client, state) = (ui_for_run.clone(), Rc::clone(&client), Rc::clone(&state));
            let (refresh, name) = (Rc::clone(&refresh), name_for_run.clone());
            let job_id = id.clone();
            view::show_notice(&ui, None);
            glib::spawn_future_local(async move {
                let params = json!({ "id": job_id });
                match client.call(action.method(), params).await {
                    Ok(_) => view::show_notice(&ui, Some(success(action, &name))),
                    Err(error) => {
                        if error.code == ErrorCode::DaemonUnavailable {
                            state.borrow_mut().set_error(error.message.clone());
                        }
                        view::show_notice(&ui, Some(error.message));
                    }
                }
                refresh();
            });
        });
        if action.needs_confirmation() {
            view::confirm(&ui, action, &name, run);
        } else {
            run();
        }
    })
}

fn success(action: JobAction, name: &str) -> String {
    let key = match action {
        JobAction::Cancel => "queue.action_cancelling",
        JobAction::Remove => "queue.action_removed",
        JobAction::Pause => "queue.action_paused",
        JobAction::Resume => "queue.action_resumed",
        JobAction::Retry => "queue.action_requeued",
        JobAction::Recover => "queue.action_requeued",
    };
    t_with(key, "{name}", name)
}

/// `queue.pause` while the queue is active and `queue.start` while it is paused,
/// with the answer reflected at once (`docs/contracts.md` §6).
fn toggle_handle(
    ui: &Ui,
    client: &Rc<Client>,
    state: &Rc<RefCell<QueueState>>,
    render: &Render,
) -> Rc<dyn Fn()> {
    let (ui, client, state, render) = (
        ui.clone(),
        Rc::clone(client),
        Rc::clone(state),
        Rc::clone(render),
    );
    Rc::new(move || {
        // The state owns the name, so the label and the call can never disagree.
        let method = state.borrow().toggle();
        let pausing = method == "queue.pause";
        let (ui, client, state, render) = (
            ui.clone(),
            Rc::clone(&client),
            Rc::clone(&state),
            Rc::clone(&render),
        );
        glib::spawn_future_local(async move {
            match client.call(method, json!({})).await {
                Ok(value) => {
                    let paused = value
                        .get("paused")
                        .and_then(Value::as_bool)
                        .unwrap_or(pausing);
                    state.borrow_mut().set_queue_paused(paused);
                }
                Err(error) => {
                    if error.code == ErrorCode::DaemonUnavailable {
                        state.borrow_mut().set_error(error.message.clone());
                    }
                    view::show_notice(&ui, Some(error.message));
                }
            }
            render();
        });
    })
}

/// Events of `docs/contracts.md` §7 plus the client's own disconnect notice.
fn subscribe(
    client: &Rc<Client>,
    state: &Rc<RefCell<QueueState>>,
    render: &Render,
    refresh: &Refresh,
) {
    let events = client.events();
    let (state, render, refresh) = (Rc::clone(state), Rc::clone(render), Rc::clone(refresh));
    glib::spawn_future_local(async move {
        while let Ok(event) = events.recv().await {
            let method = event.get("method").and_then(Value::as_str).unwrap_or("");
            let params = event.get("params").cloned().unwrap_or(Value::Null);
            match method {
                client::DISCONNECTED_EVENT => {
                    state.borrow_mut().set_error(t(OFFLINE).to_owned());
                    render();
                }
                "queue.updated" => {
                    if let Some(paused) = params.get("paused").and_then(Value::as_bool) {
                        state.borrow_mut().set_queue_paused(paused);
                        render();
                    }
                }
                "job.progress" => {
                    apply_progress(&state, &params);
                    render();
                    refresh();
                }
                "job.updated" => refresh(),
                _ => {}
            }
        }
    });
}

fn apply_progress(state: &Rc<RefCell<QueueState>>, params: &Value) {
    let id = params.get("id").and_then(Value::as_str);
    let progress = params
        .get("progress")
        .cloned()
        .and_then(|value| serde_json::from_value::<Progress>(value).ok());
    if let (Some(id), Some(progress)) = (id, progress) {
        state.borrow_mut().apply_progress(id, progress);
    }
}

/// The preset names the rows show, then the first `jobs.list`.
fn bootstrap(client: &Rc<Client>, presets: &Rc<RefCell<Vec<Preset>>>) {
    let (client, presets) = (Rc::clone(client), Rc::clone(presets));
    glib::spawn_future_local(async move {
        let list = client
            .call("presets.list", json!({}))
            .await
            .ok()
            .and_then(|value| serde_json::from_value::<Vec<Preset>>(value).ok());
        if let Some(list) = list {
            *presets.borrow_mut() = list;
        }
    });
}
