//! Task loading and the filter language shared by `/`, `f`, and saved views.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use switchbard_core::{
    blocking_dependencies, blocks, compute_goal_statuses, compute_hierarchy_rollup, is_blocked,
    load_backlog_repo, subtask_progress, week_monday_of, BacklogRepo, BacklogTask,
    BacklogTaskSource, GoalDef,
};

/// What a project section heading needs, computed once per reload from the
/// core roll-up: def status, done/total, initiative. Ordered by stack rank,
/// unranked projects after by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSummary {
    pub name: String,
    pub status: Option<String>,
    pub done: usize,
    pub total: usize,
    pub initiative: Option<String>,
}

/// What a goal section heading needs: this week's actual against target and
/// pace, when the goal has a week on the clock; otherwise just the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalSummary {
    pub name: String,
    pub progress: Option<GoalProgress>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalProgress {
    pub actual: i64,
    pub target: i64,
    pub unit: String,
    pub pace: &'static str,
}

pub struct Backlog {
    pub tasks: Vec<BacklogTask>,
    pub projects: Vec<ProjectSummary>,
    /// The repo's goal definitions; the goal column derives membership from them.
    pub goals: Vec<GoalDef>,
    /// Goal headings' facts for the current week, in `goals.yml` order.
    pub goal_summaries: Vec<GoalSummary>,
    /// The top list: the expedite lane, in order, pruned to tasks that are loaded.
    pub top: Vec<String>,
    /// Dependency and sub-task facts, computed once from `backlog_relations`.
    pub relations: TaskRelations,
}

/// Dependency and sub-task facts derived once per load from
/// `switchbard_core::backlog_relations` (task-18/17's core functions are
/// O(project) each; recomputing them per row per frame would make sbt's
/// frame cost scale with rows x tasks instead of once with tasks).
/// `blocked` backs the `blocked:` filter field and the dimmed-row style;
/// `blocked_by`/`blocks` back the detail pane's "Blocked by"/"Blocks" lists;
/// `subtasks` backs the parent roll-up badge (TASK-209.2/209.3). A done task
/// is never counted as blocked even when a listed dependency is still open,
/// matching the GUI's own `!task.is_done() && is_blocked(...)` guard
/// (`ui/backlog/list.rs`) — there is exactly one definition of "blocked"
/// (`backlog_relations::is_blocked`); this struct only caches its answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskRelations {
    pub blocked: HashSet<String>,
    /// Open dependencies only, by blocked task id: (dependency id, dependency title).
    pub blocked_by: HashMap<String, Vec<(String, String)>>,
    /// Every task that depends on this one, by task id: (dependent id, dependent title, dependent done).
    pub blocks: HashMap<String, Vec<(String, String, bool)>>,
    /// (done, total) direct sub-tasks, by parent id; absent when childless.
    pub subtasks: HashMap<String, (usize, usize)>,
}

impl TaskRelations {
    fn compute(repo: &BacklogRepo, tasks: &[BacklogTask]) -> TaskRelations {
        let mut relations = TaskRelations::default();
        for task in tasks {
            if !task.is_done() && is_blocked(task, repo) {
                relations.blocked.insert(task.id.clone());
                relations.blocked_by.insert(
                    task.id.clone(),
                    blocking_dependencies(task, repo)
                        .into_iter()
                        .map(|dep| (dep.id.clone(), dep.title.clone()))
                        .collect(),
                );
            }
            let dependents: Vec<(String, String, bool)> = blocks(task, repo)
                .into_iter()
                .map(|dep| (dep.id.clone(), dep.title.clone(), dep.is_done()))
                .collect();
            if !dependents.is_empty() {
                relations.blocks.insert(task.id.clone(), dependents);
            }
            let progress = subtask_progress(task, repo);
            if progress.1 > 0 {
                relations.subtasks.insert(task.id.clone(), progress);
            }
        }
        relations
    }
}

pub fn load(root: &Path) -> Result<Backlog> {
    let repo = load_backlog_repo(root)?;
    let rollup = compute_hierarchy_rollup(&[&repo]);
    let mut projects: Vec<ProjectSummary> = rollup
        .initiatives
        .iter()
        .flat_map(|initiative| initiative.projects.iter())
        .map(|project| ProjectSummary {
            name: project.name.clone(),
            status: project.status.clone(),
            done: project.done,
            total: project.total,
            initiative: project.initiative.clone(),
        })
        .collect();
    projects.sort_by_key(|project| {
        (
            repo.ranking
                .project_rank(&project.name)
                .unwrap_or(usize::MAX),
            project.name.clone(),
        )
    });
    let goal_summaries = goal_summaries(&repo, chrono::Local::now().date_naive());
    let goals = repo.goals.clone();
    // Cloned rather than moved out of `repo.tasks`: `TaskRelations::compute`
    // below needs the full, unfiltered repo (a dependency can name a task
    // that has since moved to `backlog/completed/tasks` — `Completed` source
    // is still a resolvable node in the dependency/sub-task graph, just not
    // one sbt lists as its own row).
    let tasks: Vec<BacklogTask> = repo
        .tasks
        .iter()
        .filter(|task| {
            matches!(
                task.source,
                BacklogTaskSource::Active | BacklogTaskSource::Draft
            )
        })
        .cloned()
        .collect();
    let relations = TaskRelations::compute(&repo, &tasks);
    let top: Vec<String> = repo
        .ranking
        .expedite
        .iter()
        .filter(|id| tasks.iter().any(|task: &BacklogTask| task.id == **id))
        .cloned()
        .collect();
    Ok(Backlog {
        tasks,
        projects,
        goals,
        goal_summaries,
        top,
        relations,
    })
}

fn goal_summaries(
    repo: &switchbard_core::BacklogRepo,
    today: chrono::NaiveDate,
) -> Vec<GoalSummary> {
    let week = week_monday_of(today).format("%Y-%m-%d").to_string();
    let statuses = compute_goal_statuses(&[repo], &week, today);
    repo.goals
        .iter()
        .map(|goal| GoalSummary {
            name: goal.name.clone(),
            progress: statuses
                .iter()
                .find(|status| status.name == goal.name)
                .map(|status| GoalProgress {
                    actual: status.actual,
                    target: status.target,
                    unit: status.unit.clone(),
                    pace: status.pace.label(),
                }),
        })
        .collect()
}

pub use crate::filter::{Filter, FilterField};

impl Filter {
    pub fn matches(
        &self,
        task: &BacklogTask,
        goals: &[GoalDef],
        blocked: &HashSet<String>,
    ) -> bool {
        self.matches_row(&crate::column_values::TaskValues {
            task,
            goals,
            top: &[],
            blocked,
        })
    }
}

/// Distinct values a field takes across `tasks`, most common first, with counts.
pub fn field_values(
    tasks: &[BacklogTask],
    field: FilterField,
    goals: &[GoalDef],
    blocked: &HashSet<String>,
) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for value in tasks
        .iter()
        .flat_map(|task| field.column().values(task, goals, blocked))
    {
        match counts.iter_mut().find(|(seen, _)| *seen == value) {
            Some(entry) => entry.1 += 1,
            None => counts.push((value, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
}
