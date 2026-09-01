//! The task list: column header, scrollable rows, bulk-select checkboxes,
//! sort controls, and the right-click bulk-action context menu. Rows nest
//! into a sub-task tree (task-17) — see `tree.rs` for how that's decided and
//! walked; this file owns rendering one row's actual columns.

use super::{dispatch_ui, format, selection, tree, Pending, TaskRow};
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
/// only in "All repos" scope, adds `REPO_COL_WIDTH` on top of this.
///
/// `pub(crate)`: `ui::places::tasks::list_body`'s column header computes its
/// own Task-label width against this same number (TASK-97 medic pass) —
/// named and shared rather than a second hardcoded `236.0` that could drift.
pub(crate) const TRAILING_COLS_WIDTH: f32 = 236.0;
pub(crate) const REPO_COL_WIDTH: f32 = 92.0;
/// The trailing AC-progress ("x/y" checked criteria) column's own width —
/// the narrow-width "Delivery column drops first" column (mock §7d, TASK-97
/// medic pass PARITY/SEVERE finding). Named so `task_col_width` and
/// `list_body`'s header can subtract it out exactly when `show_delivery` is
/// `false`, rather than re-deriving the number.
pub(crate) const AC_COL_WIDTH: f32 = 52.0;

/// Owner UX pass (2026-08-05): List used to embed its own left-list +
/// right-detail split, since it was the only lens with a detail pane at
/// all. Now that `rail::render_detail_rail` shows the same detail
/// persistently regardless of lens, List renders just the task list, at
/// full width — the shared rail is the one place detail renders.
pub(super) fn render_task_workspace(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    tasks: Vec<TaskRow<'_>>,
    pending: &mut Pending,
) {
    render_task_list(app, ui, tasks, pending);
}

