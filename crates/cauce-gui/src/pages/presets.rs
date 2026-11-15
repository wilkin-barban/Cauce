//! The Presets page: the thirteen system presets, grouped by category, with a
//! read-only detail pane (`docs/ui-mvp.md` §6, `docs/contracts.md` §2.3 and §6,
//! `docs/presets.md`).
//!
//! The catalog is immutable in v0.1: the page reads `presets.list` and offers no
//! create, edit or delete affordance at all. The decision core ([`group_presets`]
//! and [`preset_detail`]) contains no GTK type, so `tests/pages_contract.rs`
//! covers it without a display; the rest of this module is only its rendering.
//!
//! `docs/ui-mvp.md` §6 lists six group names for what `docs/contracts.md` §2.3
//! models as four `category` values, so each documented pair collapses into the
//! authoritative field: `tv` = Smart TV, `social` = Web y redes,
//! `archive` = Archivo y optimización, `audio` = Audio y música.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use adw::glib;
use adw::gtk;
use adw::prelude::*;
use cauce_core::Preset;
use libadwaita as adw;
use serde_json::json;

use crate::client::Client;
use crate::i18n::{
    codec_label, hdr_label, localized_preset_description, localized_preset_name, resolution,
    subtitle_label, t, yes_no,
};
use crate::pages::escaped;

/// Documented group order: Smart TV, Web y redes, Archivo, Audio. Any other
/// category goes last, in alphabetical order.
pub const CATEGORIES: [&str; 4] = ["tv", "social", "archive", "audio"];

/// Read-only notice, the only action the page does not offer.
const READ_ONLY: &str = "presets.read_only";

/// Header of one category; an unknown category keeps its own name.
pub fn category_label(category: &str) -> String {
    match category {
        "tv" => t("presets.group_tv").to_owned(),
        "social" => t("presets.group_social").to_owned(),
        "archive" => t("presets.group_archive").to_owned(),
        "audio" => t("presets.group_audio").to_owned(),
        other => other.to_owned(),
    }
}

/// Groups the catalog by `category`, in the documented order, keeping catalog
/// order inside each group. Unknown categories come last, alphabetically.
pub fn group_presets(presets: &[Preset]) -> Vec<(String, Vec<&Preset>)> {
    let mut buckets: BTreeMap<&str, Vec<&Preset>> = BTreeMap::new();
    for preset in presets {
        buckets
            .entry(preset.category.as_str())
            .or_default()
            .push(preset);
    }

    let mut groups: Vec<(String, Vec<&Preset>)> = Vec::with_capacity(buckets.len());
    for category in CATEGORIES {
        if let Some(items) = buckets.remove(category) {
            groups.push((category.to_owned(), items));
        }
    }
    // `BTreeMap` already yields the leftovers in alphabetical order.
    for (category, items) in buckets {
        groups.push((category.to_owned(), items));
    }
    groups
}

/// Label/value rows of the detail pane. A field that the preset leaves unset
/// produces no row at all, so nothing is ever shown as an empty value. The label
/// is a catalog key: the row says whatever the current locale says.
pub fn preset_detail(preset: &Preset) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::with_capacity(13);
    let mut push = |key: &'static str, value: String| rows.push((t(key).to_owned(), value));

    push(
        "presets.detail_name",
        localized_preset_name(preset).to_owned(),
    );
    push(
        "presets.detail_description",
        localized_preset_description(preset).to_owned(),
    );
    push("presets.detail_container", preset.container.clone());
    push(
        "presets.detail_video_codec",
        codec_label(&preset.video_codec),
    );
    push("presets.detail_audio_codec", preset.audio_codec.clone());
    if let Some(crf) = preset.video_quality {
        push("presets.detail_quality", crf.to_string());
    }
    if let Some(bitrate) = preset.audio_bitrate.as_ref() {
        push("presets.detail_audio_bitrate", bitrate.clone());
    }
    if let Some(resolution) = resolution(preset.max_width, preset.max_height) {
        push("presets.detail_resolution", resolution);
    }
    if let Some(fps) = preset.max_fps {
        push("presets.detail_fps", fps_label(fps));
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

/// Every widget the page needs after construction.
#[derive(Clone)]
struct Ui {
    page: adw::NavigationPage,
    stack: gtk::Stack,
    list: gtk::ListBox,
    error: adw::StatusPage,
    retry: gtk::Button,
    title: gtk::Label,
    description: gtk::Label,
    detail_list: gtk::ListBox,
}

/// Builds the page and asks the daemon for the catalog.
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
            let Some(preset) = presets
                .borrow()
                .iter()
                .find(|preset| preset.id == id.as_str())
                .cloned()
            else {
                return;
            };
            show_detail(&ui, &preset);
        }
    });
    ui.retry.connect_clicked({
        let (client, ui, presets) = (Rc::clone(client), ui.clone(), Rc::clone(&presets));
        move |_| load(&client, &ui, &presets)
    });

    load(client, &ui, &presets);
    ui.page
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

    let error = adw::StatusPage::new();
    error.set_icon_name(Some("dialog-warning-symbolic"));
    error.set_title(t("presets.error_title"));
    let retry = gtk::Button::with_label(t("presets.retry"));
    retry.add_css_class("pill");
    error.set_child(Some(&retry));

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    let sidebar = scrolled(&list);

    let title = gtk::Label::new(None);
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    title.set_wrap(true);
    let description = gtk::Label::new(None);
    description.add_css_class("dim-label");
    description.set_xalign(0.0);
    description.set_wrap(true);
    let detail_list = gtk::ListBox::new();
    detail_list.add_css_class("boxed-list");
    detail_list.set_selection_mode(gtk::SelectionMode::None);

    let notice = gtk::Label::new(Some(t(READ_ONLY)));
    notice.add_css_class("caption");
    notice.add_css_class("dim-label");
    notice.set_xalign(0.0);
    notice.set_wrap(true);

    let detail = gtk::Box::new(gtk::Orientation::Vertical, 12);
    detail.set_margin_top(18);
    detail.set_margin_bottom(18);
    detail.set_margin_start(18);
    detail.set_margin_end(18);
    detail.append(&title);
    detail.append(&description);
    detail.append(&detail_list);
    detail.append(&notice);

    let split = adw::OverlaySplitView::new();
    split.set_sidebar(Some(&sidebar));
    split.set_content(Some(&scrolled(&detail)));
    split.set_min_sidebar_width(220.0);
    split.set_max_sidebar_width(320.0);

    let stack = gtk::Stack::new();
    stack.add_named(&loading, Some("loading"));
    stack.add_named(&error, Some("error"));
    stack.add_named(&split, Some("ready"));
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
        error,
        retry,
        title,
        description,
        detail_list,
    }
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scrolled.set_child(Some(child));
    scrolled.set_vexpand(true);
    scrolled
}

