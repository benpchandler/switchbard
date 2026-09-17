//! Bulk apply: when the Tasks page has a non-empty selection, a value picked
//! for status, project, ball or a goal link applies to every marked task
//! instead of only the cursor row — the word-processor rule (if the cursor
//! row is not itself marked, the marks still win). Each task goes through
//! the exact core write call its own single-task action already uses, one
//! task at a time; failures are collected rather than aborting the batch on
//! the first error, and the status line always says how many of the total
//! applied, naming any that failed.
use super::App;

/// One task's failure inside a bulk apply.
pub(super) struct BulkFailure {
    pub id: String,
    pub error: String,
}

/// The ids a value picker should apply to: every marked task if the
/// selection is non-empty, else just `primary` (the task the picker opened
/// against) — the word-processor rule: a non-empty selection always wins,
/// even when the cursor itself is not part of it.
pub(super) fn targets(app: &App, primary: &str) -> Vec<String> {
    if app.task_selection.marked.is_empty() {
        vec![primary.to_string()]
    } else {
        // View order, not id order: the order the user sees and swept is the
        // order a partial failure reads back in.
        app.marked_tasks_in_view_order()
    }
}

/// One status line naming how many of `total` succeeded, and why any that
/// failed did; never silently drops a partial failure.
pub(super) fn summarize(verb: &str, total: usize, failures: &[BulkFailure]) -> String {
    let ok = total.saturating_sub(failures.len());
    if failures.is_empty() {
        format!("{verb}: {ok} task{}", if ok == 1 { "" } else { "s" })
    } else {
        let detail = failures
            .iter()
            .map(|failure| format!("{} ({})", failure.id, failure.error))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{verb}: {ok} of {total} tasks; failed: {detail}")
    }
}
