//! The List lens's sub-task tree (task-17): which rows nest under a parent,
//! and the recursive walk that renders a parent then its (expanded)
//! children. Split out of `list.rs` to keep both files well clear of the
//! repo's ~600 LOC ceiling for `ui/**` modules.
//!
//! A row whose `parent` names another row *also present in the current
//! filtered/sorted list* renders nested under that parent instead of at the
//! top level ([`child_keys_in_view`]); a parent's expand/collapse state lives
//! in `app.backlog_view.expanded_parents`. Nested children come from
//! `switchbard_core::children` against the *full* repo, not the filtered
//! view — expanding a parent should reveal its whole sub-tree, not silently
//! hide children the status/priority filters happen to exclude. A child
//! whose parent isn't in the current view (filtered out, or cross-repo —
//! impossible for a real parent/child pair, but Rule 5 says don't assume)
//! renders at the top level instead of vanishing.

use super::{list, Pending, TaskRow};
use crate::app::HiveApp;
use crate::runtime::BacklogTaskKey;
use eframe::egui;
use std::collections::HashSet;
use switchbard_core::children;

/// Keys of every row in `tasks` whose `parent` names another row also
/// present in `tasks` — these render nested, not at the top level.
pub(super) fn child_keys_in_view(tasks: &[TaskRow<'_>]) -> HashSet<BacklogTaskKey> {
    tasks
        .iter()
        .filter(|row| {
            row.task.parent.as_ref().is_some_and(|parent_id| {
                tasks
                    .iter()
                    .any(|other| other.repo.key == row.repo.key && &other.task.id == parent_id)
            })
        })
        .map(TaskRow::key)
        .collect()
}

/// Render `row`, then — if it has children and is expanded — recurse into
/// them, indented one level deeper. `children` always resolves against the
/// full repo (see the module doc), so a parent's roll-up badge and
/// expanded subtree are consistent regardless of the active filters.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_task_tree_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &TaskRow<'_>,
    all_visible: &[TaskRow<'_>],
    visible_keys: &[BacklogTaskKey],
    show_repo: bool,
    depth: usize,
    pending: &mut Pending,
) {
    let kids = children(row.task, &row.repo.repo);
    list::render_task_list_row(
        app,
        ui,
        row,
        all_visible,
        visible_keys,
        show_repo,
        depth,
        &kids,
        false, // legacy List lens keeps its collapse-by-default behavior
        true,  // legacy List lens always shows the Delivery/AC column
        pending,
    );
    ui.add_space(5.0);
    if !kids.is_empty() && app.backlog_view.expanded_parents.contains(&row.key()) {
        for child_task in &kids {
            let child_row = TaskRow {
                repo: row.repo,
                task: child_task,
            };
            render_task_tree_row(
                app,
                ui,
                &child_row,
                all_visible,
                visible_keys,
                show_repo,
                depth + 1,
                pending,
            );
        }
    }
}
