//! The task list: column header, scrollable rows, bulk-select checkboxes,
//! sort controls, and the right-click bulk-action context menu. Rows nest
//! into a sub-task tree (task-17) — see `tree.rs` for how that's decided and
//! walked; this file owns rendering one row's actual columns.

use super::{format, selection, tree, Pending, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::runtime::{BacklogTaskKey, BacklogTaskSortKey};
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::path::PathBuf;
use switchbard_core::{
    is_blocked, BacklogTask, BacklogTaskPatch, BacklogTaskSource, BACKLOG_PRIORITIES,
    BACKLOG_STATUSES,
};

/// Indentation per tree depth level.
const TREE_INDENT: f32 = 20.0;

/// Width of the trailing checkbox + status + priority + AC columns (i.e.
/// everything to the right of the title). The repo badge column, present
/// only in "All projects" scope, adds `REPO_COL_WIDTH` on top of this.
const TRAILING_COLS_WIDTH: f32 = 236.0;
const REPO_COL_WIDTH: f32 = 92.0;

pub(super) fn render_task_workspace(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    tasks: Vec<TaskRow<'_>>,
    pending: &mut Pending,
) {
    let list_width = (ui.available_width() * 0.44).clamp(420.0, 620.0);
    let height = ui.available_height();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(list_width);
            ui.set_min_height(height);
            render_task_list(app, ui, tasks, pending);
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_min_height(height);
            super::detail::render_task_detail(app, ui, snap, pending);
        });
    });
}

fn render_task_list(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    tasks: Vec<TaskRow<'_>>,
    pending: &mut Pending,
) {
    render_task_sort_controls(app, ui);
    ui.add_space(4.0);
    selection::retain_visible_bulk_selection(app, &tasks);
    let show_repo = app.backlog_view.selected_project.is_none();
    let visible_keys: Vec<BacklogTaskKey> = tasks.iter().map(TaskRow::key).collect();

    ui.horizontal(|ui| {
        render_select_all_checkbox(app, ui, &tasks);
        ui.add_sized(
            [task_col_width(ui, show_repo), 18.0],
            egui::Label::new(
                egui::RichText::new("Task")
                    .small()
                    .color(theme::muted_text()),
            ),
        );
        if show_repo {
            ui.add_sized(
                [REPO_COL_WIDTH, 18.0],
                egui::Label::new(
                    egui::RichText::new("Repo")
                        .small()
                        .color(theme::muted_text()),
                ),
            );
        }
        ui.add_sized(
            [86.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Status")
                    .small()
                    .color(theme::muted_text()),
            ),
        );
        ui.add_sized(
            [62.0, 18.0],
            egui::Label::new(
                egui::RichText::new("Priority")
                    .small()
                    .color(theme::muted_text()),
            ),
        );
        ui.add_sized(
            [52.0, 18.0],
            egui::Label::new(egui::RichText::new("AC").small().color(theme::muted_text())),
        );
    });
    ui.separator();
    let child_keys = tree::child_keys_in_view(&tasks);
    egui::ScrollArea::vertical()
        .id_salt("backlog_task_list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let rendered = tasks.len();
            for row in &tasks {
                if child_keys.contains(&row.key()) {
                    continue; // rendered nested under its parent below
                }
                tree::render_task_tree_row(
                    app,
                    ui,
                    row,
                    &tasks,
                    &visible_keys,
                    show_repo,
                    0,
                    pending,
                );
            }
            if rendered == 0 {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("No tasks match the current filters").strong());
                ui.label(
                    egui::RichText::new("Adjust the filter, status, or priority.")
                        .color(theme::muted_text()),
                );
            }
        });
}

fn task_col_width(ui: &egui::Ui, show_repo: bool) -> f32 {
    let trailing = if show_repo {
        TRAILING_COLS_WIDTH + REPO_COL_WIDTH
    } else {
        TRAILING_COLS_WIDTH
    };
    (ui.available_width() - trailing).max(140.0)
}

