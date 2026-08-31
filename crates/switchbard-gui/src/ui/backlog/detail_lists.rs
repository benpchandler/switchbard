//! Detail-pane list/checklist sections split out of `detail.rs` to keep
//! both files under the repo's ~600 LOC ceiling for `ui/**` modules:
//! dependencies, references, acceptance criteria, Definition of Done,
//! implementation notes, the read-only final summary, and the archive
//! action. `split_csv` is a small shared helper `detail.rs` also uses for
//! its labels/assignees fields.

use super::dispatch_ui::{self, DispatchState};
use super::{format, Pending};
use crate::app::HiveApp;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::path::Path;
use switchbard_core::{BacklogRepo, BacklogTask, BacklogTaskPatch};

/// "Depends on" (task-18): each dependency shown with its own done/open
/// status, not just the bare id — `dependency_statuses` resolves them
/// against `repo` (Backlog.md dependency ids are repo-scoped, no repo
/// qualifier, so there's no cross-repo case to handle here).
pub(super) fn render_dependencies(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    repo: &BacklogRepo,
    editable: bool,
    pending: &mut Pending,
) {
    ui.label(egui::RichText::new("Dependencies").strong());
    if task.dependencies.is_empty() {
        ui.label(egui::RichText::new("No dependencies").color(theme::muted_text()));
    } else {
        for (dep, done) in switchbard_core::dependency_statuses(task, repo) {
            ui.horizontal(|ui| {
                ui.label(format!("{} {}", dep.id, dep.title));
                if done {
                    status_pill(ui, StatusKind::Good, "done", None);
                } else {
                    status_pill(ui, StatusKind::Danger, "open", None);
                }
            });
        }
        // Ids that didn't resolve to a task in this repo (typo, or a task
        // since archived/removed) — dependency_statuses omits them, so name
        // them here rather than silently dropping them from view.
        let unresolved: Vec<&str> = task
            .dependencies
            .iter()
            .filter(|id| !repo.tasks.iter().any(|t| &t.id == *id))
            .map(String::as_str)
            .collect();
        if !unresolved.is_empty() {
            ui.label(
                egui::RichText::new(format!("Unresolved: {}", unresolved.join(", ")))
                    .small()
                    .color(theme::muted_text()),
            );
        }
    }
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("edit")
                    .small()
                    .color(theme::muted_text()),
            );
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

/// Sub-tasks (task-17): direct children with a roll-up progress line, plus a
/// "+ Subtask" button that opens the "New Backlog Task" modal pre-filled
/// with this task as parent (`create::render_create_modal` reads `new_task.
/// parent`). Mutation (the actual create) goes through the same `backlog
/// task create -p` path every other creation uses — this section itself
/// only sets up the modal's pre-fill, mirroring how the List lens's tree
/// expand/collapse toggle works without a direct CLI call.
pub(super) fn render_subtasks(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    repo: &BacklogRepo,
    editable: bool,
) {
    let kids = switchbard_core::children(task, repo);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Sub-tasks").strong());
        if !kids.is_empty() {
            let done = kids.iter().filter(|k| k.is_done()).count();
            ui.label(
                egui::RichText::new(format!("{done}/{} done", kids.len()))
                    .color(theme::muted_text()),
            );
        }
        if editable
            && ui
                .small_button("+ Subtask")
                .on_hover_text("Create a subtask through backlog task create -p")
                .clicked()
        {
            app.backlog_view.new_task.target_project = Some(project_root.to_path_buf());
            app.backlog_view.new_task.parent = Some(task.id.clone());
            app.backlog_view.new_task.open = true;
        }
    });
    if kids.is_empty() {
        ui.label(egui::RichText::new("No sub-tasks").color(theme::muted_text()));
        return;
    }
    for child in kids {
        ui.horizontal(|ui| {
            ui.label(format!("{} {}", child.id, child.title));
            status_pill(ui, format::status_kind(&child.status), &child.status, None);
        });
    }
}

