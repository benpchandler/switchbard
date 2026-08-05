//! The Board lens: per-status kanban columns, cross-repo, with drag-to-change
//! status writing through the `backlog` CLI (task-15 AC #1).
//!
//! Columns come from `column_order`: the standard `BACKLOG_STATUSES`, every
//! scoped project's own `config.yml`-declared statuses (TASK-25 — e.g.
//! budget's Icebox, shown even with zero Icebox tasks right now), and any
//! nonstandard status actually present on a task in scope, so a repo with
//! custom Backlog.md statuses still gets a home for those tasks rather than
//! silently dropping them.
//!
//! Drag-and-drop uses egui's native `dnd_drag_source` / `dnd_drop_zone`
//! (payload = `BacklogTaskKey`, the same `(project_root, task_id)` pair used
//! for selection elsewhere). Only editable tasks (active source, CLI
//! available) are wrapped as drag sources — draft/completed/archived cards
//! render as plain, non-draggable strips.

use super::{
    dispatch_ui, format, list, scoped_projects, selection, sort, Pending, Snapshot, TaskRow,
};
use crate::app::HiveApp;
use crate::runtime::{BacklogLens, BacklogTaskKey};
use crate::ui::theme;
use eframe::egui;
use std::collections::BTreeSet;
use switchbard_core::{
    humanize_age, parse_backlog_datetime_unix, BacklogTask, BacklogTaskPatch, BACKLOG_STATUSES,
};

/// TASK-25 (owner-requested UX): the canonical kanban order for the
/// statuses a Backlog.md `config.yml` is likely to declare — budget's own
/// config declares exactly this set, `["Icebox", "To Do", "In Progress",
/// "In Review", "Done"]`. Any status outside this list (nonstandard,
/// project-specific) sorts after it, alphabetically.
const CANONICAL_STATUS_ORDER: &[&str] = &["Icebox", "To Do", "In Progress", "In Review", "Done"];

/// Column order: every status any *scoped* project either declares in its
/// own `backlog/config.yml` (`BacklogProject::configured_statuses` —
/// TASK-25) or has a task currently carrying, deduped case-insensitively,
/// in `CANONICAL_STATUS_ORDER` first and anything else alphabetical after.
/// Declaring a status in `config.yml` is enough to earn it a column even
/// with zero tasks in it right now — a repo-specific column like Icebox
/// shouldn't only appear once someone happens to file something there.
fn column_order(app: &HiveApp, snap: &Snapshot) -> Vec<String> {
    let scoped = scoped_projects(app, snap);
    let mut set: BTreeSet<String> = BACKLOG_STATUSES.iter().map(|s| (*s).to_string()).collect();
    for project in &scoped {
        for status in &project.project.configured_statuses {
            set.insert(status.clone());
        }
    }
    for status in sort::status_options(&scoped) {
        set.insert(status);
    }

    let mut canonical: Vec<String> = Vec::new();
    let mut extra: Vec<String> = Vec::new();
    for status in set {
        if CANONICAL_STATUS_ORDER
            .iter()
            .any(|c| c.eq_ignore_ascii_case(&status))
        {
            canonical.push(status);
        } else {
            extra.push(status);
        }
    }
    canonical.sort_by_key(|status| {
        CANONICAL_STATUS_ORDER
            .iter()
            .position(|c| c.eq_ignore_ascii_case(status))
            .unwrap_or(CANONICAL_STATUS_ORDER.len())
    });
    extra.sort();
    canonical.extend(extra);
    canonical
}

pub(super) fn render_board(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    tasks: Vec<TaskRow<'_>>,
    pending: &mut Pending,
) {
    // TASK-26: keeps bulk_selected_tasks consistent with whatever's
    // currently visible — same per-frame call `list::render_task_workspace`
    // already makes, since the two lenses share `bulk_selected_tasks`.
    selection::retain_visible_bulk_selection(app, &tasks);
    render_bulk_selection_bar(app, ui);

    let columns = column_order(app, snap);
    let show_repo = app.backlog_view.selected_project.is_none();

    egui::ScrollArea::horizontal()
        .id_salt("backlog_board")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for column_status in &columns {
                    render_column(app, ui, &tasks, column_status, show_repo, pending);
                }
            });
        });
}

/// TASK-26 (owner-requested UX): the same "N selected · Clear" indicator
/// `list::render_task_sort_controls` shows, since Board shares the identical
/// `bulk_selected_tasks` state. Its own row rather than folded into an
/// existing one — Board has no sort/toolbar row of its own to attach to.
fn render_bulk_selection_bar(app: &mut HiveApp, ui: &mut egui::Ui) {
    let selected_count = app.backlog_view.bulk_selected_tasks.len();
    if selected_count == 0 {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{selected_count} selected")).color(theme::weak_text()),
        );
        ui.label(
            egui::RichText::new("· right-click a card for bulk actions")
                .small()
                .color(theme::muted_text()),
        );
        if ui
            .small_button("Clear")
            .on_hover_text("Clear selected tasks")
            .clicked()
        {
            app.backlog_view.bulk_selected_tasks.clear();
            app.backlog_view.bulk_selection_anchor = None;
        }
    });
    ui.add_space(4.0);
}

