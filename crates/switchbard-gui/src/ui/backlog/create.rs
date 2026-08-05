//! The "New Backlog Task" modal. In single-project scope the target project
//! is fixed (a label only); in the All-projects scope it carries its own
//! project picker so filing a task doesn't require leaving the unified view.

use super::{detail_lists, format, Pending, Snapshot};
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{NewBacklogTask, BACKLOG_PRIORITIES, BACKLOG_STATUSES};

pub(super) fn render_create_modal(
    app: &mut HiveApp,
    ctx: &egui::Context,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    if !app.backlog_view.new_task.open {
        return;
    }
    let Some(target_key) = app.backlog_view.new_task.target_project.clone() else {
        app.backlog_view.new_task.open = false;
        return;
    };
    let Some(project) = snap.project(&target_key) else {
        app.backlog_view.new_task.open = false;
        return;
    };
    // A subtask (task-17) can't move to a different repo than its parent —
    // Backlog.md's `parent` field is a bare, project-scoped id — so the
    // project picker is fixed whenever this modal was opened via "+ Subtask"
    // even in the otherwise-unified All-projects scope.
    let fixed_target =
        app.backlog_view.selected_project.is_some() || app.backlog_view.new_task.parent.is_some();

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
                    egui::RichText::new(format!(
                        "{} / {}",
                        project.repo_name, project.worktree_label
                    ))
                    .color(theme::muted_text()),
                );
            } else {
                ui.label(egui::RichText::new("Project").color(theme::muted_text()));
                egui::ComboBox::from_id_salt("backlog_new_task_project")
                    .selected_text(project.label())
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for row in &snap.projects {
                            let selected = app.backlog_view.new_task.target_project.as_deref()
                                == Some(&row.key);
                            if ui.selectable_label(selected, row.label()).clicked() {
                                app.backlog_view.new_task.target_project = Some(row.key.clone());
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
                format::render_value_combo(
                    ui,
                    "backlog_new_status",
                    &mut app.backlog_view.new_task.status,
                    BACKLOG_STATUSES,
                    format::title_case_value,
                );
                ui.label("priority");
                format::render_value_combo(
                    ui,
                    "backlog_new_priority",
                    &mut app.backlog_view.new_task.priority,
                    BACKLOG_PRIORITIES,
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
                ui.label("milestone");
                ui.add(
                    egui::TextEdit::singleline(&mut app.backlog_view.new_task.milestone)
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
                let can_create = project.project.cli_available()
                    && !app.backlog_view.new_task.title.trim().is_empty();
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
                    let milestone = app.backlog_view.new_task.milestone.trim().to_string();
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
                            milestone: (!milestone.is_empty()).then_some(milestone),
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
                if !project.project.cli_available() {
                    ui.label(
                        egui::RichText::new("Backlog CLI is not available").color(theme::amber()),
                    );
                }
            });
        });
    app.backlog_view.new_task.open = open && !close;
}
