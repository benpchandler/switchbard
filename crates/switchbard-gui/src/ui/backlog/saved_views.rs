//! Named filter+sort+lens combinations (task-20): save the current Backlog
//! view under a name, pick a saved one back up later, or delete it.
//!
//! Repos stay the system of record for task data; a saved view is
//! engagement-only state — which tasks you're looking at, not the tasks
//! themselves — so it's persisted additively on `Config::ui.saved_views`,
//! the single existing config source of truth, rather than a new store.

use super::reset_task_selection;
use crate::app::HiveApp;
use crate::runtime::{BacklogLens, BacklogTaskSortDirection, BacklogTaskSortKey};
use crate::ui::theme;
use eframe::egui;
use switchbard_core::config::SavedView;

pub(super) fn render_saved_views_bar(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("View").color(theme::muted_text()));
        let current_label = app
            .backlog_view
            .active_saved_view
            .clone()
            .unwrap_or_else(|| "Unsaved".to_string());
        egui::ComboBox::from_id_salt("backlog_saved_views")
            .selected_text(current_label)
            .width(160.0)
            .show_ui(ui, |ui| {
                for view in app.config.ui.saved_views.clone() {
                    let selected =
                        app.backlog_view.active_saved_view.as_deref() == Some(view.name.as_str());
                    if ui.selectable_label(selected, &view.name).clicked() {
                        apply_saved_view(app, &view);
                    }
                }
                if app.config.ui.saved_views.is_empty() {
                    ui.label(egui::RichText::new("No saved views yet").color(theme::muted_text()));
                }
            });
        if app.backlog_view.active_saved_view.is_some()
            && ui
                .small_button("Delete")
                .on_hover_text("Delete this saved view")
                .clicked()
        {
            if let Some(name) = app.backlog_view.active_saved_view.take() {
                app.config.ui.saved_views.retain(|v| v.name != name);
                app.save_config();
            }
        }

        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut app.backlog_view.saved_view_name_draft)
                .hint_text("Save current as…")
                .desired_width(140.0),
        );
        let name = app.backlog_view.saved_view_name_draft.trim().to_string();
        if ui
            .add_enabled(!name.is_empty(), egui::Button::new("Save"))
            .on_hover_text("Save the current filters, sort, and lens under this name")
            .clicked()
        {
            let view = current_as_saved_view(app, name.clone());
            // Saving under an existing name overwrites it — "Save" doubles
            // as "update", matching how most apps treat named-view saving.
            app.config.ui.saved_views.retain(|v| v.name != name);
            app.config.ui.saved_views.push(view);
            app.backlog_view.active_saved_view = Some(name);
            app.backlog_view.saved_view_name_draft.clear();
            app.save_config();
        }
    });
}

fn current_as_saved_view(app: &HiveApp, name: String) -> SavedView {
    SavedView {
        name,
        selected_project: app.backlog_view.selected_project.clone(),
        status_filter: app.backlog_view.status_filter.clone(),
        priority_filter: app.backlog_view.priority_filter.clone(),
        sort_key: app.backlog_view.sort_key.as_saved_id().to_string(),
        sort_direction: app.backlog_view.sort_direction.as_saved_id().to_string(),
        lens: app.backlog_view.lens.as_saved_id().to_string(),
        show_completed: app.backlog_view.show_completed,
        show_archived: app.backlog_view.show_archived,
        show_drafts: app.backlog_view.show_drafts,
    }
}

fn apply_saved_view(app: &mut HiveApp, view: &SavedView) {
    app.backlog_view.selected_project = view.selected_project.clone();
    app.backlog_view.status_filter = view.status_filter.clone();
    app.backlog_view.priority_filter = view.priority_filter.clone();
    app.backlog_view.sort_key = BacklogTaskSortKey::from_saved_id(&view.sort_key);
    app.backlog_view.sort_direction = BacklogTaskSortDirection::from_saved_id(&view.sort_direction);
    app.backlog_view.lens = BacklogLens::from_saved_id(&view.lens);
    app.backlog_view.show_completed = view.show_completed;
    app.backlog_view.show_archived = view.show_archived;
    app.backlog_view.show_drafts = view.show_drafts;
    app.backlog_view.active_saved_view = Some(view.name.clone());
    reset_task_selection(app);
}
