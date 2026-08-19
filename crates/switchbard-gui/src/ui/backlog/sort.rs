//! Task filtering, sorting, and the triage-ranking pipeline.
//!
//! `visible_task_rows` is the single place that turns "every tracked
//! project's tasks" into "the rows this frame renders": it applies the
//! visibility filters (status/priority/search/show-completed/show-archived),
//! then orders them either via the manual sort keys (`compare_tasks`, ported
//! unchanged from the pre-split view) or, for the default `Triage` key, via
//! `switchbard_core::triage_rank` — the pure cross-repo ranking function.

use super::{scoped_projects, ProjectRow, Snapshot, TaskRow};
use crate::app::HiveApp;
use crate::runtime::{BacklogTaskKey, BacklogTaskSortDirection, BacklogTaskSortKey};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use switchbard_core::{
    triage_entry_from_task, triage_rank, BacklogProject, BacklogTask, BacklogTaskSource,
    BACKLOG_PRIORITIES, BACKLOG_STATUSES,
};

/// Filter + order every visible task across the current scope. The single
/// entry point `list::render_task_list`, `toolbar`'s visible-count label, and
/// `mod::ensure_selection` all share, so "what's currently on screen" only
/// has one definition.
pub(super) fn visible_task_rows<'a>(app: &HiveApp, snap: &'a Snapshot) -> Vec<TaskRow<'a>> {
    let filter_lc = app.filter.to_lowercase();
    let mut rows: Vec<TaskRow<'a>> = Vec::new();
    for project in scoped_projects(app, snap) {
        for task in &project.project.tasks {
            if task_visible(task, app, &filter_lc) {
                rows.push(TaskRow { project, task });
            }
        }
    }

    match app.backlog_view.sort_key {
        BacklogTaskSortKey::Triage => sort_by_triage(app, &mut rows),
        sort_key => {
            let direction = app.backlog_view.sort_direction;
            rows.sort_by(|a, b| compare_tasks(a.task, b.task, sort_key, direction));
        }
    }
    rows
}

/// Rank via the pure core function, then re-attach each ranked entry back to
/// its `TaskRow` by `(project_key, task_id)`. `triage_rank` only sees plain
/// data (no lifetimes), which is what keeps it a pure, easily-tested core fn.
fn sort_by_triage<'a>(app: &HiveApp, rows: &mut Vec<TaskRow<'a>>) {
    let overlay = app.ordering_snapshot().overlay;
    let entries: Vec<_> = rows
        .iter()
        .map(|row| {
            triage_entry_from_task(row.project.key.clone(), &row.project.repo_name, row.task)
        })
        .collect();
    let ranked = triage_rank(&entries, &overlay);

    let mut by_key: HashMap<BacklogTaskKey, TaskRow<'a>> = std::mem::take(rows)
        .into_iter()
        .map(|row| (row.key(), row))
        .collect();
    let mut ordered: Vec<TaskRow<'a>> = ranked
        .into_iter()
        .filter_map(|entry| by_key.remove(&(entry.project_key, entry.task_id)))
        .collect();
    if app.backlog_view.sort_direction == BacklogTaskSortDirection::Descending {
        ordered.reverse();
    }
    *rows = ordered;
}

pub(super) fn compare_tasks(
    a: &BacklogTask,
    b: &BacklogTask,
    sort_key: BacklogTaskSortKey,
    sort_direction: BacklogTaskSortDirection,
) -> Ordering {
    let primary = match sort_key {
        // Triage sorts via `sort_by_triage` above; this comparator is never
        // invoked with this key. Kept in the match (rather than a wildcard)
        // so a future sort key can't silently fall through unhandled.
        BacklogTaskSortKey::Triage => Ordering::Equal,
        BacklogTaskSortKey::Task => cmp_ascii_case_insensitive(&a.id, &b.id)
            .then_with(|| cmp_ascii_case_insensitive(&a.title, &b.title)),
        BacklogTaskSortKey::Status => status_rank(&a.status)
            .cmp(&status_rank(&b.status))
            .then_with(|| cmp_ascii_case_insensitive(&a.status, &b.status)),
        BacklogTaskSortKey::Priority => priority_rank(&a.priority)
            .cmp(&priority_rank(&b.priority))
            .then_with(|| cmp_ascii_case_insensitive(&a.priority, &b.priority)),
        BacklogTaskSortKey::AcceptanceCriteria => acceptance_progress(a)
            .cmp(&acceptance_progress(b))
            .then_with(|| {
                a.acceptance_criteria
                    .len()
                    .cmp(&b.acceptance_criteria.len())
            }),
    };
    let primary = match sort_direction {
        BacklogTaskSortDirection::Ascending => primary,
        BacklogTaskSortDirection::Descending => primary.reverse(),
    };
    primary
        .then_with(|| cmp_ascii_case_insensitive(&a.id, &b.id))
        .then_with(|| cmp_ascii_case_insensitive(&a.title, &b.title))
}

fn status_rank(status: &str) -> usize {
    BACKLOG_STATUSES
        .iter()
        .position(|option| option.eq_ignore_ascii_case(status))
        .unwrap_or(BACKLOG_STATUSES.len())
}

fn priority_rank(priority: &str) -> usize {
    BACKLOG_PRIORITIES
        .iter()
        .position(|option| option.eq_ignore_ascii_case(priority))
        .unwrap_or(BACKLOG_PRIORITIES.len())
}

