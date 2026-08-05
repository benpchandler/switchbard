//! Detail-pane list/checklist sections split out of `detail.rs` to keep
//! both files under the repo's ~600 LOC ceiling for `ui/**` modules:
//! dependencies, references, acceptance criteria, Definition of Done,
//! implementation notes, the read-only final summary, and the archive
//! action. `split_csv` is a small shared helper `detail.rs` also uses for
//! its labels/assignees fields.

use super::Pending;
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use std::path::Path;
use switchbard_core::{BacklogTask, BacklogTaskPatch};

pub(super) fn render_dependencies(
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

pub(super) fn render_references(
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

pub(super) fn render_acceptance(
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

pub(super) fn render_definition_of_done(
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

pub(super) fn render_notes(
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

pub(super) fn render_readonly_sections(ui: &mut egui::Ui, task: &BacklogTask) {
    if !task.final_summary.trim().is_empty() {
        egui::CollapsingHeader::new("Final Summary")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(&task.final_summary);
            });
    }
}

pub(super) fn render_archive(
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

pub(super) fn split_csv(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}