/// "Blocks" (task-18's reverse direction): every task in `repo` that
/// names this task as one of its own dependencies. Purely derived — there's
/// no CLI mutation for it, so this is read-only, unlike `render_dependencies`.
pub(super) fn render_blocks(ui: &mut egui::Ui, task: &BacklogTask, repo: &BacklogRepo) {
    ui.label(egui::RichText::new("Blocks").strong());
    let blocked = switchbard_core::blocks(task, repo);
    if blocked.is_empty() {
        ui.label(
            egui::RichText::new("No other tasks depend on this one").color(theme::muted_text()),
        );
        return;
    }
    for dependent in blocked {
        ui.horizontal(|ui| {
            ui.label(format!("{} {}", dependent.id, dependent.title));
            if dependent.is_done() {
                status_pill(ui, StatusKind::Good, "done", None);
            }
        });
    }
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
        ui.label(egui::RichText::new("No references").color(theme::muted_text()));
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
        ui.label(egui::RichText::new("No acceptance criteria").color(theme::muted_text()));
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
        ui.label(egui::RichText::new("No Definition of Done items").color(theme::muted_text()));
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
        ui.label(egui::RichText::new("No notes yet").color(theme::muted_text()));
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

/// Archive for a non-Done task's abandonment; Complete for a Done one —
/// Backlog.md semantics (verified against a real fixture repo, 2026-08-05
/// re-verification: the real CLI refuses `task archive` on a Done task,
/// "should be completed, not archived. Use: backlog task complete"). The
/// detail pane must not offer an action the CLI will reject, so the
/// affordance switches label, destination, and status wording based on
/// `task.is_done()` rather than always showing Archive. Both share
/// `archive_confirm` — they're mutually exclusive per task (only one ever
/// renders), so a second confirm flag would just duplicate this one.
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
    let done = task.is_done();
    let (button_label, hover, confirm_label, status_verb) = if done {
        (
            "Complete",
            "Move this task into backlog/completed/tasks",
            "Confirm complete",
            "completing",
        )
    } else {
        (
            "Archive",
            "Move this task into backlog/archive/tasks",
            "Confirm archive",
            "archiving",
        )
    };

    if app.backlog_view.archive_confirm {
        ui.horizontal(|ui| {
            ui.colored_label(theme::amber(), format!("{button_label} {}?", task.id));
            if ui.add(theme::danger_button(confirm_label)).clicked() {
                if done {
                    pending.complete = Some((project_root.to_path_buf(), task.id.clone()));
                } else {
                    pending.archive = Some((project_root.to_path_buf(), task.id.clone()));
                }
                app.backlog_view.archive_confirm = false;
                app.backlog_status.set(format!("{status_verb} {}", task.id));
            }
            if ui.button("Cancel").clicked() {
                app.backlog_view.archive_confirm = false;
            }
        });
    } else if ui.button(button_label).on_hover_text(hover).clicked() {
        app.backlog_view.archive_confirm = true;
    }
}

/// Refine (TASK-44): the grooming step immediately upstream of Dispatch, and
/// deliberately rendered next to it — a card you would not want to dispatch
/// yet is exactly the card this button is for.
///
/// One click, no confirmation, unlike its neighbor: a refine run writes no
/// code, opens no PR, and can only *add* to the task (see
/// `switchbard_core::refine`'s additive-apply contract and its write-path
/// guard — a section whose content the parser cannot round-trip is skipped,
/// never overwritten), so the consequence bar that earns Dispatch its inline
/// confirm isn't met here. The button disables itself while a run is in
/// flight for this task — `HiveApp::is_refining` reads the same in-memory set
/// `spawn_backlog_refine` guards on, so the affordance and the guard can't
/// disagree.
pub(super) fn render_refine(
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
    let in_flight = app.is_refining(&(project_root.to_path_buf(), task.id.clone()));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!in_flight, egui::Button::new("Refine"))
            .on_hover_text(
                "Explore the repo read-only with a headless agent and fill in the \
                 description, acceptance criteria, and implementation plan. Your \
                 existing text and checked criteria are kept: new content is \
                 appended after a \"Refined by Switchbard\" marker, never replaced. \
                 If this task's markdown contains anything the parser cannot safely \
                 round-trip, the description and plan are skipped rather than \
                 overwritten, and the status line says so.",
            )
            .clicked()
        {
            pending.refine = Some((project_root.to_path_buf(), task.id.clone()));
        }
        if in_flight {
            ui.label(
                egui::RichText::new("Refining… existing content is preserved.")
                    .color(theme::muted_text()),
            );
        }
    });
}

