//! The "New Backlog Task" modal. In single-repo scope the target repo
//! is fixed (a label only); in the All-repos scope it carries its own
//! repo picker so filing a task doesn't require leaving the unified view.

use super::{detail_lists, format, Pending, Snapshot};
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use std::path::PathBuf;
use switchbard_core::{ordered_status_vocabulary, NewBacklogTask};

/// Open the top-level task composer, optionally preselecting a Board
/// column's status. All entry points share this so the global "+ Task"
/// control and per-column affordances cannot drift on repo targeting or
/// accidentally retain a subtask parent.
pub(super) fn open_new_task(app: &mut HiveApp, target_repo: Option<PathBuf>, status: Option<&str>) {
    app.backlog_view.new_task.target_repo = target_repo;
    app.backlog_view.new_task.parent = None;
    if let Some(status) = status {
        app.backlog_view.new_task.status = status.to_string();
    }
    app.backlog_view.new_task.open = true;
}

pub(super) fn render_create_modal(
    app: &mut HiveApp,
    ctx: &egui::Context,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    if !app.backlog_view.new_task.open {
        return;
    }
    let Some(target_key) = app.backlog_view.new_task.target_repo.clone() else {
        app.backlog_view.new_task.open = false;
        return;
    };
    let Some(repo) = snap.repo(&target_key) else {
        app.backlog_view.new_task.open = false;
        return;
    };
    // A subtask (task-17) can't move to a different repo than its parent —
    // Backlog.md's `parent` field is a bare, repo-scoped id — so the
    // repo picker is fixed whenever this modal was opened via "+ Subtask"
    // even in the otherwise-unified All-repos scope.
    let fixed_target =
        app.backlog_view.selected_repo.is_some() || app.backlog_view.new_task.parent.is_some();

    let mut open = true;
    let mut close = false;
    let title = match &app.backlog_view.new_task.parent {
        Some(parent) => format!("New Subtask of {parent}"),
        None => "New Backlog Task".to_string(),
    };
    egui::Window::new(title)
        // Stable id independent of the title (which varies with `parent`) so
        // this modal keeps one identity — position, focus — across opens.
        .id(egui::Id::new("backlog_new_task_modal"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if fixed_target {
                ui.label(
                    egui::RichText::new(format!("{} / {}", repo.repo_name, repo.worktree_label))
                        .color(theme::muted_text()),
                );
            } else {
                ui.label(egui::RichText::new("Repo").color(theme::muted_text()));
                egui::ComboBox::from_id_salt("backlog_new_task_project")
                    .selected_text(repo.label())
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for row in &snap.repos {
                            let selected =
                                app.backlog_view.new_task.target_repo.as_deref() == Some(&row.key);
                            if ui.selectable_label(selected, row.label()).clicked() {
                                app.backlog_view.new_task.target_repo = Some(row.key.clone());
                            }
                        }
                    });
            }
            ui.label("title");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.new_task.title)
                    .desired_width(520.0),
            );
            ui.label("description");
            ui.add(
                egui::TextEdit::multiline(&mut app.backlog_view.new_task.description)
                    .desired_rows(4)
                    .desired_width(520.0),
            );
            ui.horizontal(|ui| {
                ui.label("status");
                // Owner UX pass (2026-08-05): scoped to the target
                // repo's own vocabulary, not a fixed 3-entry list —
                // e.g. a repo declaring Icebox in config.yml can file a
                // new task straight into it.
                let statuses = ordered_status_vocabulary(std::iter::once(&repo.repo));
                format::render_value_combo(
                    ui,
                    "backlog_new_status",
                    &mut app.backlog_view.new_task.status,
                    &statuses,
                    format::title_case_value,
                );
                ui.label("priority");
                format::render_value_combo(
                    ui,
                    "backlog_new_priority",
                    &mut app.backlog_view.new_task.priority,
                    &format::priority_options(),
                    format::priority_title,
                );
            });
            ui.label("acceptance criteria");
            ui.add(
                egui::TextEdit::multiline(&mut app.backlog_view.new_task.acceptance_criteria)
                    .hint_text("One criterion per line")
                    .desired_rows(4)
                    .desired_width(520.0),
            );
            ui.horizontal(|ui| {
                ui.label("labels");
                ui.add(
                    egui::TextEdit::singleline(&mut app.backlog_view.new_task.labels)
                        .hint_text("comma, separated")
                        .desired_width(200.0),
                );
                ui.label("assignee");
                ui.add(
                    egui::TextEdit::singleline(&mut app.backlog_view.new_task.assignees)
                        .hint_text("comma, separated")
                        .desired_width(200.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("project");
                ui.add(
                    egui::TextEdit::singleline(&mut app.backlog_view.new_task.project)
                        .desired_width(200.0),
                );
                ui.label("dependencies");
                ui.add(
                    egui::TextEdit::singleline(&mut app.backlog_view.new_task.dependencies)
                        .hint_text("TASK-1, TASK-2")
                        .desired_width(200.0),
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_create = !app.backlog_view.new_task.title.trim().is_empty();
                if ui
                    .add_enabled(can_create, egui::Button::new("Create"))
                    .clicked()
                {
                    let criteria = app
                        .backlog_view
                        .new_task
                        .acceptance_criteria
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(str::to_string)
                        .collect();
                    let milestone = app.backlog_view.new_task.project.trim().to_string();
                    pending.create = Some((
                        target_key.clone(),
                        NewBacklogTask {
                            title: app.backlog_view.new_task.title.trim().to_string(),
                            description: app.backlog_view.new_task.description.trim().to_string(),
                            status: app.backlog_view.new_task.status.clone(),
                            priority: app.backlog_view.new_task.priority.clone(),
                            acceptance_criteria: criteria,
                            parent: app.backlog_view.new_task.parent.clone(),
                            labels: detail_lists::split_csv(&app.backlog_view.new_task.labels),
                            assignees: detail_lists::split_csv(
                                &app.backlog_view.new_task.assignees,
                            ),
                            project: (!milestone.is_empty()).then_some(milestone),
                            dependencies: detail_lists::split_csv(
                                &app.backlog_view.new_task.dependencies,
                            ),
                        },
                    ));
                    app.backlog_view.new_task = Default::default();
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    app.backlog_view.new_task.open = open && !close;
}