fn acceptance_progress(task: &BacklogTask) -> usize {
    let total = task.acceptance_criteria.len();
    if total == 0 {
        return 0;
    }
    task.acceptance_done_count() * 1_000 / total
}

pub(super) fn cmp_ascii_case_insensitive(a: &str, b: &str) -> Ordering {
    let mut a = a.bytes();
    let mut b = b.bytes();
    loop {
        match (a.next(), b.next()) {
            (Some(left), Some(right)) => {
                let ordering = left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase());
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

pub(super) fn task_visible(task: &BacklogTask, app: &HiveApp, filter_lc: &str) -> bool {
    if task_is_completed(task) && !app.backlog_view.show_completed {
        return false;
    }
    if task.source == BacklogTaskSource::Archived && !app.backlog_view.show_archived {
        return false;
    }
    if app.backlog_view.status_filter != "all"
        && !task
            .status
            .eq_ignore_ascii_case(&app.backlog_view.status_filter)
    {
        return false;
    }
    if app.backlog_view.priority_filter != "all"
        && !task
            .priority
            .eq_ignore_ascii_case(&app.backlog_view.priority_filter)
    {
        return false;
    }
    if filter_lc.is_empty() {
        return true;
    }
    let haystack = [
        task.id.as_str(),
        task.title.as_str(),
        task.status.as_str(),
        task.priority.as_str(),
        task.description.as_str(),
        &task.labels.join(" "),
        &task.assignees.join(" "),
    ]
    .join(" ")
    .to_lowercase();
    haystack.contains(filter_lc)
}

pub(super) fn open_task_count(project: &BacklogProject) -> usize {
    project
        .tasks
        .iter()
        .filter(|task| !task_is_completed(task) && task.source != BacklogTaskSource::Archived)
        .count()
}

pub(super) fn task_is_completed(task: &BacklogTask) -> bool {
    task.source == BacklogTaskSource::Completed || task.status.eq_ignore_ascii_case("done")
}

/// The set of status values worth offering in the filter combo box: the
/// standard `BACKLOG_STATUSES` plus any nonstandard value actually present
/// on a task in the current scope (so a hand-edited task's odd status is
/// still filterable).
pub(super) fn status_options(scoped: &[&ProjectRow]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for status in BACKLOG_STATUSES {
        set.insert((*status).to_string());
    }
    for project in scoped {
        for task in &project.project.tasks {
            set.insert(task.status.clone());
        }
    }
    set.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn task_with_status(status: &str, source: BacklogTaskSource) -> BacklogTask {
        let mut task = task_with_fields(
            &format!("TASK-{}", status.replace(' ', "-")),
            status,
            status,
            "medium",
            0,
            0,
        );
        task.source = source;
        task
    }

    fn task_with_fields(
        id: &str,
        title: &str,
        status: &str,
        priority: &str,
        checked_criteria: usize,
        total_criteria: usize,
    ) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            milestone: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: (0..total_criteria)
                .map(|index| switchbard_core::BacklogChecklistItem {
                    index: index + 1,
                    checked: index < checked_criteria,
                    text: format!("Criterion {}", index + 1),
                })
                .collect(),
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/tmp/project/backlog/tasks/task.md"),
        }
    }

    #[test]
    fn done_status_counts_as_completed_even_before_cleanup_moves_file() {
        let task = task_with_status("Done", BacklogTaskSource::Active);

        assert!(task_is_completed(&task));
    }

    #[test]
    fn open_task_count_excludes_done_and_archived_tasks() {
        let project = BacklogProject {
            root: PathBuf::from("/tmp/project"),
            cli_path: None,
            tasks: vec![
                task_with_status("To Do", BacklogTaskSource::Active),
                task_with_status("In Progress", BacklogTaskSource::Active),
                task_with_status("Done", BacklogTaskSource::Active),
                task_with_status("To Do", BacklogTaskSource::Archived),
            ],
            warnings: vec![],
            loaded_at_unix: 0,
        };

        assert_eq!(open_task_count(&project), 2);
    }

    #[test]
    fn priority_sort_uses_backlog_priority_order_in_both_directions() {
        let high = task_with_fields("TASK-1", "High priority", "To Do", "high", 0, 0);
        let medium = task_with_fields("TASK-2", "Medium priority", "To Do", "medium", 0, 0);
        let low = task_with_fields("TASK-3", "Low priority", "To Do", "low", 0, 0);
        let mut tasks = [&low, &high, &medium];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Priority,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-1", "TASK-2", "TASK-3"]
        );

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::Priority,
                BacklogTaskSortDirection::Descending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-3", "TASK-2", "TASK-1"]
        );
    }

    #[test]
    fn acceptance_sort_orders_by_completion_progress() {
        let empty = task_with_fields("TASK-1", "No AC", "To Do", "medium", 0, 0);
        let partial = task_with_fields("TASK-2", "Partial AC", "To Do", "medium", 1, 3);
        let complete = task_with_fields("TASK-3", "Complete AC", "To Do", "medium", 2, 2);
        let mut tasks = [&complete, &empty, &partial];

        tasks.sort_by(|a, b| {
            compare_tasks(
                a,
                b,
                BacklogTaskSortKey::AcceptanceCriteria,
                BacklogTaskSortDirection::Ascending,
            )
        });
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TASK-1", "TASK-2", "TASK-3"]
        );
    }
}
