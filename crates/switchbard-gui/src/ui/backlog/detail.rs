//! The selected-task detail pane: header and the editable field block
//! (title/status/priority/labels/assignees/milestone/description/plan).
//! The checklist/list sections (dependencies, references, acceptance
//! criteria, Definition of Done, notes, archive, read-only summary) live in
//! `detail_lists` — split out to keep this file under the repo's ~600 LOC
//! ceiling for `ui/**` modules as the parity work landed.
//!
//! Task-15 AC #3 parity note: the description renders as CommonMark by
//! default (`egui_commonmark`) with a raw-editor toggle, rather than always
//! showing a plain multiline `TextEdit` — matching the Backlog.md webview.

use super::{detail_lists, format, Pending, RepoRow, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogEditorState;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use egui_commonmark::CommonMarkViewer;
use std::path::Path;
use switchbard_core::{ordered_status_vocabulary, BacklogTask, BacklogTaskPatch};

pub(super) fn render_task_detail(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let selected = app.backlog_view.selected_task.clone();
    let found = selected.as_ref().and_then(|(project_key, task_id)| {
        let repo = snap.repo(project_key)?;
        let task = repo.repo.tasks.iter().find(|task| &task.id == task_id)?;
        Some((repo, task))
    });
    let Some((repo, task)) = found else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(egui::RichText::new("Select a task").strong());
        });
        return;
    };

    sync_editor(app, &repo.key, task);
    let editable = task.editable();

    egui::ScrollArea::vertical()
        .id_salt("backlog_task_detail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::default()
                .fill(theme::card_bg())
                .stroke(theme::surface_stroke())
                .corner_radius(7.0)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| render_detail_header(ui, repo, task, editable));
            ui.add_space(8.0);
            egui::Frame::default()
                .fill(theme::card_bg())
                .stroke(theme::surface_stroke())
                .corner_radius(7.0)
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    render_editor(app, ui, repo, task, editable, pending);
                });
            ui.add_space(10.0);
            detail_lists::render_subtasks(app, ui, &repo.key, task, &repo.repo, editable);
            ui.add_space(10.0);
            detail_lists::render_dependencies(
                app, ui, &repo.key, task, &repo.repo, editable, pending,
            );
            ui.add_space(10.0);
            detail_lists::render_blocks(ui, task, &repo.repo);
            ui.add_space(10.0);
            detail_lists::render_references(app, ui, &repo.key, task, editable, pending);
            ui.add_space(10.0);
            detail_lists::render_acceptance(app, ui, &repo.key, task, editable, pending);
            ui.add_space(10.0);
            detail_lists::render_definition_of_done(app, ui, &repo.key, task, editable, pending);
            ui.add_space(10.0);
            detail_lists::render_notes(app, ui, &repo.key, task, editable, pending);
            detail_lists::render_readonly_sections(ui, task);
            ui.add_space(10.0);
            detail_lists::render_refine(app, ui, &repo.key, task, editable, pending);
            detail_lists::render_dispatch(app, ui, &repo.key, task, editable, pending);
            ui.add_space(10.0);
            detail_lists::render_archive(app, ui, &repo.key, task, editable, pending);
        });
}

fn render_detail_header(ui: &mut egui::Ui, repo: &RepoRow, task: &BacklogTask, editable: bool) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&task.id)
                .monospace()
                .color(theme::muted_text()),
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
        if !task.is_done() && switchbard_core::is_blocked(task, &repo.repo) {
            status_pill(
                ui,
                StatusKind::Danger,
                "blocked",
                Some("Blocked by one or more open dependencies — see Dependencies below"),
            );
        }
    });
    ui.label(egui::RichText::new(&task.title).heading().strong());
    ui.label(
        egui::RichText::new(format!("{} / {}", repo.repo_name, repo.worktree_label))
            .small()
            .color(theme::muted_text()),
    )
    .on_hover_text(task.path.display().to_string());
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "created {}",
                task.created_date.as_deref().unwrap_or("unknown")
            ))
            .small()
            .color(theme::muted_text()),
        );
        ui.separator();
        ui.label(
            egui::RichText::new(format!(
                "updated {}",
                task.updated_date.as_deref().unwrap_or("unknown")
            ))
            .small()
            .color(theme::muted_text()),
        );
    });
    if !repo.repo.warnings.is_empty() {
        for warning in &repo.repo.warnings {
            ui.colored_label(theme::amber(), warning);
        }
    }
}

fn render_editor(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    repo: &RepoRow,
    task: &BacklogTask,
    editable: bool,
    pending: &mut Pending,
) {
    let project_root = &repo.key;
    ui.label(
        egui::RichText::new("Task fields")
            .strong()
            .color(theme::weak_text()),
    );
    ui.add_space(3.0);
    let mut status_save: Option<String> = None;
    ui.add_enabled_ui(editable, |ui| {
        ui.label("title");
        ui.add(
            egui::TextEdit::singleline(&mut app.backlog_view.editor.title)
                .desired_width(f32::INFINITY),
        );

        ui.columns(2, |columns| {
            columns[0].label("status");
            // Owner UX pass (2026-08-05): this task's own repo's shared
            // vocabulary, not a fixed 3-entry list — matches what Board and
            // the List filter now offer for the same repo.
            let statuses = ordered_status_vocabulary(std::iter::once(&repo.repo));
            if format::render_value_combo(
                &mut columns[0],
                "backlog_task_status",
                &mut app.backlog_view.editor.status,
                &statuses,
                format::title_case_value,
            ) {
                status_save = Some(app.backlog_view.editor.status.trim().to_string());
            }
            columns[1].label("priority");
            format::render_value_combo(
                &mut columns[1],
                "backlog_task_priority",
                &mut app.backlog_view.editor.priority,
                &format::priority_options(),
                format::priority_title,
            );
        });

        ui.columns(2, |columns| {
            columns[0].label("labels");
            columns[0].add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.labels)
                    .desired_width(f32::INFINITY),
            );
            columns[1].label("assignees");
            columns[1].add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.assignees)
                    .desired_width(f32::INFINITY),
            );
        });

        ui.label("milestone");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.editor.project)
                    .hint_text("none")
                    .desired_width(ui.available_width() - 50.0),
            );
            if !app.backlog_view.editor.project.trim().is_empty()
                && ui
                    .small_button("Clear")
                    .on_hover_text("Clear the milestone assignment")
                    .clicked()
            {
                app.backlog_view.editor.project.clear();
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
                    .color(theme::muted_text()),
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
        ui.label(egui::RichText::new("No description").color(theme::muted_text()));
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
    app.backlog_view.editor.project = task.project.clone().unwrap_or_default();
    app.backlog_view.editor.note.clear();
    app.backlog_view.editor.description_editing = false;
    app.backlog_view.editor.new_reference.clear();
    app.backlog_view.archive_confirm = false;
    app.backlog_view.dispatch_confirm = false;
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
    let labels = detail_lists::split_csv(&editor.labels);
    if labels != task.labels {
        patch.labels = Some(labels);
    }
    let assignees = detail_lists::split_csv(&editor.assignees);
    if assignees != task.assignees {
        patch.assignees = Some(assignees);
    }
    let plan = editor.plan.trim().to_string();
    if plan != task.implementation_plan {
        patch.implementation_plan = Some(plan);
    }
    let milestone = editor.project.trim();
    let current_milestone = task.project.as_deref().unwrap_or("");
    if milestone != current_milestone {
        if milestone.is_empty() {
            patch.clear_project = true;
        } else {
            patch.project = Some(milestone.to_string());
        }
    }
    patch
}
