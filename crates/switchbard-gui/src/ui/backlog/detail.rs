//! The selected-task detail pane: header, editable fields, rendered
//! description, dependencies/references, acceptance criteria + Definition of
//! Done checklists, implementation plan, notes, archive, and the read-only
//! final summary.
//!
//! Task-15 AC #3 parity note: the description renders as CommonMark by
//! default (`egui_commonmark`) with a raw-editor toggle, rather than always
//! showing a plain multiline `TextEdit` — matching the Backlog.md webview.

use super::{format, Pending, ProjectRow, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogEditorState;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use egui_commonmark::CommonMarkViewer;
use std::path::Path;
use switchbard_core::{BacklogTask, BacklogTaskPatch, BACKLOG_PRIORITIES, BACKLOG_STATUSES};

pub(super) fn render_task_detail(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let selected = app.backlog_view.selected_task.clone();
    let found = selected.as_ref().and_then(|(project_key, task_id)| {
        let project = snap.project(project_key)?;
        let task = project
            .project
            .tasks
            .iter()
            .find(|task| &task.id == task_id)?;
        Some((project, task))
    });
    let Some((project, task)) = found else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(egui::RichText::new("Select a task").strong());
        });
        return;
    };

    sync_editor(app, &project.key, task);
    let editable = task.editable() && project.project.cli_available();

    egui::ScrollArea::vertical()
        .id_salt("backlog_task_detail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_detail_header(ui, project, task, editable);
            ui.add_space(8.0);
            render_editor(app, ui, &project.key, task, editable, pending);
            ui.add_space(10.0);
            render_dependencies(app, ui, &project.key, task, editable, pending);
            ui.add_space(10.0);
            render_references(app, ui, &project.key, task, editable, pending);
            ui.add_space(10.0);
            render_acceptance(app, ui, &project.key, task, editable, pending);
            ui.add_space(10.0);
            render_definition_of_done(app, ui, &project.key, task, editable, pending);
            ui.add_space(10.0);
            render_notes(app, ui, &project.key, task, editable, pending);
            render_readonly_sections(ui, task);
            ui.add_space(10.0);
            render_archive(app, ui, &project.key, task, editable, pending);
        });
}

fn render_detail_header(
    ui: &mut egui::Ui,
    project: &ProjectRow,
    task: &BacklogTask,
    editable: bool,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&task.id)
                .monospace()
                .color(theme::MUTED_TEXT),
        );
        status_pill(ui, format::status_kind(&task.status), &task.status, None);
        ui.label(
            egui::RichText::new(format::priority_title(&task.priority))
                .color(format::priority_color(&task.priority)),
        );
        if !editable {
            status_pill(
                ui,
                StatusKind::Neutral,
                "read-only",
                Some("Only active backlog/tasks entries are edited through the CLI"),
            );
        }
    });
    ui.heading(&task.title);
    ui.label(
        egui::RichText::new(format!(
            "{} / {}",
            project.repo_name, project.worktree_label
        ))
        .small()
        .color(theme::MUTED_TEXT),
    )
    .on_hover_text(task.path.display().to_string());
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "created {}",
                task.created_date.as_deref().unwrap_or("unknown")
            ))
            .small()
            .color(theme::MUTED_TEXT),
        );
        ui.separator();
        ui.label(
            egui::RichText::new(format!(
                "updated {}",
                task.updated_date.as_deref().unwrap_or("unknown")
            ))
            .small()
            .color(theme::MUTED_TEXT),
        );
    });
    if !project.project.warnings.is_empty() {
        for warning in &project.project.warnings {
            ui.colored_label(theme::AMBER, warning);
        }
    }
}

fn render_editor(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Task").strong());
    let mut status_save: Option<String> = None;
    ui.add_enabled_ui(editable, |ui| {
        ui.label("title");
        ui.add(
            egui::TextEdit::singleline(&mut app.backlog_view.editor.title)
                .desired_width(f32::INFINITY),
        );

        ui.horizontal(|ui| {
            ui.label("status");
            if format::render_value_combo(
                ui,
                "backlog_task_status",
                &mut app.backlog_view.editor.status,
                BACKLOG_STATUSES,
                format::title_case_value,
            ) {
                status_save = Some(app.backlog_view.editor.status.trim().to_string());
            }
            ui.label("priority");
            format::render_value_combo(
                ui,
                "backlog_task_priority",
                &mut app.backlog_view.editor.priority,
                BACKLOG_PRIORITIES,
                format::priority_title,
            );
        });

        ui.horizontal(|ui| {
            ui.label("labels");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.labels)
                    .desired_width(260.0),
            );
            ui.label("assignees");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.assignees)
                    .desired_width(180.0),
            );
        });

        ui.horizontal(|ui| {
            ui.label("milestone");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.milestone)
                    .hint_text("none")
                    .desired_width(200.0),
            );
            if !app.backlog_view.editor.milestone.trim().is_empty()
                && ui
                    .small_button("Clear")
                    .on_hover_text("Clear the milestone assignment")
                    .clicked()
            {
                app.backlog_view.editor.milestone.clear();
            }
        });

        render_description_editor(app, ui);

        ui.label("implementation plan");
        ui.add(
            egui::TextEdit::multiline(&mut app.backlog_view.editor.plan)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
    });

    let mut patch = patch_from_editor(task, &app.backlog_view.editor);
    if let Some(new_status) =
        status_save.filter(|status| !status.eq_ignore_ascii_case(task.status.trim()))
    {
        pending.save = Some((
            project_root.to_path_buf(),
            task.id.clone(),
            BacklogTaskPatch {
                status: Some(new_status),
                ..Default::default()
            },
        ));
        patch.status = None;
        app.backlog_status
            .set(format!("updating {} status", task.id));
    }
    let can_save = editable && !patch.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_save, egui::Button::new("Save"))
            .on_hover_text("Save task fields through backlog task edit")
            .clicked()
        {
            pending.save = Some((project_root.to_path_buf(), task.id.clone(), patch));
        }
        if !editable {
            ui.label(
                egui::RichText::new("Backlog CLI edits are enabled for active tasks only.")
                    .color(theme::MUTED_TEXT),
            );
        }
    });
}