const COLUMN_WIDTH: f32 = 260.0;

fn render_column(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    all_visible: &[TaskRow<'_>],
    column_status: &str,
    show_repo: bool,
    pending: &mut Pending,
) {
    let column_tasks: Vec<&TaskRow<'_>> = all_visible
        .iter()
        .filter(|row| row.task.status.eq_ignore_ascii_case(column_status))
        .collect();

    ui.vertical(|ui| {
        ui.set_width(COLUMN_WIDTH);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(column_status).strong());
            ui.label(
                egui::RichText::new(format!("{}", column_tasks.len())).color(theme::muted_text()),
            );
        });
        ui.separator();

        // `dnd_drop_zone` ignores the fill on the `Frame` it's handed and
        // always paints `visuals().widgets.{inactive,active}.bg_fill`
        // instead (see its source — it overwrites `frame.frame.fill` right
        // before painting). Overriding those two fields on this column's
        // child `Ui` is the only way to actually land our tuned `faint_bg`
        // instead of stock egui's default widget gray, which is what a
        // "No tasks" label would otherwise render against.
        ui.visuals_mut().widgets.inactive.bg_fill = theme::faint_bg();
        ui.visuals_mut().widgets.active.bg_fill = theme::faint_bg();
        let frame = egui::Frame::default().inner_margin(4.0);
        let (_, dropped) = ui.dnd_drop_zone::<BacklogTaskKey, ()>(frame, |ui| {
            ui.set_min_height(120.0);
            egui::ScrollArea::vertical()
                .id_salt(format!("backlog_board_col_{column_status}"))
                .max_height(ui.available_height().max(200.0))
                .show(ui, |ui| {
                    for row in &column_tasks {
                        render_strip(app, ui, row, all_visible, show_repo, pending);
                        ui.add_space(4.0);
                    }
                    if column_tasks.is_empty() {
                        ui.label(egui::RichText::new("No tasks").color(theme::muted_text()));
                    }
                });
        });

        if let Some(dropped_key) = dropped {
            apply_drop(app, all_visible, &dropped_key, column_status, pending);
        }
    });
}

