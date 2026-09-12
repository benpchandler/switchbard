//! Sort orders picked with `s <column>`: plain ascending/descending, or the semantic
//! order a column's vocabulary already implies (high before low, To Do before Done).

use std::cmp::Ordering;
use std::collections::HashSet;

use switchbard_core::{BacklogTask, GoalDef};

use crate::columns::{Column, ColumnRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Ascending,
    Descending,
    Semantic,
}

impl Order {
    pub fn label(self, column: Column, registry: &ColumnRegistry) -> String {
        match self {
            Order::Ascending => "ascending".to_string(),
            Order::Descending => "descending".to_string(),
            Order::Semantic => match column {
                Column::Priority => "semantic (high, medium, low)".to_string(),
                Column::Status => "semantic (to do, in progress, done)".to_string(),
                // A declared enum field's order is its own `values` list, which
                // the repo can make arbitrarily long; name the column instead.
                _ => format!("semantic ({} order)", column.name(registry)),
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
    pub fn label(&self, registry: &ColumnRegistry) -> String {
        format!("{}{}", self.order.glyph(), self.column.header(registry))
    }

    /// `pri:semantic`, the form saved views carry.
    pub fn to_text(&self, registry: &ColumnRegistry) -> String {
        let order = match self.order {
            Order::Ascending => "ascending",
            Order::Descending => "descending",
            Order::Semantic => "semantic",
        };
        format!("{}:{order}", self.column.save_name(registry))
    }

    pub fn parse(text: &str, registry: &ColumnRegistry) -> Option<Sort> {
        // From the right: a declared field writes itself as `field:<name>`, so
        // the column part may carry a colon of its own. An order never does.
        let (column, order) = text.trim().rsplit_once(':')?;
        let column = registry.parse(column)?;
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
pub fn orders_for(column: Column, registry: &ColumnRegistry) -> Vec<Order> {
    if column.spec(registry).vocabulary().is_empty() {
        vec![Order::Ascending, Order::Descending]
    } else {
        vec![Order::Semantic, Order::Ascending, Order::Descending]
    }
}

pub fn apply(
    registry: &ColumnRegistry,
    tasks: &[BacklogTask],
    visible: &mut [usize],
    sort: Sort,
    top: &[String],
    goals: &[GoalDef],
    blocked: &HashSet<String>,
) {
    visible.sort_by(|&a, &b| compare(registry, &tasks[a], &tasks[b], sort, top, goals, blocked));
}

fn compare(
    registry: &ColumnRegistry,
    a: &BacklogTask,
    b: &BacklogTask,
    sort: Sort,
    top: &[String],
    goals: &[GoalDef],
    blocked: &HashSet<String>,
) -> Ordering {
    compare_values(
        &crate::column_values::TaskValues {
            registry,
            task: a,
            top,
            goals,
            blocked,
        },
        &crate::column_values::TaskValues {
            registry,
            task: b,
            top,
            goals,
            blocked,
        },
        sort,
        registry,
    )
}

pub fn compare_values(
    a: &impl crate::column_values::ColumnValues,
    b: &impl crate::column_values::ColumnValues,
    sort: Sort,
    registry: &ColumnRegistry,
) -> Ordering {
    // Due sorts on the raw `YYYY-MM-DD` value (lexicographic == chronological
    // for that format), with an absent due date always last regardless of
    // direction — "no due date" is not a date, so ascending/descending
    // shouldn't reposition it the way a real value would. A declared field
    // sorts on the same rule: an unset value is not a value, so it sits last
    // whichever way the rest run (`switchbard_core::custom_field_sort_key`
    // says the same for `sb list --sort`).
    if sort.column == Column::Due || matches!(sort.column, Column::Custom(_)) {
        let first = |values: &dyn crate::column_values::ColumnValues| {
            values.values(sort.column).into_iter().next()
        };
        let rank = |value: &str| sort.column.vocabulary_rank(registry, value);
        let ordering = match (first(a), first(b)) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            // Declared order for an enum field; every other kind ranks flat,
            // so the raw value is what orders it.
            (Some(a), Some(b)) => match sort.order {
                Order::Semantic => rank(&a).cmp(&rank(&b)).then_with(|| a.cmp(&b)),
                Order::Ascending => a.cmp(&b),
                Order::Descending => b.cmp(&a),
            },
        };
        return ordering
            .then_with(|| a.numeric_key(Column::Id).cmp(&b.numeric_key(Column::Id)))
            .then_with(|| a.identity().cmp(b.identity()));
    }
    let plain = || {
        if sort.column.spec(registry).numeric {
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
                    .map(|value| sort.column.vocabulary_rank(registry, value))
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
            custom: std::collections::BTreeMap::new(),
        }
    }

    fn ordered(tasks: &[BacklogTask], order: Order) -> Vec<String> {
        let goals = [];
        let mut visible: Vec<usize> = (0..tasks.len()).collect();
        let blocked = std::collections::HashSet::new();
        let registry = ColumnRegistry::builtin_only();
        apply(
            &registry,
            tasks,
            &mut visible,
            Sort {
                column: Column::Due,
                order,
            },
            &[],
            &goals,
            &blocked,
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