fn render_task_list(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    tasks: Vec<TaskRow<'_>>,
    pending: &mut Pending,
) {
    selection::retain_visible_bulk_selection(app, &tasks);
    let show_repo = app.backlog_view.selected_repo.is_none();
    let visible_keys: Vec<BacklogTaskKey> = tasks.iter().map(TaskRow::key).collect();

    ui.horizontal(|ui| {
        render_select_all_checkbox(app, ui, &tasks);
        ui.add_sized(
            // The legacy List lens always shows the Delivery/AC column —
            // only `ui::places::tasks::list_body` drops it at narrow widths.
            [task_col_width(ui, show_repo, true), 18.0],
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

fn task_col_width(ui: &egui::Ui, show_repo: bool, show_delivery: bool) -> f32 {
    let mut trailing = if show_repo {
        TRAILING_COLS_WIDTH + REPO_COL_WIDTH
    } else {
        TRAILING_COLS_WIDTH
    };
    if !show_delivery {
        trailing -= AC_COL_WIDTH;
    }
    (ui.available_width() - trailing).max(140.0)
}

pub(crate) fn render_select_all_checkbox(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    tasks: &[TaskRow<'_>],
) {
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

/// Sort key + direction.
///
/// Rendered by the shared toolbar rather than by any one lens: every lens
/// draws from the same `sort::visible_task_rows`, so the ordering already
/// applied everywhere — only the *control* was List-only, which left Board
/// and Milestones silently sorted by a key their user could not see or
/// change.
pub(super) fn render_task_sort_controls(app: &mut HiveApp, ui: &mut egui::Ui) {
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
                    BacklogTaskSortKey::Labels,
                    BacklogTaskSortKey::Assignee,
                    BacklogTaskSortKey::Project,
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
pub(crate) fn render_task_list_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    all_visible: &[TaskRow<'_>],
    visible_keys: &[BacklogTaskKey],
    show_repo: bool,
    depth: usize,
    children: &[&BacklogTask],
    // TASK-97: the Tasks place's sub-issue tree is always expanded (decision
    // record Q9 = A) — no per-parent collapse affordance, just permanent
    // indentation (mock §7c shows no caret at all on a parent row). `true`
    // suppresses the caret/`expanded_parents` toggle below; every pre-
    // existing call site (the legacy List lens, still compiling per TASK-97's
    // binding directive) passes `false` to keep its collapse-by-default
    // behavior unchanged.
    always_expanded: bool,
    // TASK-97 medic pass (PARITY/SEVERE finding): the "Delivery" (AC
    // progress, "x/y") column — mock §7d's "below 720px the facet bar wraps
    // and the Delivery column drops first". `true` for every pre-existing
    // call site (the legacy List lens, and this place at ordinary widths);
    // `ui::places::tasks::list_body` passes `false` once the viewport drops
    // below `ui::nav::NARROW_WIDTH_THRESHOLD`.
    show_delivery: bool,
    pending: &mut Pending,
) {
    let task = row.task;
    let key = row.key();
    let detail_selected = app.backlog_view.selected_task.as_ref() == Some(&key);
    let bulk_selected = app.backlog_view.bulk_selected_tasks.contains(&key);
    let selected = detail_selected || bulk_selected;
    let title_width = task_col_width(ui, show_repo, show_delivery) - (depth as f32 * TREE_INDENT);
    // TASK-97/TASK-38: the stroke-based selection ring shared with the
    // Board lens's cards (`board::paint_card`) — same `theme::
    // selected_row_stroke` authority, applied here so a selected row never
    // reads as "just the title button highlighted" the way the old stock-
    // egui `.selected()` button style alone did.
    //
    // Stroke only, deliberately no `theme::selected_row_tint()` fill: a row
    // can carry a priority/status pill (`format::priority_color`'s
    // `warn_orange` for "High") painted directly on top of this frame's
    // background, and legibility_audit.rs's WCAG check caught that the
    // tint's composite pushes that already-marginal dark-theme color below
    // the 4.5:1 floor — the exact "translucent overlay produced a muddy
    // composite that failed WCAG AA on the dark theme" problem `board::
    // paint_card`'s own doc comment already names and avoids the same way.
    let row_frame = if selected {
        egui::Frame::NONE
            .stroke(theme::selected_row_stroke())
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(2, 1))
    } else {
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(2, 1))
    };
    let row_response = row_frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            if depth > 0 {
                ui.add_space(depth as f32 * TREE_INDENT);
            }
            if children.is_empty() || always_expanded {
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
            // TASK-97: the title never prefixes with the repo name, even when
            // `show_repo` also renders the separate badge column below — the
            // two used to be redundant (a title like "demo:TASK-2  …" next to
            // its own "demo" badge). `show_repo` now means exactly one thing:
            // whether the badge column renders (mock §2's bare "TASK-83" id
            // plus a lone `repo-badge` span, never both spellings of the repo
            // name on one row). The Tasks place passes `show_repo: true`
            // unconditionally (directive #9: repo badges on rows, always).
            let title_text = format!("{}  {}", task.id, task.title);
            let title_text = if children.is_empty() {
                title_text
            } else {
                let done = children.iter().filter(|c| c.is_done()).count();
                format!("{title_text}  [{done}/{}]", children.len())
            };
            // TASK-97 medic pass (PARITY finding): a List row stays a fixed
            // `ROW_HEIGHT` (`list_body.rs`'s uniform-row virtualization math
            // depends on it), so the title itself stays single-line
            // `.truncate()` rather than clamping to two lines in place — the
            // mock's own §7c two-line clamp is delivered on Board cards
            // instead (`board::paint_card`, bounded, never grows), and this
            // row's hover reveals what truncation hid: the full id+title
            // (+ roll-up suffix) line, then the description below it. See
            // `list_body.rs`'s module doc for why this option was chosen
            // over raising `ROW_HEIGHT` to fit two rows.
            let hover_text = if task.description.is_empty() {
                title_text.clone()
            } else {
                format!("{title_text}\n\n{}", task.description)
            };
            let resp = ui
                .add_sized(
                    [title_width, 26.0],
                    egui::Button::new(egui::RichText::new(title_text).strong())
                        .selected(selected)
                        .frame(false)
                        .truncate(),
                )
                .on_hover_text(hover_text);
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
                            row.repo.repo_name.clone(),
                            Some(&row.repo.label()),
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
            if show_delivery {
                ui.allocate_ui_with_layout(
                    egui::vec2(AC_COL_WIDTH, 26.0),
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
            }
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
            if !task.is_done() && is_blocked(task, &row.repo.repo) {
                status_pill(
                    ui,
                    StatusKind::Danger,
                    "blocked",
                    Some("Blocked by one or more open dependencies"),
                );
            }
            // TASK-97: moved here from the now-cut Projects lens's own row
            // renderer (`projects::render_row`) — "Expedited tasks surface per
            // the core order" (binding directive #5) needs a visible marker on
            // the row itself, not just in the detail rail's own lane pill, now
            // that the Projects lens no longer exists to have shown it.
            if row.repo.repo.ranking.is_expedited(&task.id) {
                status_pill(
                    ui,
                    StatusKind::Danger,
                    "expedited",
                    Some("In the expedite lane — jumps the repo's whole computed order"),
                );
            }
            dispatch_ui::render_dispatch_pill(ui, &dispatch_ui::dispatch_state(task));
        });
    });
    if row_response.response.secondary_clicked() {
        selection::focus_context_selection(app, key.clone());
    }
    row_response.response.context_menu(|ui| {
        render_task_context_menu(app, ui, row, all_visible, pending);
    });
    ui.separator();
}

/// TASK-26 (owner-requested UX): also called from `board::render_strip`'s
/// card context menu — the Move/Priority actions and the Board lens's own
/// selection state (`bulk_selected_tasks`, shared across both lenses) are
/// identical, so Board reuses this rather than a parallel implementation.
pub(crate) fn render_task_context_menu(
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
        ui.close();
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
        ui.close();
    }
}

/// A cross-repo bulk selection needs one `backlog` CLI invocation per repo
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