/// The description field: rendered CommonMark by default, with an Edit
/// toggle that swaps in the raw multiline editor bound to the same buffer.
fn render_description_editor(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("description");
        let toggle_label = if app.backlog_view.editor.description_editing {
            "View rendered"
        } else {
            "Edit raw"
        };
        if ui.small_button(toggle_label).clicked() {
            app.backlog_view.editor.description_editing =
                !app.backlog_view.editor.description_editing;
        }
    });
    if app.backlog_view.editor.description_editing {
        ui.add(
            egui::TextEdit::multiline(&mut app.backlog_view.editor.description)
                .desired_rows(6)
                .desired_width(f32::INFINITY),
        );
    } else if app.backlog_view.editor.description.trim().is_empty() {
        ui.label(egui::RichText::new("No description").color(theme::MUTED_TEXT));
    } else {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            CommonMarkViewer::new().show(
                ui,
                &mut app.commonmark_cache,
                &app.backlog_view.editor.description,
            );
        });
    }
}

fn render_dependencies(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Dependencies").strong());
    if task.dependencies.is_empty() {
        ui.label(egui::RichText::new("No dependencies").color(theme::MUTED_TEXT));
    } else {
        ui.label(task.dependencies.join(", "));
    }
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("edit").small().color(theme::MUTED_TEXT));
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.dependencies)
                    .hint_text("TASK-1, TASK-2")
                    .desired_width(220.0),
            );
            let new_deps = split_csv(&app.backlog_view.editor.dependencies);
            let changed = new_deps != task.dependencies;
            if ui
                .add_enabled(changed, egui::Button::new("Save"))
                .on_hover_text("Set dependencies through backlog task edit --depends-on")
                .clicked()
            {
                pending.save = Some((
                    project_root.to_path_buf(),
                    task.id.clone(),
                    BacklogTaskPatch {
                        dependencies: Some(new_deps),
                        ..Default::default()
                    },
                ));
            }
        });
    });
}

fn render_references(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("References").strong());
    if task.references.is_empty() {
        ui.label(egui::RichText::new("No references").color(theme::MUTED_TEXT));
    } else {
        for reference in &task.references {
            ui.hyperlink_to(reference, reference);
        }
    }
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.new_reference)
                    .hint_text("Add a reference (URL or note)")
                    .desired_width(260.0),
            );
            let candidate = app.backlog_view.editor.new_reference.trim().to_string();
            if ui
                .add_enabled(!candidate.is_empty(), egui::Button::new("Add"))
                .clicked()
            {
                let mut references = task.references.clone();
                references.push(candidate);
                pending.save = Some((
                    project_root.to_path_buf(),
                    task.id.clone(),
                    BacklogTaskPatch {
                        references: Some(references),
                        ..Default::default()
                    },
                ));
                app.backlog_view.editor.new_reference.clear();
            }
        });
    });
}

fn render_acceptance(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Acceptance Criteria").strong());
    if task.acceptance_criteria.is_empty() {
        ui.label(egui::RichText::new("No acceptance criteria").color(theme::MUTED_TEXT));
        return;
    }
    for item in &task.acceptance_criteria {
        let mut checked = item.checked;
        let response = ui
            .add_enabled_ui(editable, |ui| {
                ui.checkbox(&mut checked, format!("#{} {}", item.index, item.text))
            })
            .inner;
        if response.changed() {
            pending.toggle_ac = Some((
                project_root.to_path_buf(),
                task.id.clone(),
                item.index,
                checked,
            ));
            app.backlog_status
                .set(format!("updating {} AC #{}", task.id, item.index));
        }
    }
}

