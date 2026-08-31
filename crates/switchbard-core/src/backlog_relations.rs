//! Task relationship graphs within one Backlog project: dependency/blocked
//! state (task-18) and parent/child sub-task hierarchy (task-17).
//!
//! Both are pure, no-IO functions over an already-loaded `BacklogRepo` —
//! no new store, matching the same constraint `backlog_stats` and
//! `backlog_triage` already follow. Dependencies and sub-tasks are scoped to
//! a single project: Backlog.md's own `dependencies`/`parent` fields store
//! bare task ids with no repo qualifier, so "TASK-3 depends on TASK-1" only
//! ever means "TASK-1 in this same project" — there is no native cross-repo
//! dependency concept to resolve.

use crate::backlog::{parse_backlog_day, BacklogRepo, BacklogTask};

/// `true` when any of `task`'s `dependencies` names a task in `project` that
/// isn't done. A dependency id that doesn't resolve to any task in the
/// project (typo, or a task since archived/removed) can't be verified, so it
/// doesn't block — matching Rule 5's "don't over-block on ambiguous data".
pub fn is_blocked(task: &BacklogTask, project: &BacklogRepo) -> bool {
    !blocking_dependencies(task, project).is_empty()
}

/// The specific dependencies of `task` that are still open, in the order
/// they're listed. Empty when `task` isn't blocked.
pub fn blocking_dependencies<'a>(
    task: &BacklogTask,
    project: &'a BacklogRepo,
) -> Vec<&'a BacklogTask> {
    task.dependencies
        .iter()
        .filter_map(|dep_id| find_task(project, dep_id))
        .filter(|dep| !dep.is_done())
        .collect()
}

/// Every dependency of `task`, resolved to its task and done/not-done state,
/// for the detail pane's "Depends on" list (task-18: shows status per dep,
/// not just the id). Unresolvable ids are omitted — there's nothing to show
/// a status for.
pub fn dependency_statuses<'a>(
    task: &BacklogTask,
    project: &'a BacklogRepo,
) -> Vec<(&'a BacklogTask, bool)> {
    task.dependencies
        .iter()
        .filter_map(|dep_id| find_task(project, dep_id))
        .map(|dep| (dep, dep.is_done()))
        .collect()
}

/// The reverse edge: every task in `project` that names `task.id` as one of
/// its own dependencies — i.e. what `task` blocks (task-18's "blocks"
/// direction). Purely derived; Backlog.md has no stored inverse field.
pub fn blocks<'a>(task: &BacklogTask, project: &'a BacklogRepo) -> Vec<&'a BacklogTask> {
    project
        .tasks
        .iter()
        .filter(|other| other.dependencies.iter().any(|dep_id| dep_id == &task.id))
        .collect()
}

/// How recently a dependency must have been completed for [`is_newly_
/// unblocked`] to flag `task` — see that function's doc for why this is a
/// proxy rather than a tracked transition.
const NEWLY_UNBLOCKED_WINDOW_DAYS: i64 = 3;

/// `true` when `task` is currently unblocked (see [`is_blocked`]) *and* at
/// least one of its dependencies was completed within the last
/// [`NEWLY_UNBLOCKED_WINDOW_DAYS`] days (that dependency's status is done and
/// its `updated_date` is recent) — task-21's digest "newly unblocked"
/// section.
///
/// Switchbard has no persisted history of blocked-state transitions, and
/// adding one would be a new store (against task-16/20's constraint), so
/// "newly" is approximated from data already on the tasks themselves: a
/// dependency that just flipped to Done is the observable signal that
/// `task` just became actionable. A task with no dependencies at all is
/// never "newly" anything — there was nothing to unblock.
pub fn is_newly_unblocked(task: &BacklogTask, project: &BacklogRepo, today_unix_day: i64) -> bool {
    if task.dependencies.is_empty() || is_blocked(task, project) {
        return false;
    }
    task.dependencies.iter().any(|dep_id| {
        find_task(project, dep_id).is_some_and(|dep| {
            dep.is_done()
                && dep
                    .updated_date
                    .as_deref()
                    .and_then(parse_backlog_day)
                    .is_some_and(|day| today_unix_day - day <= NEWLY_UNBLOCKED_WINDOW_DAYS)
        })
    })
}

/// Direct sub-tasks of `task` (task-17): every task in `project` whose
/// `parent` names `task.id`. Not recursive — a roll-up over grandchildren
/// would double-count if a grandchild's parent (the child) is also counted,
/// so "children done/total" per the task's own description means direct
/// children only.
pub fn children<'a>(task: &BacklogTask, project: &'a BacklogRepo) -> Vec<&'a BacklogTask> {
    project
        .tasks
        .iter()
        .filter(|other| other.parent.as_deref() == Some(task.id.as_str()))
        .collect()
}

/// `(done, total)` roll-up over `task`'s direct children, for the tree
/// view's parent-row progress badge. `(0, 0)` for a childless task — callers
/// checking `total == 0` know to skip the badge rather than render "0/0".
pub fn subtask_progress(task: &BacklogTask, project: &BacklogRepo) -> (usize, usize) {
    let kids = children(task, project);
    let done = kids.iter().filter(|kid| kid.is_done()).count();
    (done, kids.len())
}

