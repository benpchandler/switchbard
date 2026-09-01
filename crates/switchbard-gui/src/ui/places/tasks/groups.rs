//! TASK-97: the generic group-by engine. `build_groups` buckets the current
//! frame's visible+filtered rows by one [`TaskField`], computing each
//! group's roll-up (done/total, always) and — for `TaskField::Project`
//! only — the def-backed metadata the expanded header summary shows
//! (status chip, target date, description, goal pace). Every other field
//! gets the plain roll-up; there is deliberately no per-field bespoke
//! renderer (`projects.rs`'s pre-TASK-97 pattern) — one engine, one arm per
//! field in `fields::field_values`, same discipline the binding directive
//! asks for.

use std::collections::BTreeMap;

use crate::ui::backlog::{RepoRow, TaskRow};
use switchbard_core::{compute_hierarchy_rollup, BacklogRepo, GoalPace};

use super::fields::{self, TaskField, NO_PROJECT};

/// A goal that counts this project, computed fresh (never stored) via
/// `switchbard_core::compute_goal_statuses` — the expanded header's "goal:
/// behind · 1/4" chip (mock §3).
pub(super) struct GoalChip {
    pub pace: GoalPace,
    pub actual: i64,
    pub target: i64,
}

/// One group-by bucket for this frame: its computed roll-up, and — for
/// `Project` only — the def metadata the expanded summary band shows.
/// `rows` are every visible row in this bucket, in the scope's stable
/// order (`sort::visible_task_rows`'s own order — grouping is a partition,
/// not a re-sort).
pub(super) struct Group<'a> {
    /// Stable identity for expand/collapse persistence and tests — the
    /// bucket value itself (`fields::field_values`'s output), which is
    /// already display-ready for every field.
    pub key: String,
    pub done: usize,
    pub total: usize,
    /// `Project` field only: the def's status vocabulary value, when a def
    /// exists.
    pub status_chip: Option<String>,
    pub target_date: Option<String>,
    pub description: String,
    pub goal: Option<GoalChip>,
    pub rows: Vec<TaskRow<'a>>,
}

/// Bucket `tasks` by `field`. `Project` gets `compute_hierarchy_rollup`'s
/// structure (so a defined-but-empty project still shows a 0/0 group,
/// matching the pre-TASK-97 Projects lens's own behavior) plus goal/def
/// metadata; every other field is a plain value → rows partition, done/
/// total computed straight from the visible rows.
pub(super) fn build_groups<'a>(
    field: TaskField,
    scoped: &[&RepoRow],
    tasks: &[TaskRow<'a>],
) -> Vec<Group<'a>> {
    let mut buckets: BTreeMap<String, Vec<TaskRow<'a>>> = BTreeMap::new();
    for row in tasks {
        for value in fields::field_values(row.task, row.repo, field) {
            buckets.entry(value).or_default().push(*row);
        }
    }

    if field == TaskField::Project {
        return project_groups(scoped, buckets);
    }

    buckets
        .into_iter()
        .map(|(key, rows)| generic_group(key, rows))
        .collect()
}

/// Group-by "None": every visible row in one synthetic, header-suppressed
/// group — `list_body::render`'s `show_headers: false` then flattens it as
/// a plain list, same sub-issue nesting rule as any real group.
pub(super) fn build_groups_ungrouped<'a>(tasks: &[TaskRow<'a>]) -> Vec<Group<'a>> {
    vec![generic_group(String::new(), tasks.to_vec())]
}

fn generic_group<'a>(key: String, rows: Vec<TaskRow<'a>>) -> Group<'a> {
    let done = rows.iter().filter(|row| row.task.is_done()).count();
    let total = rows.len();
    Group {
        key,
        done,
        total,
        status_chip: None,
        target_date: None,
        description: String::new(),
        goal: None,
        rows,
    }
}

fn project_groups<'a>(
    scoped: &[&RepoRow],
    mut buckets: BTreeMap<String, Vec<TaskRow<'a>>>,
) -> Vec<Group<'a>> {
    let repos: Vec<&BacklogRepo> = scoped.iter().map(|row| &row.repo).collect();
    let rollup = compute_hierarchy_rollup(&repos);
    let today = chrono::Local::now().date_naive();
    let week = switchbard_core::week_monday_of(today)
        .format("%Y-%m-%d")
        .to_string();

    let mut groups: Vec<Group<'a>> = Vec::new();
    for initiative in &rollup.initiatives {
        for project in &initiative.projects {
            let rows = buckets.remove(project.name.as_str()).unwrap_or_default();
            let done = rows.iter().filter(|row| row.task.is_done()).count();
            let total = rows.len();
            groups.push(Group {
                key: project.name.clone(),
                done,
                total,
                status_chip: project.status.clone(),
                target_date: project.target_date.clone(),
                description: project_description(scoped, &project.name),
                goal: goal_chip_for_project(scoped, &project.name, &week, today),
                rows,
            });
        }
    }
    // "No project" — built from whatever `field_values` bucketed under
    // `NO_PROJECT`, not `rollup.unassigned_*` (which counts the *whole*
    // scope, not just the visible/filtered rows this frame draws — the same
    // "join visible rows onto rollup structure" rule the pre-TASK-97
    // Projects lens already followed).
    if let Some(rows) = buckets.remove(NO_PROJECT) {
        let done = rows.iter().filter(|row| row.task.is_done()).count();
        let total = rows.len();
        groups.push(Group {
            key: NO_PROJECT.to_string(),
            done,
            total,
            status_chip: None,
            target_date: None,
            description: String::new(),
            goal: None,
            rows,
        });
    }
    groups
}