/// Dispatch (task-11 GUI wiring): a per-task, strictly opt-in affordance.
/// Flagging a task only sets its `dispatch` label through the CLI
/// (`Pending::dispatch_toggle` → `HiveApp::spawn_backlog_dispatch_toggle`);
/// everything after that — claiming it, the worktree, the headless `claude
/// -p` run, opening the PR — runs on `workers::spawn_dispatch`'s own
/// schedule, not from this click. State (queued/in-flight/dispatched/
/// failed) is read straight off the task's label + notes via `dispatch_ui`
/// — this function never invents its own status.
pub(super) fn render_dispatch(
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
    // No redundant "Dispatch" section header — matches `render_archive`'s
    // convention of letting the pill/button speak for the section, since a
    // header here would collide with the "Dispatch" button's own label in
    // the accessibility tree.
    ui.separator();
    let state = dispatch_ui::dispatch_state(task);
    match &state {
        DispatchState::NotFlagged => render_dispatch_toggle(app, ui, project_root, task, pending),
        DispatchState::Queued => {
            ui.horizontal(|ui| {
                dispatch_ui::render_dispatch_pill(ui, &state);
                ui.label(
                    egui::RichText::new("Waiting for the dispatch worker to pick this up.")
                        .color(theme::muted_text()),
                );
            });
            render_unflag_button(app, ui, project_root, task, pending);
        }
        DispatchState::InFlight => {
            ui.horizontal(|ui| {
                dispatch_ui::render_dispatch_pill(ui, &state);
                ui.label(
                    egui::RichText::new(
                        "A headless agent run is in progress in an isolated worktree.",
                    )
                    .color(theme::muted_text()),
                );
            });
        }
        DispatchState::Dispatched { pr_url } => {
            ui.horizontal(|ui| {
                dispatch_ui::render_dispatch_pill(ui, &state);
                match pr_url {
                    Some(url) => {
                        ui.hyperlink_to(url, url);
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("(PR link not found in notes)")
                                .color(theme::muted_text()),
                        );
                    }
                }
            });
        }
        DispatchState::Failed { reason } => {
            ui.horizontal(|ui| {
                dispatch_ui::render_dispatch_pill(ui, &state);
                ui.label(
                    egui::RichText::new(reason.as_deref().unwrap_or("(no reason recorded)"))
                        .color(theme::danger()),
                );
            });
            render_dispatch_toggle(app, ui, project_root, task, pending);
        }
    }
}

fn render_dispatch_toggle(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    pending: &mut Pending,
) {
    if app.backlog_view.dispatch_confirm {
        ui.horizontal(|ui| {
            ui.colored_label(
                theme::amber(),
                format!(
                    "Hand {} to a headless agent run in an isolated worktree?",
                    task.id
                ),
            );
            if ui
                .button("Confirm dispatch")
                .on_hover_text("Flags this task for the dispatch worker via backlog task edit")
                .clicked()
            {
                pending.dispatch_toggle = Some((project_root.to_path_buf(), task.id.clone(), true));
                app.backlog_view.dispatch_confirm = false;
                app.backlog_status
                    .set(format!("flagged {} for dispatch", task.id));
            }
            if ui.button("Cancel").clicked() {
                app.backlog_view.dispatch_confirm = false;
            }
        });
    } else if ui
        .button("Dispatch")
        .on_hover_text("Flag this task for an autonomous headless agent run")
        .clicked()
    {
        app.backlog_view.dispatch_confirm = true;
    }
}

fn render_unflag_button(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    project_root: &Path,
    task: &BacklogTask,
    pending: &mut Pending,
) {
    if ui
        .small_button("Unflag")
        .on_hover_text("Remove the dispatch label before the worker claims it")
        .clicked()
    {
        pending.dispatch_toggle = Some((project_root.to_path_buf(), task.id.clone(), false));
        app.backlog_status
            .set(format!("unflagged {} for dispatch", task.id));
    }
}

pub(super) fn split_csv(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}