fn find_task<'a>(project: &'a BacklogRepo, id: &str) -> Option<&'a BacklogTask> {
    project.tasks.iter().find(|task| task.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::{BacklogChecklistItem, BacklogTaskSource};
    use std::path::PathBuf;

    fn task(id: &str, status: &str, deps: &[&str], parent: Option<&str>) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: id.to_string(),
            status: status.to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            references: vec![],
            project: None,
            parent: parent.map(str::to_string),
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: Vec::<BacklogChecklistItem>::new(),
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/repo/backlog/tasks/task.md"),
        }
    }

    fn project(tasks: Vec<BacklogTask>) -> BacklogRepo {
        BacklogRepo {
            root: PathBuf::from("/repo"),
            tasks,
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        }
    }

    #[test]
    fn task_with_an_open_dependency_is_blocked() {
        let project = project(vec![
            task("TASK-1", "To Do", &[], None),
            task("TASK-2", "To Do", &["TASK-1"], None),
        ]);
        let dependent = &project.tasks[1];

        assert!(is_blocked(dependent, &project));
        assert_eq!(blocking_dependencies(dependent, &project).len(), 1);
    }

    #[test]
    fn task_whose_dependency_is_done_is_not_blocked() {
        let project = project(vec![
            task("TASK-1", "Done", &[], None),
            task("TASK-2", "To Do", &["TASK-1"], None),
        ]);
        let dependent = &project.tasks[1];

        assert!(!is_blocked(dependent, &project));
        assert!(blocking_dependencies(dependent, &project).is_empty());
    }

    #[test]
    fn unresolvable_dependency_id_does_not_block() {
        let project = project(vec![task("TASK-2", "To Do", &["TASK-999"], None)]);
        let dependent = &project.tasks[0];

        assert!(!is_blocked(dependent, &project));
    }

    #[test]
    fn blocks_is_the_reverse_of_dependencies() {
        let project = project(vec![
            task("TASK-1", "To Do", &[], None),
            task("TASK-2", "To Do", &["TASK-1"], None),
            task("TASK-3", "To Do", &["TASK-1"], None),
        ]);
        let root = &project.tasks[0];

        let blocked = blocks(root, &project);
        assert_eq!(blocked.len(), 2);
        assert!(blocked.iter().any(|t| t.id == "TASK-2"));
        assert!(blocked.iter().any(|t| t.id == "TASK-3"));
    }

    #[test]
    fn dependency_statuses_pairs_each_dep_with_its_done_state() {
        let project = project(vec![
            task("TASK-1", "Done", &[], None),
            task("TASK-2", "To Do", &[], None),
            task("TASK-3", "To Do", &["TASK-1", "TASK-2"], None),
        ]);
        let dependent = &project.tasks[2];

        let statuses = dependency_statuses(dependent, &project);
        assert_eq!(statuses.len(), 2);
        assert!(statuses.iter().any(|(t, done)| t.id == "TASK-1" && *done));
        assert!(statuses.iter().any(|(t, done)| t.id == "TASK-2" && !*done));
    }

    #[test]
    fn newly_unblocked_when_a_dependency_completed_within_the_window() {
        let mut dep = task("TASK-1", "Done", &[], None);
        dep.updated_date = Some("2026-06-18 00:00".to_string());
        let dependent = task("TASK-2", "To Do", &["TASK-1"], None);
        let project = project(vec![dep, dependent]);
        let today = parse_backlog_day("2026-06-20 00:00").unwrap();

        assert!(is_newly_unblocked(&project.tasks[1], &project, today));
    }

    #[test]
    fn not_newly_unblocked_when_the_dependency_completed_long_ago() {
        let mut dep = task("TASK-1", "Done", &[], None);
        dep.updated_date = Some("2026-01-01 00:00".to_string());
        let dependent = task("TASK-2", "To Do", &["TASK-1"], None);
        let project = project(vec![dep, dependent]);
        let today = parse_backlog_day("2026-06-20 00:00").unwrap();

        assert!(!is_newly_unblocked(&project.tasks[1], &project, today));
    }

    #[test]
    fn not_newly_unblocked_when_still_blocked_by_another_dependency() {
        let mut dep_done = task("TASK-1", "Done", &[], None);
        dep_done.updated_date = Some("2026-06-19 00:00".to_string());
        let dep_open = task("TASK-2", "To Do", &[], None);
        let dependent = task("TASK-3", "To Do", &["TASK-1", "TASK-2"], None);
        let project = project(vec![dep_done, dep_open, dependent]);
        let today = parse_backlog_day("2026-06-20 00:00").unwrap();

        assert!(!is_newly_unblocked(&project.tasks[2], &project, today));
    }

    #[test]
    fn a_task_with_no_dependencies_is_never_newly_unblocked() {
        let solo = task("TASK-1", "To Do", &[], None);
        let project = project(vec![solo]);
        let today = parse_backlog_day("2026-06-20 00:00").unwrap();

        assert!(!is_newly_unblocked(&project.tasks[0], &project, today));
    }

    #[test]
    fn subtask_progress_counts_direct_children_only() {
        let project = project(vec![
            task("TASK-1", "To Do", &[], None),
            task("TASK-1.1", "Done", &[], Some("TASK-1")),
            task("TASK-1.2", "To Do", &[], Some("TASK-1")),
            task("TASK-1.2.1", "Done", &[], Some("TASK-1.2")),
        ]);
        let parent = &project.tasks[0];

        assert_eq!(subtask_progress(parent, &project), (1, 2));
        assert_eq!(children(parent, &project).len(), 2);
    }

    #[test]
    fn childless_task_reports_zero_zero_not_a_badge() {
        let project = project(vec![task("TASK-1", "To Do", &[], None)]);
        let task = &project.tasks[0];

        assert_eq!(subtask_progress(task, &project), (0, 0));
    }
}
