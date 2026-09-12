//! Sort orders picked with `s <column>`: plain ascending/descending, or the semantic
//! order a column's vocabulary already implies (high before low, To Do before Done).

use std::cmp::Ordering;

use switchbard_core::{BacklogTask, GoalDef};

use crate::columns::Column;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Ascending,
    Descending,
    Semantic,
}

impl Order {
    pub fn label(self, column: Column) -> String {
        match self {
            Order::Ascending => "ascending".to_string(),
            Order::Descending => "descending".to_string(),
            Order::Semantic => match column {
                Column::Priority => "semantic (high, medium, low)".to_string(),
                Column::Status => "semantic (to do, in progress, done)".to_string(),
                _ => "semantic".to_string(),
            },
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Order::Ascending => "↑",
            Order::Descending => "↓",
            Order::Semantic => "≈",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub column: Column,
    pub order: Order,
}

impl Sort {
    pub fn label(&self) -> String {
        format!("{}{}", self.order.glyph(), self.column.header())
    }

    /// `pri:semantic`, the form saved views carry.
    pub fn to_text(&self) -> String {
        let order = match self.order {
            Order::Ascending => "ascending",
            Order::Descending => "descending",
            Order::Semantic => "semantic",
        };
        format!("{}:{order}", self.column.name())
    }

    pub fn parse(text: &str) -> Option<Sort> {
        let (column, order) = text.trim().split_once(':')?;
        let column = Column::parse(column)?;
        let order = match order {
            "ascending" => Order::Ascending,
            "descending" => Order::Descending,
            "semantic" => Order::Semantic,
            _ => return None,
        };
        Some(Sort { column, order })
    }
}

/// Orders offered for a column; semantic only where the vocabulary has one.
pub fn orders_for(column: Column) -> Vec<Order> {
    match column {
        _ if !column.spec().vocabulary.is_empty() => {
            vec![Order::Semantic, Order::Ascending, Order::Descending]
        }
        _ => vec![Order::Ascending, Order::Descending],
    }
}

pub fn apply(
    tasks: &[BacklogTask],
    visible: &mut [usize],
    sort: Sort,
    top: &[String],
    goals: &[GoalDef],
) {
    visible.sort_by(|&a, &b| compare(&tasks[a], &tasks[b], sort, top, goals));
}

fn compare(
    a: &BacklogTask,
    b: &BacklogTask,
    sort: Sort,
    top: &[String],
    goals: &[GoalDef],
) -> Ordering {
    compare_values(
        &crate::column_values::TaskValues {
            task: a,
            top,
            goals,
        },
        &crate::column_values::TaskValues {
            task: b,
            top,
            goals,
        },
        sort,
    )
}

pub fn compare_values(
    a: &impl crate::column_values::ColumnValues,
    b: &impl crate::column_values::ColumnValues,
    sort: Sort,
) -> Ordering {
    // Due sorts on the raw `YYYY-MM-DD` value (lexicographic == chronological
    // for that format), with an absent due date always last regardless of
    // direction — "no due date" is not a date, so ascending/descending
    // shouldn't reposition it the way a real value would.
    if sort.column == Column::Due {
        let due = |values: &dyn crate::column_values::ColumnValues| {
            values.values(Column::Due).into_iter().next()
        };
        let ordering = match (due(a), due(b)) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) if sort.order == Order::Descending => b.cmp(&a),
            (Some(a), Some(b)) => a.cmp(&b),
        };
        return ordering
            .then_with(|| a.numeric_key(Column::Id).cmp(&b.numeric_key(Column::Id)))
            .then_with(|| a.identity().cmp(b.identity()));
    }
    let plain = || {
        if sort.column.spec().numeric {
            a.numeric_key(sort.column).cmp(&b.numeric_key(sort.column))
        } else {
            a.values(sort.column)
                .join(",")
                .to_lowercase()
                .cmp(&b.values(sort.column).join(",").to_lowercase())
        }
    };
    let ordering = match sort.order {
        Order::Semantic => {
            let rank = |values: Vec<String>| {
                values
                    .first()
                    .map(|value| sort.column.vocabulary_rank(value))
                    .unwrap_or(usize::MAX)
            };
            rank(a.values(sort.column)).cmp(&rank(b.values(sort.column)))
        }
        Order::Ascending => plain(),
        Order::Descending => plain().reverse(),
    };
    ordering
        .then_with(|| a.numeric_key(Column::Id).cmp(&b.numeric_key(Column::Id)))
        .then_with(|| a.identity().cmp(b.identity()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::BacklogTaskSource;

    fn task(id: &str, due: Option<&str>) -> BacklogTask {
        BacklogTask {
            storage_identity: None,
            id: id.to_string(),
            title: id.to_string(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: None,
            updated_date: None,
            due_date: due.map(str::to_string),
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: std::path::PathBuf::from(format!("/repo/backlog/tasks/{id}.md")),
        }
    }

    fn ordered(tasks: &[BacklogTask], order: Order) -> Vec<String> {
        let goals = [];
        let mut visible: Vec<usize> = (0..tasks.len()).collect();
        apply(
            tasks,
            &mut visible,
            Sort {
                column: Column::Due,
                order,
            },
            &[],
            &goals,
        );
        visible.into_iter().map(|i| tasks[i].id.clone()).collect()
    }

    #[test]
    fn ascending_orders_by_date_with_the_absent_value_last() {
        let tasks = [
            task("TASK-1", Some("2026-09-20")),
            task("TASK-2", None),
            task("TASK-3", Some("2026-09-14")),
        ];
        assert_eq!(
            ordered(&tasks, Order::Ascending),
            vec!["TASK-3", "TASK-1", "TASK-2"]
        );
    }

    /// Descending still keeps an absent due date last — it is not a date
    /// that reverses with the rest, it is the absence of one.
    #[test]
    fn descending_reverses_dated_tasks_but_still_keeps_the_absent_value_last() {
        let tasks = [
            task("TASK-1", Some("2026-09-20")),
            task("TASK-2", None),
            task("TASK-3", Some("2026-09-14")),
        ];
        assert_eq!(
            ordered(&tasks, Order::Descending),
            vec!["TASK-1", "TASK-3", "TASK-2"]
        );
    }

    #[test]
    fn two_absent_due_dates_break_ties_by_id() {
        let tasks = [task("TASK-2", None), task("TASK-1", None)];
        assert_eq!(ordered(&tasks, Order::Ascending), vec!["TASK-1", "TASK-2"]);
    }
}