fn render_definition_of_done(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Definition of Done").strong());
    if task.definition_of_done.is_empty() {
        ui.label(egui::RichText::new("No Definition of Done items").color(theme::MUTED_TEXT));
        return;
    }
    for item in &task.definition_of_done {
        let mut checked = item.checked;
        let response = ui
            .add_enabled_ui(editable, |ui| {
                ui.checkbox(&mut checked, format!("#{} {}", item.index, item.text))
            })
            .inner;
        if response.changed() {
            pending.toggle_dod = Some((
                project_root.to_path_buf(),
                task.id.clone(),
                item.index,
                checked,
            ));
            app.backlog_status
                .set(format!("updating {} DoD #{}", task.id, item.index));
        }
    }
}

fn render_notes(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Implementation Notes").strong());
    if task.implementation_notes.trim().is_empty() {
        ui.label(egui::RichText::new("No notes yet").color(theme::MUTED_TEXT));
    } else {
        egui::ScrollArea::vertical()
            .id_salt(format!("notes_{}", task.id))
            .max_height(140.0)
            .show(ui, |ui| {
                ui.label(&task.implementation_notes);
            });
    }
    ui.add_space(4.0);
    ui.add_enabled_ui(editable, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut app.backlog_view.editor.note)
                .hint_text("Append note")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        let can_append = !app.backlog_view.editor.note.trim().is_empty();
        if ui
            .add_enabled(can_append, egui::Button::new("Append Note"))
            .clicked()
        {
            pending.append_note = Some((
                project_root.to_path_buf(),
                task.id.clone(),
                app.backlog_view.editor.note.trim().to_string(),
            ));
            app.backlog_view.editor.note.clear();
        }
    });
}

fn render_readonly_sections(ui: &mut egui::Ui, task: &BacklogTask) {
    if !task.final_summary.trim().is_empty() {
        egui::CollapsingHeader::new("Final Summary")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(&task.final_summary);
            });
    }
}

fn render_archive(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    if !editable {
        return;
    }
    ui.separator();
    if app.backlog_view.archive_confirm {
        ui.horizontal(|ui| {
            ui.colored_label(theme::AMBER, format!("Archive {}?", task.id));
            if ui.add(theme::danger_button("Confirm archive")).clicked() {
                pending.archive = Some((project_root.to_path_buf(), task.id.clone()));
                app.backlog_view.archive_confirm = false;
                app.backlog_status.set(format!("archiving {}", task.id));
            }
            if ui.button("Cancel").clicked() {
                app.backlog_view.archive_confirm = false;
            }
        });
    } else if ui
        .button("Archive")
        .on_hover_text("Move this task into backlog/archive/tasks")
        .clicked()
    {
        app.backlog_view.archive_confirm = true;
    }
}

fn sync_editor(app: &mut HiveApp, project_root: &Path, task: &BacklogTask) {
    let key = format!(
        "{}::{}::{}",
        project_root.display(),
        task.id,
        task.updated_date.as_deref().unwrap_or("")
    );
    if app.backlog_view.editor.loaded_key.as_deref() == Some(key.as_str()) {
        return;
    }
    app.backlog_view.editor.loaded_key = Some(key);
    app.backlog_view.editor.title = task.title.clone();
    app.backlog_view.editor.description = task.description.clone();
    app.backlog_view.editor.status = task.status.clone();
    app.backlog_view.editor.priority = task.priority.clone();
    app.backlog_view.editor.labels = task.labels.join(", ");
    app.backlog_view.editor.assignees = task.assignees.join(", ");
    app.backlog_view.editor.dependencies = task.dependencies.join(", ");
    app.backlog_view.editor.plan = task.implementation_plan.clone();
    app.backlog_view.editor.milestone = task.milestone.clone().unwrap_or_default();
    app.backlog_view.editor.note.clear();
    app.backlog_view.editor.description_editing = false;
    app.backlog_view.editor.new_reference.clear();
    app.backlog_view.archive_confirm = false;
}

fn patch_from_editor(task: &BacklogTask, editor: &BacklogEditorState) -> BacklogTaskPatch {
    let mut patch = BacklogTaskPatch::default();
    let title = editor.title.trim().to_string();
    if title != task.title {
        patch.title = Some(title);
    }
    let description = editor.description.trim().to_string();
    if description != task.description {
        patch.description = Some(description);
    }
    if !editor
        .status
        .trim()
        .eq_ignore_ascii_case(task.status.trim())
    {
        patch.status = Some(editor.status.trim().to_string());
    }
    if !editor
        .priority
        .trim()
        .eq_ignore_ascii_case(task.priority.trim())
    {
        patch.priority = Some(editor.priority.trim().to_string());
    }
    let labels = split_csv(&editor.labels);
    if labels != task.labels {
        patch.labels = Some(labels);
    }
    let assignees = split_csv(&editor.assignees);
    if assignees != task.assignees {
        patch.assignees = Some(assignees);
    }
    let plan = editor.plan.trim().to_string();
    if plan != task.implementation_plan {
        patch.implementation_plan = Some(plan);
    }
    let milestone = editor.milestone.trim();
    let current_milestone = task.milestone.as_deref().unwrap_or("");
    if milestone != current_milestone {
        if milestone.is_empty() {
            patch.clear_milestone = true;
        } else {
            patch.milestone = Some(milestone.to_string());
        }
    }
    patch
}

fn split_csv(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}
