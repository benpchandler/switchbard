//! The summary line ("N open · M total", warnings) and the filter bar
//! (project/scope picker, status/priority filters, completed/archived
//! toggles).

use super::{format, reset_task_selection, sort, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogLens;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use switchbard_core::BACKLOG_PRIORITIES;

/// The lens tab strip shown under the summary line: List / Board /
/// Milestones / Statistics. Switching lenses does not clear the current
/// filters or selection — every lens reads the same `Snapshot`.
pub(super) fn render_lens_tabs(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for lens in [
            BacklogLens::List,
            BacklogLens::Board,
            BacklogLens::Milestones,
            BacklogLens::Statistics,
        ] {
            if ui
                .selectable_label(app.backlog_view.lens == lens, lens.label())
                .clicked()
            {
                app.backlog_view.lens = lens;
            }
        }
    });
}

pub(super) fn render_summary(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot) {
    let scoped = super::scoped_projects(app, snap);
    let task_count: usize = scoped.iter().map(|row| row.project.tasks.len()).sum();
    let open_count: usize = scoped
        .iter()
        .map(|row| sort::open_task_count(&row.project))
        .sum();
    let warning_count: usize = scoped.iter().map(|row| row.project.warnings.len()).sum();
    let ordering_warning = app.ordering_snapshot().warning;
    let all_scope = app.backlog_view.selected_project.is_none();

    ui.horizontal(|ui| {
        ui.heading("Backlog");
        ui.separator();
        let count_label = if all_scope {
            format!("All projects · {open_count} open · {task_count} total")
        } else {
            format!("{open_count} open · {task_count} total")
        };
        ui.label(egui::RichText::new(count_label).color(theme::weak_text()));
        if warning_count > 0 {
            ui.separator();
            status_pill(
                ui,
                StatusKind::Warn,
                format!(
                    "{warning_count} warning{}",
                    if warning_count == 1 { "" } else { "s" }
                ),
                Some("One or more Backlog projects loaded with warnings"),
            );
        }
        if let Some(warning) = &ordering_warning {
            ui.separator();
            status_pill(ui, StatusKind::Warn, "ordering.yml", Some(warning.as_str()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Refresh Backlog")
                .on_hover_text("Reload Backlog tasks from tracked worktrees")
                .clicked()
            {
                app.backlog_kick.notify();
                app.backlog_status.set("refreshing Backlog projects");
            }
            if ui
                .button("+ Task")
                .on_hover_text("Create a task in a Backlog project")
                .clicked()
            {
                let target = app
                    .backlog_view
                    .selected_project
                    .clone()
                    .or_else(|| snap.projects.first().map(|row| row.key.clone()));
                app.backlog_view.new_task.target_project = target;
                app.backlog_view.new_task.open = true;
            }
        });
    });
}

pub(super) fn render_project_toolbar(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    visible_count: usize,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Project").color(theme::muted_text()));
        ui.add(
            egui::TextEdit::singleline(&mut app.backlog_view.project_filter)
                .hint_text("Filter projects")
                .desired_width(180.0),
        );
        let project_filter_lc = app.backlog_view.project_filter.to_lowercase();
        let combo_label = app
            .backlog_view
            .selected_project
            .as_deref()
            .and_then(|key| snap.project(key))
            .map(|row| row.label())
            .unwrap_or_else(|| "All projects".to_string());
        egui::ComboBox::from_id_salt("backlog_project_picker")
            .selected_text(combo_label)
            .width(280.0)
            .show_ui(ui, |ui| {
                let mut shown = 0usize;
                if project_filter_lc.is_empty() || "all projects".contains(&project_filter_lc) {
                    shown += 1;
                    let selected = app.backlog_view.selected_project.is_none();
                    let total_open: usize = snap
                        .projects
                        .iter()
                        .map(|row| sort::open_task_count(&row.project))
                        .sum();
                    if ui
                        .selectable_label(selected, format!("All projects  ·  {total_open} open"))
                        .clicked()
                    {
                        app.backlog_view.selected_project = None;
                        reset_task_selection(app);
                    }
                }
                for row in &snap.projects {
                    if !row.matches_filter(&project_filter_lc) {
                        continue;
                    }
                    shown += 1;
                    let selected = app.backlog_view.selected_project.as_deref() == Some(&row.key);
                    let label = format!(
                        "{}  ·  {} open",
                        row.label(),
                        sort::open_task_count(&row.project)
                    );
                    if ui.selectable_label(selected, label).clicked() {
                        app.backlog_view.selected_project = Some(row.key.clone());
                        reset_task_selection(app);
                    }
                }
                if shown == 0 {
                    ui.label(
                        egui::RichText::new("No matching projects").color(theme::muted_text()),
                    );
                }
            });

        ui.separator();
        ui.label(egui::RichText::new("Status").color(theme::muted_text()));
        let statuses = sort::status_options(&super::scoped_projects(app, snap));
        egui::ComboBox::from_id_salt("backlog_status_filter")
            .selected_text(format::status_filter_label(&app.backlog_view.status_filter))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.backlog_view.status_filter,
                    "all".to_string(),
                    "All",
                );
                for status in statuses {
                    ui.selectable_value(
                        &mut app.backlog_view.status_filter,
                        status.clone(),
                        status,
                    );
                }
            });

        ui.label(egui::RichText::new("Priority").color(theme::muted_text()));
        egui::ComboBox::from_id_salt("backlog_priority_filter")
            .selected_text(format::priority_filter_label(
                &app.backlog_view.priority_filter,
            ))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.backlog_view.priority_filter,
                    "all".to_string(),
                    "All",
                );
                for priority in BACKLOG_PRIORITIES {
                    ui.selectable_value(
                        &mut app.backlog_view.priority_filter,
                        (*priority).to_string(),
                        format::priority_title(priority),
                    );
                }
            });

        ui.checkbox(&mut app.backlog_view.show_completed, "Done");
        ui.checkbox(&mut app.backlog_view.show_archived, "Archived");
        ui.checkbox(&mut app.backlog_view.show_drafts, "Drafts");
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{visible_count} visible")).color(theme::muted_text()),
        );
    });
}