fn render_select_all_checkbox(app: &mut HiveApp, ui: &mut egui::Ui, tasks: &[TaskRow<'_>]) {
    let all_selected = !tasks.is_empty()
        && tasks
            .iter()
            .all(|row| app.backlog_view.bulk_selected_tasks.contains(&row.key()));
    let mut checked = all_selected;
    let response = ui
        .add_sized([24.0, 18.0], egui::Checkbox::without_text(&mut checked))
        .on_hover_text("Select all visible tasks");
    if response.clicked() {
        if all_selected {
            for row in tasks {
                app.backlog_view.bulk_selected_tasks.remove(&row.key());
            }
            app.backlog_view.bulk_selection_anchor = None;
        } else {
            for row in tasks {
                app.backlog_view.bulk_selected_tasks.insert(row.key());
            }
            app.backlog_view.bulk_selection_anchor = tasks.first().map(TaskRow::key);
        }
    }
}

fn render_task_sort_controls(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Sort").color(theme::muted_text()));
        egui::ComboBox::from_id_salt("backlog_task_sort_key")
            .selected_text(app.backlog_view.sort_key.label())
            .width(118.0)
            .show_ui(ui, |ui| {
                for key in [
                    BacklogTaskSortKey::Triage,
                    BacklogTaskSortKey::Task,
                    BacklogTaskSortKey::Status,
                    BacklogTaskSortKey::Priority,
                    BacklogTaskSortKey::AcceptanceCriteria,
                ] {
                    ui.selectable_value(&mut app.backlog_view.sort_key, key, key.label());
                }
            });
        if ui
            .button(app.backlog_view.sort_direction.label())
            .on_hover_text("Toggle task list sort direction")
            .clicked()
        {
            app.backlog_view.sort_direction = app.backlog_view.sort_direction.toggled();
        }
        let selected_count = app.backlog_view.bulk_selected_tasks.len();
        if selected_count > 0 {
            ui.separator();
            ui.label(
                egui::RichText::new(format!("{selected_count} selected")).color(theme::weak_text()),
            );
            if ui
                .small_button("Clear")
                .on_hover_text("Clear selected tasks")
                .clicked()
            {
                app.backlog_view.bulk_selected_tasks.clear();
                app.backlog_view.bulk_selection_anchor = None;
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_task_list_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    all_visible: &[TaskRow<'_>],
    visible_keys: &[BacklogTaskKey],
    show_repo: bool,
    depth: usize,
    children: &[&BacklogTask],
    pending: &mut Pending,
) {
    let task = row.task;
    let key = row.key();
    let detail_selected = app.backlog_view.selected_task.as_ref() == Some(&key);
    let bulk_selected = app.backlog_view.bulk_selected_tasks.contains(&key);
    let selected = detail_selected || bulk_selected;
    let title_width = task_col_width(ui, show_repo) - (depth as f32 * TREE_INDENT);
    let row_response = ui.horizontal(|ui| {
        if depth > 0 {
            ui.add_space(depth as f32 * TREE_INDENT);
        }
        if children.is_empty() {
            ui.add_space(18.0); // keeps childless rows aligned with the caret column
        } else {
            let expanded = app.backlog_view.expanded_parents.contains(&key);
            if theme::caret_button(ui, expanded).clicked() {
                if expanded {
                    app.backlog_view.expanded_parents.remove(&key);
                } else {
                    app.backlog_view.expanded_parents.insert(key.clone());
                }
            }
        }
        let mut checked = bulk_selected;
        let checkbox = ui
            .add_sized([24.0, 26.0], egui::Checkbox::without_text(&mut checked))
            .on_hover_text("Select task for bulk actions");
        if checkbox.changed() {
            let shift = ui.input(|input| input.modifiers.shift);
            if shift {
                selection::select_bulk_task_range(app, visible_keys, key.clone());
            } else {
                selection::set_bulk_task_selected(app, key.clone(), checked);
            }
        }
        let title_text = if show_repo {
            format!("{}:{}  {}", row.project.repo_name, task.id, task.title)
        } else {
            format!("{}  {}", task.id, task.title)
        };
        let title_text = if children.is_empty() {
            title_text
        } else {
            let done = children.iter().filter(|c| c.is_done()).count();
            format!("{title_text}  [{done}/{}]", children.len())
        };
        let resp = ui
            .add_sized(
                [title_width, 26.0],
                egui::Button::new(egui::RichText::new(title_text).strong())
                    .selected(selected)
                    .frame(false)
                    .truncate(),
            )
            .on_hover_text(task.description.as_str());
        if resp.clicked() {
            let (shift, toggle_bulk) = ui.input(|input| {
                (
                    input.modifiers.shift,
                    input.modifiers.command || input.modifiers.ctrl,
                )
            });
            if shift {
                selection::select_bulk_task_range(app, visible_keys, key.clone());
            } else if toggle_bulk {
                selection::toggle_bulk_task_selection(app, key.clone());
            } else {
                app.backlog_view.selected_task = Some(key.clone());
                app.backlog_view.bulk_selection_anchor = Some(key.clone());
                app.backlog_view.editor.loaded_key = None;
            }
        }
        if show_repo {
            ui.allocate_ui_with_layout(
                egui::vec2(REPO_COL_WIDTH, 26.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    status_pill(
                        ui,
                        StatusKind::Neutral,
                        row.project.repo_name.clone(),
                        Some(&row.project.label()),
                    );
                },
            );
        }
        ui.allocate_ui_with_layout(
            egui::vec2(86.0, 26.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                status_pill(ui, format::status_kind(&task.status), &task.status, None);
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(62.0, 26.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(format::priority_title(&task.priority))
                        .small()
                        .color(format::priority_color(&task.priority)),
                );
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(52.0, 26.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if !task.acceptance_criteria.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}/{}",
                            task.acceptance_done_count(),
                            task.acceptance_criteria.len()
                        ))
                        .small()
                        .color(theme::muted_text()),
                    );
                } else {
                    ui.label(egui::RichText::new("-").small().color(theme::muted_text()));
                }
            },
        );
        if task.source != BacklogTaskSource::Active {
            ui.label(
                egui::RichText::new(task.source.label())
                    .small()
                    .color(theme::muted_text()),
            );
        }
        // task-18: a lamp-language marker (StatusKind::Danger → warn_orange,
        // the same "hot" tone Operator's Console uses for line/due alerts)
        // for tasks with at least one open dependency.
        if !task.is_done() && is_blocked(task, &row.project.project) {
            status_pill(
                ui,
                StatusKind::Danger,
                "blocked",
                Some("Blocked by one or more open dependencies"),
            );
        }
    });
    if row_response.response.secondary_clicked() {
        selection::focus_context_selection(app, key.clone());
    }
    row_response.response.context_menu(|ui| {
        render_task_context_menu(app, ui, row, all_visible, pending);
    });
    ui.separator();
}

