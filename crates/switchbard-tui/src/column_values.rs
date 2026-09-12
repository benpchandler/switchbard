//! Explicit page adapters for the shared column, sorting and matching contracts.
use crate::{ball::Ball, columns::Column};
use switchbard_core::{goals_feeding, BacklogTask, GoalDef, PrListRow};

pub trait ColumnValues {
    fn values(&self, column: Column) -> Vec<String>;
    fn numeric_key(&self, column: Column) -> u64;
    fn identity(&self) -> &str;
    fn text(&self) -> Vec<String> {
        [self.values(Column::Id), self.values(Column::Title)].concat()
    }
}

pub struct TaskValues<'a> {
    pub task: &'a BacklogTask,
    pub goals: &'a [GoalDef],
    pub top: &'a [String],
}

impl ColumnValues for TaskValues<'_> {
    fn values(&self, column: Column) -> Vec<String> {
        let Self { task, goals, .. } = self;
        match column {
            Column::Id => vec![task.id.clone()],
            Column::Status => vec![task.status.clone()],
            Column::Priority => vec![task.priority.clone()],
            Column::Title => vec![task.title.clone()],
            Column::Filed => crate::date_fields::filed(task.created_date.as_deref()),
            Column::Due => task.due_date.clone().into_iter().collect(),
            Column::Labels => task.labels.clone(),
            Column::Project => task.project.clone().into_iter().collect(),
            Column::Ball => Ball::of(task)
                .map(|ball| ball.text().to_string())
                .into_iter()
                .collect(),
            // Rank and work are not on the task: `App::cell` supplies them
            // from the lane and the live session list.
            Column::Rank
            | Column::Work
            | Column::Lifecycle
            | Column::Tasks
            | Column::Checks
            | Column::Review
            | Column::Merge
            | Column::Draft
            | Column::Merged => Vec::new(),
            Column::Goal => goals_feeding(goals, task)
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
    fn numeric_key(&self, column: Column) -> u64 {
        match column {
            Column::Id => task_number(&self.task.id),
            Column::Rank => self
                .top
                .iter()
                .position(|id| id == &self.task.id)
                .map(|index| index as u64)
                .unwrap_or(u64::MAX),
            _ => 0,
        }
    }
    fn identity(&self) -> &str {
        &self.task.id
    }
}

pub struct PrValues<'a> {
    pub row: &'a PrListRow,
    pub links: &'a [(String, String)],
}
impl ColumnValues for PrValues<'_> {
    fn values(&self, column: Column) -> Vec<String> {
        let Self { row, links } = self;
        use switchbard_core::{PrChecks, PrLifecycle};
        match column {
            Column::Id => vec![row.number.to_string()],
            Column::Title => vec![row.title.clone()],
            Column::Merged => crate::date_fields::merged(row),
            Column::Lifecycle | Column::Status => vec![row.lifecycle.label().to_string()],
            Column::Tasks => links.iter().map(|(id, _)| id.clone()).collect(),
            Column::Checks => vec![if row.lifecycle != PrLifecycle::Open {
                "Not fetched"
            } else {
                match row.checks {
                    PrChecks::Failed => "Failed",
                    PrChecks::Unknown => "Unknown",
                    PrChecks::Running => "Pending",
                    PrChecks::Passing => "Passed",
                    PrChecks::NoneObserved => "None observed",
                }
            }
            .to_string()],
            Column::Review => vec![row.review.label().to_string()],
            Column::Merge => vec![row.merge.label().to_string()],
            Column::Draft => vec![if row.draft { "Draft" } else { "Ready" }.to_string()],
            _ => Vec::new(),
        }
    }
    fn numeric_key(&self, column: Column) -> u64 {
        if column == Column::Id {
            self.row.number
        } else {
            u64::MAX
        }
    }
    fn identity(&self) -> &str {
        &self.row.id
    }
}

fn task_number(id: &str) -> u64 {
    let digits = id.rsplit('-').next().unwrap_or(id);
    let mut parts = digits.split('.');
    let major: u64 = parts
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX / 1000);
    let minor: u64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    major.saturating_mul(1000).saturating_add(minor)
}