/// `presets.list`, awaited off the GTK thread; failure is a page, not a crash.
fn load(client: &Rc<Client>, ui: &Ui, presets: &Rc<RefCell<Vec<Preset>>>) {
    ui.stack.set_visible_child_name("loading");
    let (client, ui, presets) = (Rc::clone(client), ui.clone(), Rc::clone(presets));
    glib::spawn_future_local(async move {
        let list = match client.call("presets.list", json!({})).await {
            Ok(value) => match serde_json::from_value::<Vec<Preset>>(value) {
                Ok(list) => list,
                Err(error) => {
                    fail(
                        &ui,
                        &t("presets.unreadable").replace("{error}", &error.to_string()),
                    );
                    return;
                }
            },
            Err(error) => {
                fail(&ui, &error.message);
                return;
            }
        };
        *presets.borrow_mut() = list;
        render(&ui, &presets);
        ui.stack.set_visible_child_name("ready");
    });
}

fn fail(ui: &Ui, message: &str) {
    ui.error.set_description(Some(message));
    ui.stack.set_visible_child_name("error");
}

/// Rebuilds the grouped list and selects the first preset.
fn render(ui: &Ui, presets: &Rc<RefCell<Vec<Preset>>>) {
    while let Some(child) = ui.list.first_child() {
        ui.list.remove(&child);
    }
    let presets = presets.borrow();
    let mut first: Option<adw::ActionRow> = None;
    for (category, items) in group_presets(&presets) {
        ui.list.append(&header_row(&category));
        for preset in items {
            let row = preset_row(preset);
            ui.list.append(&row);
            if first.is_none() {
                first = Some(row);
            }
        }
    }
    if let Some(row) = first {
        ui.list.select_row(Some(&row));
    }
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
    row.set_title(&escaped(localized_preset_name(preset)));
    row.set_subtitle(&escaped(&format!(
        "{} · {}",
        preset.container.to_uppercase(),
        codec_summary(preset)
    )));
    row.set_subtitle_lines(1);
    // The id is the row's widget name, so the selection handler needs no map.
    row.set_widget_name(&preset.id);
    row
}

fn codec_summary(preset: &Preset) -> String {
    match preset.video_codec.as_str() {
        "none" => t("presets.value_audio_only").replace("{codec}", &preset.audio_codec),
        video => format!("{video} + {}", preset.audio_codec),
    }
}

fn show_detail(ui: &Ui, preset: &Preset) {
    ui.title.set_label(localized_preset_name(preset));
    ui.description
        .set_label(localized_preset_description(preset));
    while let Some(child) = ui.detail_list.first_child() {
        ui.detail_list.remove(&child);
    }
    for (label, value) in preset_detail(preset) {
        let row = adw::ActionRow::new();
        row.set_title(&escaped(&label));
        row.set_subtitle(&escaped(&value));
        row.set_subtitle_lines(0);
        ui.detail_list.append(&row);
    }
}