fn render_task_context_menu(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    clicked: &TaskRow<'_>,
    all_visible: &[TaskRow<'_>],
    pending: &mut Pending,
) {
    let selected_keys = selection::selected_keys_for_menu(app, all_visible, clicked);
    let editable = selection::editable_keys(all_visible, &selected_keys);
    ui.label(format!(
        "{} selected · {} editable",
        selected_keys.len(),
        editable.len()
    ));
    if editable.len() < selected_keys.len() {
        ui.label(
            egui::RichText::new("Completed, archived, and draft tasks are skipped")
                .small()
                .color(theme::muted_text()),
        );
    }
    ui.separator();

    ui.label(
        egui::RichText::new("Move")
            .small()
            .color(theme::muted_text()),
    );
    for status in BACKLOG_STATUSES {
        let label = if status.eq_ignore_ascii_case("done") {
            "Mark Done".to_string()
        } else {
            format!("Move to {status}")
        };
        bulk_patch_button(
            app,
            ui,
            pending,
            &editable,
            label,
            BacklogTaskPatch {
                status: Some((*status).to_string()),
                ..Default::default()
            },
        );
    }

    ui.separator();
    ui.label(
        egui::RichText::new("Priority")
            .small()
            .color(theme::muted_text()),
    );
    for priority in BACKLOG_PRIORITIES {
        bulk_patch_button(
            app,
            ui,
            pending,
            &editable,
            format!("Set priority {}", format::priority_title(priority)),
            BacklogTaskPatch {
                priority: Some((*priority).to_string()),
                ..Default::default()
            },
        );
    }

    ui.separator();
    if ui.button("Clear selection").clicked() {
        app.backlog_view.bulk_selected_tasks.clear();
        app.backlog_view.bulk_selection_anchor = None;
        ui.close_menu();
    }
}

fn bulk_patch_button(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    pending: &mut Pending,
    editable: &[BacklogTaskKey],
    label: String,
    patch: BacklogTaskPatch,
) {
    if ui
        .add_enabled(!editable.is_empty(), egui::Button::new(&label))
        .clicked()
    {
        for (project_root, task_ids) in group_by_project(editable) {
            pending
                .bulk_save
                .push((project_root, task_ids, patch.clone(), label.clone()));
        }
        app.backlog_status
            .set(format!("{label}: updating {} task(s)", editable.len()));
        ui.close_menu();
    }
}

/// A cross-repo bulk selection needs one `backlog` CLI invocation per project
/// root (the CLI runs with that root as its working directory) — group the
/// selected keys accordingly before queuing the pending bulk-save entries.
fn group_by_project(keys: &[BacklogTaskKey]) -> Vec<(PathBuf, Vec<String>)> {
    let mut grouped: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for (project_root, task_id) in keys {
        match grouped.iter_mut().find(|(root, _)| root == project_root) {
            Some((_, ids)) => ids.push(task_id.clone()),
            None => grouped.push((project_root.clone(), vec![task_id.clone()])),
        }
    }
    grouped
}
