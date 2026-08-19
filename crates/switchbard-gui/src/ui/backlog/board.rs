//! The Board lens: per-status kanban columns, cross-repo, with drag-to-change
//! status writing through the `backlog` CLI (task-15 AC #1).
//!
//! Columns come from `sort::status_options` — the standard `BACKLOG_STATUSES`
//! plus any nonstandard status actually present on a task in scope, so a
//! repo with custom Backlog.md statuses (e.g. an "In Review" column) still
//! gets a home for those tasks rather than silently dropping them.
//!
//! Drag-and-drop uses egui's native `dnd_drag_source` / `dnd_drop_zone`
//! (payload = `BacklogTaskKey`, the same `(project_root, task_id)` pair used
//! for selection elsewhere). Only editable tasks (active source, CLI
//! available) are wrapped as drag sources — draft/completed/archived cards
//! render as plain, non-draggable strips.

use super::{format, scoped_projects, sort, Pending, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::runtime::BacklogTaskKey;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{BacklogTaskPatch, BACKLOG_STATUSES};

/// Column order: the standard statuses in their natural kanban order, then
/// any nonstandard status found in the current scope, alphabetically.
fn column_order(app: &HiveApp, snap: &Snapshot) -> Vec<String> {
    let mut columns: Vec<String> = BACKLOG_STATUSES.iter().map(|s| (*s).to_string()).collect();
    for status in sort::status_options(&scoped_projects(app, snap)) {
        if !columns.iter().any(|c| c.eq_ignore_ascii_case(&status)) {
            columns.push(status);
        }
    }
    columns
}

pub(super) fn render_board(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    tasks: Vec<TaskRow<'_>>,
    pending: &mut Pending,
) {
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

const COLUMN_WIDTH: f32 = 260.0;

fn render_column(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    tasks: &[TaskRow<'_>],
    column_status: &str,
    show_repo: bool,
    pending: &mut Pending,
) {
    let column_tasks: Vec<&TaskRow<'_>> = tasks
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
                        render_strip(app, ui, row, show_repo);
                        ui.add_space(4.0);
                    }
                    if column_tasks.is_empty() {
                        ui.label(egui::RichText::new("No tasks").color(theme::muted_text()));
                    }
                });
        });

        if let Some(dropped_key) = dropped {
            apply_drop(app, tasks, &dropped_key, column_status, pending);
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
fn render_strip(app: &mut HiveApp, ui: &mut egui::Ui, row: &TaskRow<'_>, show_repo: bool) {
    let key = row.key();
    let editable = row.task.editable() && row.project.project.cli_available();
    let selected = app.backlog_view.selected_task.as_ref() == Some(&key);

    let paint_strip = |ui: &mut egui::Ui, app: &mut HiveApp| {
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
            .rounding(3.0)
            .inner_margin(egui::Margin::symmetric(8.0, 6.0));
        let resp = frame
            .show(ui, |ui| {
                ui.set_width(COLUMN_WIDTH - 16.0);
                ui.horizontal(|ui| {
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
                        });
                    });
                });
            })
            .response;
        if resp.interact(egui::Sense::click()).clicked() {
            app.backlog_view.selected_task = Some(key.clone());
            app.backlog_view.editor.loaded_key = None;
        }
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