fn apply_drop(
    app: &mut HiveApp,
    tasks: &[TaskRow<'_>],
    dropped_key: &BacklogTaskKey,
    column_status: &str,
    pending: &mut Pending,
) {
    let Some(row) = tasks.iter().find(|row| &row.key() == dropped_key) else {
        return;
    };
    if row.task.status.eq_ignore_ascii_case(column_status) {
        return;
    }
    if !(row.task.editable() && row.project.project.cli_available()) {
        app.backlog_status
            .set(format!("{} is read-only; drag ignored", row.task.id));
        return;
    }
    pending.save = Some((
        row.project.key.clone(),
        row.task.id.clone(),
        BacklogTaskPatch {
            status: Some(column_status.to_string()),
            ..Default::default()
        },
    ));
    app.backlog_status
        .set(format!("moving {} to {column_status}", row.task.id));
}

/// One "flight strip": a repo-colored rail, id/title, priority, and AC
/// progress. Draggable when the task is CLI-editable; otherwise a plain,
/// non-interactive frame with the same layout so the board doesn't jump
/// around depending on editability.
fn render_strip(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    all_visible: &[TaskRow<'_>],
    show_repo: bool,
    pending: &mut Pending,
) {
    let key = row.key();
    let editable = row.task.editable() && row.project.project.cli_available();
    let bulk_selected = app.backlog_view.bulk_selected_tasks.contains(&key);
    let selected = app.backlog_view.selected_task.as_ref() == Some(&key) || bulk_selected;
    // TASK-26: shift-range select needs the full visible order, same
    // "flatten once, reuse per click" shape list.rs's own row rendering
    // uses for its `visible_keys` parameter.
    let visible_keys: Vec<BacklogTaskKey> = all_visible.iter().map(TaskRow::key).collect();

    let mut paint_strip = |ui: &mut egui::Ui, app: &mut HiveApp| {
        // The fill is always `extreme_bg_color` — every text color rendered
        // inside a strip is tuned against that exact card color (see
        // `theme.rs`'s palette doc). Selection is a border color change
        // instead of a translucent overlay: layering `visuals().selection.
        // bg_fill` (untuned, stock egui) at partial alpha over the card
        // produced a muddy composite that failed WCAG AA on the dark
        // theme — a stroke can't create that problem since the audit only
        // measures fills and text, never strokes.
        let frame = egui::Frame::default()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(if selected {
                egui::Stroke::new(2.0, theme::sky())
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke
            })
            .corner_radius(3.0)
            .inner_margin(egui::Margin::symmetric(8, 6));
        let resp = frame
            .show(ui, |ui| {
                ui.set_width(COLUMN_WIDTH - 16.0);
                ui.horizontal(|ui| {
                    // TASK-26 (owner-requested UX): bulk-select checkbox,
                    // reusing the exact same `selection` state machine
                    // list.rs's row checkbox drives (`bulk_selected_tasks`/
                    // `bulk_selection_anchor` are shared across lenses, not
                    // per-lens state) — shift toggles range-select the same
                    // way.
                    let mut checked = bulk_selected;
                    let checkbox = ui
                        .add_sized([20.0, 18.0], egui::Checkbox::without_text(&mut checked))
                        .on_hover_text("Select task for bulk actions");
                    if checkbox.changed() {
                        let shift = ui.input(|input| input.modifiers.shift);
                        if shift {
                            selection::select_bulk_task_range(app, &visible_keys, key.clone());
                        } else {
                            selection::set_bulk_task_selected(app, key.clone(), checked);
                        }
                    }
                    let _ = theme::painted_dot(ui, theme::repo_rail_color(&row.project.repo_name));
                    ui.vertical(|ui| {
                        if show_repo {
                            ui.label(
                                egui::RichText::new(&row.project.repo_name)
                                    .small()
                                    .color(theme::muted_text()),
                            );
                        }
                        ui.label(
                            egui::RichText::new(&row.task.id)
                                .monospace()
                                .small()
                                .color(theme::muted_text()),
                        );
                        ui.label(egui::RichText::new(&row.task.title).strong().small());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format::priority_title(&row.task.priority))
                                    .small()
                                    .color(format::priority_color(&row.task.priority)),
                            );
                            if !row.task.acceptance_criteria.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}/{}",
                                        row.task.acceptance_done_count(),
                                        row.task.acceptance_criteria.len()
                                    ))
                                    .small()
                                    .color(theme::muted_text()),
                                );
                            }
                            // task-18 lamp-language marker — same rationale
                            // as the List lens's blocked pill.
                            if !row.task.is_done()
                                && switchbard_core::is_blocked(row.task, &row.project.project)
                            {
                                ui.label(
                                    egui::RichText::new("blocked")
                                        .small()
                                        .strong()
                                        .color(theme::warn_orange()),
                                );
                            }
                            dispatch_ui::render_dispatch_pill(
                                ui,
                                &dispatch_ui::dispatch_state(row.task),
                            );
                        });
                        render_labels_and_age(ui, row.task);
                    });
                });
            })
            .response;
        let interacted = resp
            .interact(egui::Sense::click())
            .on_hover_text("Open in the List lens");
        if interacted.clicked() {
            // TASK-24 (owner-requested UX): a Board card click used to only
            // select the task — invisible, since the Board lens has no
            // detail pane of its own. Jump to the List lens the same way
            // Digest's card click already does (digest.rs), so the click
            // actually opens something. The task's current scope carries
            // over unchanged (no `selected_project` reset like Digest's):
            // this card only rendered because the task was already visible
            // under the current scope, so the List lens will find it there
            // too.
            app.backlog_view.selected_task = Some(key.clone());
            app.backlog_view.editor.loaded_key = None;
            app.backlog_view.lens = BacklogLens::List;
        }
        // TASK-26: right-click bulk actions, reusing list::
        // render_task_context_menu exactly as list.rs's own row does —
        // same UNDRIVABLE-BY-KITTEST status as that menu (see
        // backlog_controls.rs's "List lens: right-click bulk context menu"
        // note); verified by code review, since the reused function is
        // already proven at the List level.
        if interacted.secondary_clicked() {
            selection::focus_context_selection(app, key.clone());
        }
        interacted.context_menu(|ui| {
            list::render_task_context_menu(app, ui, row, all_visible, pending);
        });
    };

    if editable {
        ui.dnd_drag_source(
            egui::Id::new(("backlog_board_strip", &key)),
            key.clone(),
            |ui| {
                paint_strip(ui, app);
            },
        );
    } else {
        paint_strip(ui, app);
    }
}

/// Labels and a humanized age (webview kanban card parity, QA parity matrix
/// row "Kanban card: labels"/"Kanban card: age" — previously a LOW gap).
/// Skips the whole line when there's nothing to show, same convention as
/// `dispatch_ui::render_dispatch_pill`'s `NotFlagged` no-op, so an
/// unlabeled/undated card doesn't paint an empty row.
fn render_labels_and_age(ui: &mut egui::Ui, task: &BacklogTask) {
    let age = card_age(task);
    if task.labels.is_empty() && age.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        if !task.labels.is_empty() {
            ui.label(
                egui::RichText::new(task.labels.join(", "))
                    .small()
                    .color(theme::muted_text()),
            );
        }
        if let Some(age) = age {
            ui.label(egui::RichText::new(age).small().color(theme::muted_text()));
        }
    });
}

/// Prefers `updated_date` (the webview's card age reflects last activity,
/// not creation) and falls back to `created_date` for a task never edited
/// since creation. `None` for a task with neither date parseable — the card
/// just omits the age rather than showing a placeholder.
fn card_age(task: &BacklogTask) -> Option<String> {
    task.updated_date
        .as_deref()
        .or(task.created_date.as_deref())
        .and_then(parse_backlog_datetime_unix)
        .map(humanize_age)
}