fn project_description(scoped: &[&RepoRow], name: &str) -> String {
    scoped
        .iter()
        .find_map(|row| row.repo.project_defs.iter().find(|def| def.name == name))
        .map(|def| def.description.clone())
        .unwrap_or_default()
}

/// The first scoped repo carrying a goal that counts `project_name` — by
/// explicit `scope` match or an attached input (TASK-92's `goal attach`,
/// `GoalDef::inputs.projects`) — with that goal's status for `week`.
/// First-match-wins across repos, the same conflict rule
/// `compute_hierarchy_rollup` uses for def name collisions.
fn goal_chip_for_project(
    scoped: &[&RepoRow],
    project_name: &str,
    week: &str,
    today: chrono::NaiveDate,
) -> Option<GoalChip> {
    for row in scoped {
        let Some(goal) = row.repo.goals.iter().find(|goal| {
            goal.scope.as_deref() == Some(project_name)
                || goal.inputs.projects.iter().any(|name| name == project_name)
        }) else {
            continue;
        };
        let statuses = switchbard_core::compute_goal_statuses(&[&row.repo], week, today);
        if let Some(status) = statuses.into_iter().find(|status| status.name == goal.name) {
            return Some(GoalChip {
                pace: status.pace,
                actual: status.actual,
                target: status.target,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use switchbard_core::{BacklogTask, BacklogTaskSource, ProjectDef};

    fn task(id: &str, project: Option<&str>, status: &str) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: id.to_string(),
            status: status.to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: project.map(str::to_string),
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/tmp/fixture/backlog/tasks/t.md"),
        }
    }

    fn repo_row(tasks: Vec<BacklogTask>, project_defs: Vec<ProjectDef>) -> RepoRow {
        RepoRow {
            key: PathBuf::from("/tmp/fixture"),
            repo_name: "fixture".to_string(),
            worktree_label: "main".to_string(),
            branch: None,
            repo: BacklogRepo {
                root: PathBuf::from("/tmp/fixture"),
                tasks,
                warnings: vec![],
                project_defs,
                initiative_defs: vec![],
                goals: vec![],
                ranking: switchbard_core::RepoRanking::default(),
                loaded_at_unix: 0,
                configured_statuses: vec![],
            },
        }
    }

    fn rows(row: &RepoRow) -> Vec<TaskRow<'_>> {
        row.repo
            .tasks
            .iter()
            .map(|task| TaskRow { repo: row, task })
            .collect()
    }

    #[test]
    fn grouping_by_status_buckets_by_the_raw_status_string() {
        let row = repo_row(
            vec![
                task("TASK-1", None, "To Do"),
                task("TASK-2", None, "To Do"),
                task("TASK-3", None, "Done"),
            ],
            vec![],
        );
        let tasks = rows(&row);
        let groups = build_groups(TaskField::Status, &[&row], &tasks);

        let by_key: BTreeMap<&str, (usize, usize)> = groups
            .iter()
            .map(|g| (g.key.as_str(), (g.done, g.total)))
            .collect();
        assert_eq!(by_key.get("To Do"), Some(&(0, 2)));
        assert_eq!(by_key.get("Done"), Some(&(1, 1)));
    }

    #[test]
    fn grouping_by_project_includes_a_defined_but_empty_project_at_zero() {
        let row = repo_row(
            vec![task("TASK-1", Some("Alpha"), "To Do")],
            vec![ProjectDef {
                name: "Empty".to_string(),
                status: "Planned".to_string(),
                target_date: None,
                initiative: None,
                lead: None,
                description: "not yet started".to_string(),
                path: PathBuf::from("/tmp/fixture/backlog/projects/Empty.md"),
            }],
        );
        let tasks = rows(&row);
        let groups = build_groups(TaskField::Project, &[&row], &tasks);

        let empty = groups
            .iter()
            .find(|g| g.key == "Empty")
            .expect("a defined-but-empty project still appears");
        assert_eq!((empty.done, empty.total), (0, 0));
        assert_eq!(empty.description, "not yet started");
        let alpha = groups.iter().find(|g| g.key == "Alpha").unwrap();
        assert_eq!((alpha.done, alpha.total), (0, 1));
    }

    #[test]
    fn grouping_by_project_only_counts_visible_rows_not_the_whole_scope() {
        // Two tasks in "Alpha", only one passed in as "visible" (as if a
        // filter hid the other) — the group's done/total must reflect the
        // visible set, not `compute_hierarchy_rollup`'s whole-scope counts.
        let row = repo_row(
            vec![
                task("TASK-1", Some("Alpha"), "To Do"),
                task("TASK-2", Some("Alpha"), "Done"),
            ],
            vec![],
        );
        let all_rows = rows(&row);
        let visible = vec![all_rows[0]];
        let groups = build_groups(TaskField::Project, &[&row], &visible);
        let alpha = groups.iter().find(|g| g.key == "Alpha").unwrap();
        assert_eq!((alpha.done, alpha.total), (0, 1));
    }

    #[test]
    fn a_task_with_no_project_lands_in_the_no_project_bucket() {
        let row = repo_row(vec![task("TASK-1", None, "To Do")], vec![]);
        let tasks = rows(&row);
        let groups = build_groups(TaskField::Project, &[&row], &tasks);
        assert!(groups.iter().any(|g| g.key == NO_PROJECT));
    }

    #[test]
    fn labels_fan_a_task_out_into_every_one_of_its_label_groups() {
        let mut t = task("TASK-1", None, "To Do");
        t.labels = vec!["a".to_string(), "b".to_string()];
        let row = repo_row(vec![t], vec![]);
        let tasks = rows(&row);
        let groups = build_groups(TaskField::Label, &[&row], &tasks);
        let keys: Vec<&str> = groups.iter().map(|g| g.key.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
        assert!(groups.iter().all(|g| g.total == 1));
    }
}
