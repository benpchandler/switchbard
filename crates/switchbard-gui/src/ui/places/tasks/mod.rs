//! TASK-97: the Tasks place's real body — the primary work list, at parity
//! with the frozen mock (`~/.lavish/switchbard-ia-places.html` §2, §2b
//! crumbs, §3 expanded header, §4 board, §7c sub-issue tree, §8, §9).
//! Reuses the pre-IA-V2 Backlog view's machinery (`ui::backlog::{sort,
//! list, board, selection, rail}`, widened to `pub(crate)` for this module
//! to reuse) rather than forking it — the binding directive's explicit
//! instruction.
//!
//! ## What changed from the orphaned legacy body
//!
//! The old lens-tab toolbar (Digest/List/Board/Projects/Portfolio/
//! Statistics) is gone from the Tasks body: Digest is its own place
//! (`ui::backlog::digest::render_digest_place`); Statistics/Portfolio are
//! not part of Tasks at all (their code keeps compiling, reachable nowhere
//! — `ui::backlog::render`, the whole legacy body, is `pub fn` and so stays
//! exempt from dead-code lint even though nothing calls it anymore, per the
//! module's own note in `ui::backlog::mod`); Projects is subsumed by the
//! generic group-by below (`Group by: Project` reproduces its exact
//! grouping, cross-repo, defined-but-empty projects included). List/Board
//! survive as *view modes* of the one Tasks place, sharing every facet
//! (group-by is List-only; filters and scope apply to both).
//!
//! ## Module map
//! - `state`   — `TasksPlaceState`/`TasksViewMode`/`FilterPredicate`,
//!   persisted under the "tasks.all" `FilterMemory` (`HiveApp::tasks_place`).
//! - `fields`  — `TaskField`, the one generic field enumeration group-by and
//!   the filter builder both key on, and `field_values`/`distinct_values`.
//! - `groups`  — the group-by engine: buckets visible rows by `TaskField`,
//!   computing each group's roll-up (`Project` additionally joins
//!   `compute_hierarchy_rollup`'s def metadata + a goal-pace chip).
//! - `filters` — the "+ Filter" builder UI and AND-predicate matching.
//! - `header`  — a group header row and its in-place expanded summary band
//!   (mock §3) — the cut project page's replacement.
//! - `list_body` — flattens groups (or the whole scope, ungrouped) into a
//!   uniform-height row list and renders it virtualized
//!   (`egui::ScrollArea::show_rows`, the TASK-13 pattern).

// `fields`/`state` are `pub` (not `pub(crate)`) because `TaskField`/
// `FilterPredicate`/`TasksPlaceState` need to be nameable from
// `tests/*.rs` integration tests, which link against this crate as an
// external dependency — `pub(crate)` wouldn't be visible there, the same
// reason `crate::runtime::{Place, TasksView}` are plain `pub`.
pub mod fields;
mod filters;
mod groups;
mod header;
mod list_body;
pub mod state;

use eframe::egui;

use crate::app::HiveApp;
use crate::ui::backlog::{self, sort, RepoRow, TaskRow};
use crate::ui::filter_bar;
use crate::ui::theme;

use fields::TaskField;
use state::TasksViewMode;

