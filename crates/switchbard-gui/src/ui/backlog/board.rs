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
///
/// TASK-29 (owner-reported live regression, 2026-08-05): card clicks and
/// the bulk-select checkbox never registered in the real app — this was a
/// genuine defect, not a kittest harness limitation as TASK-24/26 first
/// concluded (owner confirmed dead clicks against the live 0.31 build).
/// Root cause, confirmed by reading egui 0.31.1's own source
/// (`hit_test.rs::hit_test_on_close`): a retroactive `resp.interact(..)`
/// call — or `Ui::dnd_drag_source`'s own internal `Sense::drag()`-only
/// wrapper widget — registered *after* a card's content already painted
/// its own smaller widgets (the checkbox), and covering (containing)
/// their rect, always wins egui's hit-test tie-break for anything inside
/// it (`should_prioritize_hits_on_back` explicitly declines to protect a
/// widget that's "fully occluded" by a larger one registered on top). And
/// when that winning widget senses *only* drag — exactly what
/// `dnd_drag_source` auto-generates — egui explicitly discards any click
/// underneath it: "ignore the click-widget, because it would be confusing
/// if clicking a drag-widget would actually click something else below
/// it" (hit_test.rs's own comment). That's what silently ate every card
/// click *and* the checkbox's own click sense, on editable (drag-wrapped)
/// cards; non-editable cards had the milder version of the same bug (the
/// checkbox lost ties to the card-wide click interact for the same
/// "larger widget registered after, containing a smaller one" reason,
/// just without the drag-only widget's harsher "discard the click
/// entirely" rule on top).
///
/// Fix, two structural changes:
/// 1. The bulk-select checkbox is a **sibling** of the "click to open,
///    drag to move" region (`content_rect`, captured from its own
///    `ui.scope`), not nested inside it — their rects never overlap, so
///    there is no tie for egui to resolve either way.
/// 2. "Click to open" and "drag to reorder" are **one** widget
///    (`Sense::click_and_drag()`), not two competing ones. A single widget
///    sensing both lets egui's own press/release-without-movement vs.
///    movement-past-threshold logic (`PointerState::is_decidedly_dragging`)
///    disambiguate which happened, instead of a drag-only layer shadowing
///    everything below it. This rules out `Ui::dnd_drag_source` — it
///    always creates that second, drag-only widget — so the drag payload
///    is set manually via `Response::dnd_set_drag_payload`, and the
///    floating "ghost" card shown mid-drag is reimplemented by hand in
///    `render_drag_ghost`, mirroring `dnd_drag_source`'s own dragging
///    branch line-for-line (still safe to mirror — only the *non*-dragging
///    branch, which layers the extra drag-only widget, is the culprit).
///
/// Trade-off: a drag can only be started by pressing on the card body, not
/// directly on the checkbox — acceptable, since a checkbox that also
/// drag-initiates would be surprising UX regardless.
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
    let card_id = egui::Id::new(("backlog_board_strip", &key));

    if editable && ui.ctx().is_being_dragged(card_id) {
        render_drag_ghost(ui, card_id, row, show_repo, selected, bulk_selected);
        return;
    }

    let (checkbox_resp, checked_now, content_rect) =
        paint_card(ui, row, show_repo, selected, bulk_selected);
    if checkbox_resp.changed() {
        // TASK-26 (owner-requested UX): bulk-select checkbox, reusing the
        // exact same `selection` state machine list.rs's row checkbox
        // drives (`bulk_selected_tasks`/`bulk_selection_anchor` are shared
        // across lenses, not per-lens state) — shift toggles range-select
        // the same way.
        let shift = ui.input(|input| input.modifiers.shift);
        if shift {
            selection::select_bulk_task_range(app, &visible_keys, key.clone());
        } else {
            selection::set_bulk_task_selected(app, key.clone(), checked_now);
        }
    }

    let sense = if editable {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::click()
    };
    let mut interacted = ui
        .interact(content_rect, card_id, sense)
        .on_hover_text("Open in the List lens");
    if editable {
        interacted = interacted.on_hover_cursor(egui::CursorIcon::Grab);
        interacted.dnd_set_drag_payload(key.clone());
    }
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
    // render_task_context_menu exactly as list.rs's own row does — same
    // pattern as the List lens's own right-click menu.
    if interacted.secondary_clicked() {
        selection::focus_context_selection(app, key.clone());
    }
    interacted.context_menu(|ui| {
        list::render_task_context_menu(app, ui, row, all_visible, pending);
    });
}

/// Paints one card's frame, checkbox, and content — pure function of its
/// input, no `HiveApp` access, so it can be reused unchanged for both the
/// normal in-place render and the mid-drag floating ghost
/// (`render_drag_ghost`). Returns `(checkbox_response,
/// checkbox_checked_after_paint, content_rect)`: `content_rect` is
/// deliberately the "dot + vertical" sub-area only, excluding the
/// checkbox, so the caller's click/drag interact call never overlaps the
/// checkbox's own (see `render_strip`'s doc for why that matters).
fn paint_card(
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    show_repo: bool,
    selected: bool,
    bulk_selected: bool,
) -> (egui::Response, bool, egui::Rect) {
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
    let mut checked = bulk_selected;
    let mut content_rect = egui::Rect::NOTHING;
    let checkbox_resp = frame
        .show(ui, |ui| {
            ui.set_width(COLUMN_WIDTH - 16.0);
            ui.horizontal(|ui| {
                let checkbox = ui
                    .add_sized([20.0, 18.0], egui::Checkbox::without_text(&mut checked))
                    .on_hover_text("Select task for bulk actions");
                let content_resp = ui
                    .scope(|ui| {
                        let _ =
                            theme::painted_dot(ui, theme::repo_rail_color(&row.project.repo_name));
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
                                // task-18 lamp-language marker — same
                                // rationale as the List lens's blocked pill.
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
                    })
                    .response;
                content_rect = content_resp.rect;
                checkbox
            })
            .inner
        })
        .inner;
    (checkbox_resp, checked, content_rect)
}

/// The floating "ghost" shown while a card is mid-drag — repaints the same
/// visual content onto an `Order::Tooltip` layer near the pointer,
/// mirroring `Ui::dnd_drag_source`'s own dragging branch (not reused
/// directly — see `render_strip`'s doc for why `dnd_drag_source` itself
/// can't be used for the non-dragging path). `Order::Tooltip` responses
/// are always inert ("anything with `Order::Tooltip` always gets an empty
/// Response" — egui's own doc comment), so painting the checkbox here is
/// purely cosmetic; it can't receive input mid-drag, which is fine since
/// nothing needs to change on the ghost itself.
fn render_drag_ghost(
    ui: &mut egui::Ui,
    card_id: egui::Id,
    row: &TaskRow<'_>,
    show_repo: bool,
    selected: bool,
    bulk_selected: bool,
) {
    egui::DragAndDrop::set_payload(ui.ctx(), row.key());
    let layer_id = egui::LayerId::new(egui::Order::Tooltip, card_id);
    let response = ui
        .scope_builder(egui::UiBuilder::new().layer_id(layer_id), |ui| {
            paint_card(ui, row, show_repo, selected, bulk_selected);
        })
        .response;
    if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
        let delta = pointer_pos - response.rect.center();
        ui.ctx()
            .transform_layer_shapes(layer_id, egui::emath::TSTransform::from_translation(delta));
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
