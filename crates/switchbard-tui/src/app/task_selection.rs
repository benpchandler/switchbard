//! `space`/`shift-down`/`shift-up` bulk selection on the Tasks page: the same
//! shared `Selection` shape Pull Requests uses (`crate::selection`), applied
//! over task rows rather than PR rows. A heading a sweep crosses marks the
//! tasks around it, never the heading itself — `row_of_task` and the range
//! below only ever look at `Row::Task`.
use std::collections::BTreeSet;

use crate::group::Row;

use super::App;

impl App {
    /// Marked task ids in the order the list shows them: the bulk-apply order.
    pub fn marked_tasks_in_view_order(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter_map(|row| match row {
                Row::Task(index) => Some(&self.tasks[*index]),
                Row::Heading { .. } => None,
            })
            .filter(|task| self.task_selection.is_marked(&task.id))
            .map(|task| task.id.clone())
            .collect()
    }

    /// The row position of the task carrying `id`, or `None` if it is
    /// filtered out of the current projection.
    fn row_of_task(&self, id: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::Task(index) if self.tasks[*index].id == id))
    }

    /// `space`: toggle the cursor row's mark.
    pub(super) fn toggle_task_mark(&mut self) {
        let Some(task) = self.selected_task() else {
            self.status = "no task selected".to_string();
            return;
        };
        let id = task.id.clone();
        let marked = self.task_selection.toggle(&id);
        let count = self.task_selection.marked.len();
        self.status = if marked {
            format!("Marked ({count}); the next value picked applies to all of them, Esc clears")
        } else {
            format!("Unmarked ({count} marked)")
        };
        if marked {
            self.step(1);
        }
    }

    /// Word-processor-style range select: the first call anchors at the
    /// cursor row; each further call moves the cursor by `delta` (`1` or
    /// `-1`) and marks every task between the anchor and the new cursor,
    /// inclusive, unmarking any the range has left behind (`Selection`
    /// tracks which marks are the sweep's own to take back).
    pub(super) fn extend_task_mark(&mut self, delta: isize) {
        let anchor_id = match self.task_selection.sweep_anchor() {
            Some(anchor) => anchor.to_string(),
            None => match self.selected_task() {
                Some(task) => task.id.clone(),
                None => return,
            },
        };
        self.step(delta);
        let Some(anchor_row) = self.row_of_task(&anchor_id) else {
            // The anchor row is no longer visible (filtered out from under
            // the sweep); nothing sane left to sweep against.
            self.task_selection.reset_sweep();
            return;
        };
        let lo = anchor_row.min(self.selected);
        let hi = anchor_row.max(self.selected);
        let in_range: BTreeSet<String> = self.rows[lo..=hi]
            .iter()
            .filter_map(|row| match row {
                Row::Task(index) => Some(self.tasks[*index].id.clone()),
                Row::Heading { .. } => None,
            })
            .collect();
        let count = self.task_selection.extend(anchor_id, in_range);
        self.status =
            format!("Marked ({count}); the next value picked applies to all of them, Esc clears");
    }
}