/// The Tasks place's entry point (`Place::Tasks`, `TasksView::All`) —
/// `app.rs`'s only caller.
pub fn render_tasks_place(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    neutralize_legacy_filters(app);
    let snap = backlog::Snapshot::collect(app);
    backlog::reconcile_selected_repo(app, &snap);
    backlog::search::handle_shortcut(app, ctx);
    // Mirrors `tasks_place.view_mode` onto the legacy `backlog_view.lens`
    // field purely as a compatibility shim: `toolbar::render_summary`
    // (reused below for its Refresh/+Task/Clean-Up/bulk-clear buttons) and
    // `toolbar::lens_filters` still gate a couple of sub-widgets on it. This
    // does NOT resurrect the lens-tab chrome — nothing here reads `lens` to
    // choose what to render, only these two reused leaf widgets.
    app.backlog_view.lens = match app.tasks_place.view_mode {
        TasksViewMode::List => crate::runtime::BacklogLens::List,
        TasksViewMode::Board => crate::runtime::BacklogLens::Board,
    };

    // `sort::visible_task_rows` already applies repo scope, the show-
    // completed/archived/drafts toggles, the free-text search, and the
    // active sort key (including TASK-97's new `Rank`) — reused wholesale,
    // then narrowed by the filter builder's own AND predicates on top.
    let sorted = sort::visible_task_rows(app, &snap);
    let filtered: Vec<TaskRow<'_>> = sorted
        .into_iter()
        .filter(|row| filters::matches(row, &app.tasks_place.filters))
        .collect();
    backlog::reconcile_selected_task(app, &filtered);

    let mut pending = backlog::Pending::default();

    if !snap.repos.is_empty() {
        backlog::rail::render_detail_rail(app, ui, &snap, &mut pending);
    }

    let workspace_frame =
        egui::Frame::central_panel(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(12));
    egui::CentralPanel::default()
        .frame(workspace_frame)
        .show(ui, |ui| {
            if snap.repos.is_empty() {
                render_empty(ui);
                return;
            }
            // Reused wholesale: heading + count, "Refresh Backlog", "+
            // Task", "Clean Up Old Tasks" (with its confirm step), and the
            // bulk-clear button — all pre-existing, tested behavior with no
            // Tasks-place-specific shape of their own.
            backlog::toolbar::render_summary(
                app,
                ui,
                &snap,
                &mut pending,
                Some(filtered.len()),
                "Tasks",
            );
            ui.add_space(6.0);

            // Same offer-above-the-controls placement the legacy toolbar
            // used (`ui::backlog::render`'s own comment): information the
            // user can act on or ignore, gating nothing below it.
            let roots: Vec<std::path::PathBuf> =
                snap.repos.iter().map(|row| row.repo.root.clone()).collect();
            backlog::status_migration::detect(app, &roots);
            if app.status_migration_prompt.is_some() {
                ui.add_space(6.0);
                backlog::status_migration::render(app, ui);
            }
            ui.add_space(6.0);

            let scoped = backlog::scoped_repos(app, &snap);
            egui::Frame::default()
                .fill(theme::nav_bg())
                .stroke(theme::surface_stroke())
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    render_facets_row(app, ui, &filtered);
                    ui.separator();
                    render_group_sort_row(app, ui);
                    ui.separator();
                    // TASK-97 medic pass (MAJOR finding): saved-view save/
                    // browse/delete restored here — its only prior caller
                    // was the orphaned legacy `ui::backlog::render` body,
                    // which nothing routes to anymore (see this module's own
                    // doc). `saved_views::apply_saved_view` now translates
                    // into `tasks_place.{filters,group_by,view_mode}`, so
                    // picking a saved view from this bar's combo box (or a
                    // FAVORITES-group click — `HiveApp::navigate_to_favorite`
                    // shares this same function) actually changes what's on
                    // screen here, not just the dead legacy facets.
                    backlog::saved_views::render_saved_views_bar(app, ui);
                });
            ui.separator();

            match app.tasks_place.view_mode {
                TasksViewMode::List => render_list(app, ui, &scoped, &filtered, &mut pending),
                TasksViewMode::Board => {
                    backlog::board::render_board(app, ui, &snap, filtered, &mut pending);
                }
            }
        });

    backlog::search::render_overlay(app, ctx, &snap);
    backlog::create::render_create_modal(app, ctx, &snap, &mut pending);
    backlog::apply_pending(app, ui, pending);
}

/// TASK-97 medic pass (BLOCKER finding): `sort::visible_task_rows` (reused
/// wholesale by this place, above) still narrows on four legacy single-value
/// facets — `backlog_view.{status,priority,project,label}_filter` — that
/// this place's own UI exposes no control for. Every write site that used to
/// set one of them for this place has been moved onto `tasks_place.filters`
/// instead (Digest's "View all", a saved-view apply, a Project favorite's
/// click — the one predicate set this place actually renders a removable
/// chip for), so this is a pure safety net, not the primary fix: force the
/// four back to "all" on every Tasks-place frame, so a value smuggled in
/// some other way (an old persisted filter memory, a future write site added
/// without reading this doc) can never silently narrow the list with no UI
/// to undo it.
fn neutralize_legacy_filters(app: &mut HiveApp) {
    app.backlog_view.status_filter = "all".to_string();
    app.backlog_view.priority_filter = "all".to_string();
    app.backlog_view.project_filter = "all".to_string();
    app.backlog_view.label_filter = "all".to_string();
}

