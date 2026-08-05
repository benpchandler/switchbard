//! Presentation-only helpers shared by the toolbar, list, detail, and create
//! submodules: how a status/priority value is labeled and colored, and the
//! generic "pick one of these known values, or keep the current unrecognized
//! one" combo box used by both the editor and the create-task modal.

use crate::ui::components::StatusKind;
use crate::ui::theme;
use eframe::egui;

pub(super) fn status_kind(status: &str) -> StatusKind {
    match status.to_ascii_lowercase().as_str() {
        "done" => StatusKind::Good,
        "in progress" => StatusKind::Info,
        "blocked" => StatusKind::Danger,
        "to do" => StatusKind::Neutral,
        _ => StatusKind::Warn,
    }
}

pub(super) fn priority_title(priority: &str) -> String {
    match priority.to_ascii_lowercase().as_str() {
        "high" => "High".to_string(),
        "medium" => "Medium".to_string(),
        "low" => "Low".to_string(),
        _ => priority.to_string(),
    }
}

pub(super) fn priority_color(priority: &str) -> egui::Color32 {
    match priority.to_ascii_lowercase().as_str() {
        "high" => theme::WARN_ORANGE,
        "medium" => theme::SKY,
        "low" => theme::MUTED_TEXT,
        _ => theme::WEAK_TEXT,
    }
}

pub(super) fn status_filter_label(status: &str) -> String {
    if status == "all" {
        "All".to_string()
    } else {
        status.to_string()
    }
}

pub(super) fn priority_filter_label(priority: &str) -> String {
    if priority == "all" {
        "All".to_string()
    } else {
        priority_title(priority)
    }
}

pub(super) fn title_case_value(value: &str) -> String {
    value.to_string()
}

/// A combo box over a fixed set of known values that also tolerates (and
/// preserves) a value outside that set — the Backlog CLI accepts free-form
/// status/priority strings, so a task edited outside Switchbard can carry
/// one this view has never offered as an option.
pub(super) fn render_value_combo(
    ui: &mut egui::Ui,
    id: &'static str,
    value: &mut String,
    options: &[&str],
    label: fn(&str) -> String,
) -> bool {
    let before = value.clone();
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(value))
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(value, (*option).to_string(), label(option));
            }
            if !options
                .iter()
                .any(|option| option.eq_ignore_ascii_case(value))
            {
                ui.selectable_value(value, value.clone(), label(value));
            }
        });
    before != *value
}
