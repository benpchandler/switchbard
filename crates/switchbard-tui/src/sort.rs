//! Sort orders picked with `s <column>`: plain ascending/descending, or the semantic
//! order a column's vocabulary already implies (high before low, To Do before Done).

use std::cmp::Ordering;
use std::collections::HashSet;

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
    blocked: &HashSet<String>,
) {
    visible.sort_by(|&a, &b| compare(&tasks[a], &tasks[b], sort, top, goals, blocked));
}

fn compare(
    a: &BacklogTask,
    b: &BacklogTask,
    sort: Sort,
    top: &[String],
    goals: &[GoalDef],
    blocked: &HashSet<String>,
) -> Ordering {
    compare_values(
        &crate::column_values::TaskValues {
            task: a,
            top,
            goals,
            blocked,
        },
        &crate::column_values::TaskValues {
            task: b,
            top,
            goals,
            blocked,
        },
        sort,
    )
}

pub fn compare_values(
    a: &impl crate::column_values::ColumnValues,
    b: &impl crate::column_values::ColumnValues,
    sort: Sort,
) -> Ordering {
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
