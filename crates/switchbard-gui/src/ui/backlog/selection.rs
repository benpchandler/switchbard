//! Bulk multi-select state machine: shift-click range select, ctrl/cmd-click
//! toggle, right-click context-menu targeting, and keeping the selection
//! consistent as the visible task list changes under it.
//!
//! Keyed on `BacklogTaskKey` (`(project_root, task_id)`) rather than a bare
//! task id — the unified All-projects scope can show two same-numbered tasks
//! from different repos, and a bare-id `BTreeSet` would silently conflate
//! them.

use super::TaskRow;
use crate::app::HiveApp;
use crate::runtime::BacklogTaskKey;

/// The keys a context-menu action should apply to: every currently
/// bulk-selected key if the clicked row is part of that selection, otherwise
/// just the clicked row (a right-click outside the current selection acts on
/// that one row, matching common file-manager conventions).
pub(super) fn selected_keys_for_menu(
    app: &HiveApp,
    visible: &[TaskRow<'_>],
    clicked: &TaskRow<'_>,
) -> Vec<BacklogTaskKey> {
    let clicked_key = clicked.key();
    if app.backlog_view.bulk_selected_tasks.contains(&clicked_key) {
        visible
            .iter()
            .map(TaskRow::key)
            .filter(|key| app.backlog_view.bulk_selected_tasks.contains(key))
            .collect()
    } else {
        vec![clicked_key]
    }
}

/// Narrow `selected` to the keys whose task is still `editable()` — the
/// Backlog CLI only edits active (non-completed/archived/draft) tasks.
pub(super) fn editable_keys(
    visible: &[TaskRow<'_>],
    selected: &[BacklogTaskKey],
) -> Vec<BacklogTaskKey> {
    selected
        .iter()
        .filter(|key| {
            visible
                .iter()
                .any(|row| &row.key() == *key && row.task.editable())
        })
        .cloned()
        .collect()
}

/// Drop any bulk-selected key (and the range anchor) that's no longer among
/// the currently visible rows — called once per frame before rendering so a
/// filter change can't leave a "ghost" selected task the user can't see.
pub(super) fn retain_visible_bulk_selection(app: &mut HiveApp, visible: &[TaskRow<'_>]) {
    app.backlog_view
        .bulk_selected_tasks
        .retain(|key| visible.iter().any(|row| row.key() == *key));
    if app
        .backlog_view
        .bulk_selection_anchor
        .as_ref()
        .is_some_and(|anchor| !visible.iter().any(|row| row.key() == *anchor))
    {
        app.backlog_view.bulk_selection_anchor = None;
    }
}

pub(super) fn set_bulk_task_selected(app: &mut HiveApp, key: BacklogTaskKey, selected: bool) {
    if selected {
        app.backlog_view.bulk_selected_tasks.insert(key.clone());
        app.backlog_view.bulk_selection_anchor = Some(key);
    } else {
        app.backlog_view.bulk_selected_tasks.remove(&key);
        if app.backlog_view.bulk_selection_anchor.as_ref() == Some(&key) {
            app.backlog_view.bulk_selection_anchor =
                app.backlog_view.bulk_selected_tasks.iter().next().cloned();
        }
    }
}

pub(super) fn select_bulk_task_range(
    app: &mut HiveApp,
    visible_keys: &[BacklogTaskKey],
    key: BacklogTaskKey,
) {
    let range = bulk_range_keys(
        visible_keys,
        app.backlog_view.bulk_selection_anchor.as_ref(),
        &key,
    );
    if range.is_empty() {
        set_bulk_task_selected(app, key, true);
        return;
    }
    for range_key in range {
        app.backlog_view.bulk_selected_tasks.insert(range_key);
    }
    app.backlog_view.bulk_selection_anchor = Some(key);
}

fn bulk_range_keys(
    visible_keys: &[BacklogTaskKey],
    anchor: Option<&BacklogTaskKey>,
    clicked: &BacklogTaskKey,
) -> Vec<BacklogTaskKey> {
    let Some(clicked_index) = visible_keys.iter().position(|key| key == clicked) else {
        return Vec::new();
    };
    let anchor_index = anchor
        .and_then(|anchor| visible_keys.iter().position(|key| key == anchor))
        .unwrap_or(clicked_index);
    let (start, end) = if anchor_index <= clicked_index {
        (anchor_index, clicked_index)
    } else {
        (clicked_index, anchor_index)
    };
    visible_keys[start..=end].to_vec()
}

pub(super) fn toggle_bulk_task_selection(app: &mut HiveApp, key: BacklogTaskKey) {
    if !app.backlog_view.bulk_selected_tasks.remove(&key) {
        app.backlog_view.bulk_selected_tasks.insert(key.clone());
    }
    app.backlog_view.bulk_selection_anchor = Some(key);
}

/// Right-click on a row that isn't already part of the bulk selection
/// collapses the selection to just that row before opening the context menu,
/// so the menu's "N selected" count always matches what it's about to act on.
pub(super) fn focus_context_selection(app: &mut HiveApp, key: BacklogTaskKey) {
    if !app.backlog_view.bulk_selected_tasks.contains(&key) {
        app.backlog_view.bulk_selected_tasks.clear();
        app.backlog_view.bulk_selected_tasks.insert(key.clone());
    }
    app.backlog_view.bulk_selection_anchor = Some(key.clone());
    app.backlog_view.selected_task = Some(key);
    app.backlog_view.editor.loaded_key = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn key(id: &str) -> BacklogTaskKey {
        (PathBuf::from("/repos/a"), id.to_string())
    }

    fn keys<const N: usize>(ids: [&str; N]) -> Vec<BacklogTaskKey> {
        ids.into_iter().map(key).collect()
    }

    #[test]
    fn bulk_range_keys_selects_contiguous_visible_range() {
        let visible = keys(["TASK-1", "TASK-2", "TASK-3", "TASK-4"]);

        assert_eq!(
            bulk_range_keys(&visible, Some(&key("TASK-1")), &key("TASK-3")),
            keys(["TASK-1", "TASK-2", "TASK-3"])
        );
        assert_eq!(
            bulk_range_keys(&visible, Some(&key("TASK-4")), &key("TASK-2")),
            keys(["TASK-2", "TASK-3", "TASK-4"])
        );
    }

    #[test]
    fn bulk_range_keys_falls_back_to_clicked_task_without_visible_anchor() {
        let visible = keys(["TASK-1", "TASK-2", "TASK-3"]);

        assert_eq!(
            bulk_range_keys(&visible, Some(&key("TASK-99")), &key("TASK-2")),
            keys(["TASK-2"])
        );
        assert!(bulk_range_keys(&visible, Some(&key("TASK-1")), &key("TASK-99")).is_empty());
    }
}