fn render_empty(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading("Tasks");
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "No tracked worktrees have a backlog/config.yml or backlog/tasks directory.",
            )
            .color(theme::muted_text()),
        );
    });
}

/// Row 1: free-text search, the filter builder + recent-filters row, and
/// the List/Board segmented control.
fn render_facets_row(app: &mut HiveApp, ui: &mut egui::Ui, filtered: &[TaskRow<'_>]) {
    ui.horizontal_wrapped(|ui| {
        filter_bar::search(ui, "tasks_place_search", app.filter_mut(), "Search tasks");
        ui.separator();
        // Base visibility toggles (not filter-builder predicates — these
        // gate what `sort::visible_task_rows` ever considers "visible" in
        // the first place, same fields the legacy toolbar's checkboxes
        // read/write).
        ui.checkbox(&mut app.backlog_view.show_completed, "Done");
        ui.checkbox(&mut app.backlog_view.show_archived, "Archived");
        ui.checkbox(&mut app.backlog_view.show_drafts, "Drafts");
        ui.separator();
        filters::render(app, ui, filtered);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (mode, label) in [
                (TasksViewMode::Board, "Board"),
                (TasksViewMode::List, "List"),
            ] {
                if ui
                    .selectable_label(app.tasks_place.view_mode == mode, label)
                    .clicked()
                {
                    app.tasks_place.view_mode = mode;
                }
            }
        });
    });
}

/// Row 2: `Group by:` (List-only — a no-op combo shown disabled in Board
/// mode, since Board keeps its own status columns) and `Sort:`.
fn render_group_sort_row(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_enabled_ui(app.tasks_place.view_mode == TasksViewMode::List, |ui| {
            ui.label(egui::RichText::new("Group by").color(theme::muted_text()));
            let selected_text = app
                .tasks_place
                .group_by
                .map(TaskField::label)
                .unwrap_or("None");
            egui::ComboBox::from_id_salt("tasks_place_group_by")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.tasks_place.group_by, None, "None");
                    for field in TaskField::ALL {
                        ui.selectable_value(
                            &mut app.tasks_place.group_by,
                            Some(field),
                            field.label(),
                        );
                    }
                });
        });
        ui.separator();
        ui.label(egui::RichText::new("Sort").color(theme::muted_text()));
        egui::ComboBox::from_id_salt("tasks_place_sort_key")
            .selected_text(app.backlog_view.sort_key.label())
            .show_ui(ui, |ui| {
                for key in [
                    crate::runtime::BacklogTaskSortKey::Rank,
                    crate::runtime::BacklogTaskSortKey::Triage,
                    crate::runtime::BacklogTaskSortKey::Task,
                    crate::runtime::BacklogTaskSortKey::Status,
                    crate::runtime::BacklogTaskSortKey::Priority,
                    crate::runtime::BacklogTaskSortKey::AcceptanceCriteria,
                    crate::runtime::BacklogTaskSortKey::Labels,
                    crate::runtime::BacklogTaskSortKey::Assignee,
                    crate::runtime::BacklogTaskSortKey::Project,
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
        // Mirrors `list::render_task_sort_controls`'s own "N selected ·
        // Clear" tail (not reused wholesale — that function also renders
        // its own Sort combo, which would duplicate the one above).
        // `bulk_selected_tasks` is shared with Board, same as everywhere
        // else selection state is shared.
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

fn render_list(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    scoped: &[&RepoRow],
    filtered: &[TaskRow<'_>],
    pending: &mut backlog::Pending,
) {
    match app.tasks_place.group_by {
        Some(field) => {
            let groups = groups::build_groups(field, scoped, filtered);
            list_body::render(app, ui, &groups, true, pending);
        }
        None => {
            let ungrouped = groups::build_groups_ungrouped(filtered);
            list_body::render(app, ui, &ungrouped, false, pending);
        }
    }
}
