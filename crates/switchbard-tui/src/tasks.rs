//! Task loading and the filter language shared by `/`, `f`, and saved views.

use std::path::Path;

use anyhow::Result;
use switchbard_core::{
    compute_goal_statuses, compute_hierarchy_rollup, load_backlog_repo, week_monday_of,
    BacklogTask, BacklogTaskSource, GoalDef,
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
    let tasks: Vec<BacklogTask> = repo
        .tasks
        .into_iter()
        .filter(|task| {
            matches!(
                task.source,
                BacklogTaskSource::Active | BacklogTaskSource::Draft
            )
        })
        .collect();
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
    pub fn matches(&self, task: &BacklogTask, goals: &[GoalDef]) -> bool {
        self.matches_row(&crate::column_values::TaskValues {
            task,
            goals,
            top: &[],
        })
    }
}

/// Distinct values a field takes across `tasks`, most common first, with counts.
pub fn field_values(
    tasks: &[BacklogTask],
    field: FilterField,
    goals: &[GoalDef],
) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for value in tasks
        .iter()
        .flat_map(|task| field.column().values(task, goals))
    {
        match counts.iter_mut().find(|(seen, _)| *seen == value) {
            Some(entry) => entry.1 += 1,
            None => counts.push((value, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
}
