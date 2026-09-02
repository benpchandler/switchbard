//! Sort orders picked with `s <column>`: plain ascending/descending, or the semantic
//! order a column's vocabulary already implies (high before low, To Do before Done).

use std::cmp::Ordering;

use switchbard_core::{BacklogTask, BACKLOG_PRIORITIES, CANONICAL_STATUS_ORDER};

use crate::config::Column;

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
        Column::Priority | Column::Status => {
            vec![Order::Semantic, Order::Ascending, Order::Descending]
        }
        _ => vec![Order::Ascending, Order::Descending],
    }
}

pub fn apply(tasks: &[BacklogTask], visible: &mut [usize], sort: Sort) {
    visible.sort_by(|&a, &b| compare(&tasks[a], &tasks[b], sort));
}

fn compare(a: &BacklogTask, b: &BacklogTask, sort: Sort) -> Ordering {
    let ordering = match sort.order {
        Order::Semantic => semantic_rank(a, sort.column).cmp(&semantic_rank(b, sort.column)),
        Order::Ascending => plain_key(a, sort.column).cmp(&plain_key(b, sort.column)),
        Order::Descending => plain_key(b, sort.column).cmp(&plain_key(a, sort.column)),
    };
    ordering.then_with(|| id_number(&a.id).cmp(&id_number(&b.id)))
}

fn plain_key(task: &BacklogTask, column: Column) -> (u64, String) {
    match column {
        Column::Id => (id_number(&task.id), String::new()),
        Column::Status => (0, task.status.to_lowercase()),
        Column::Priority => (0, task.priority.to_lowercase()),
        Column::Title => (0, task.title.to_lowercase()),
        Column::Labels => (0, task.labels.join(",").to_lowercase()),
        Column::Project => (0, task.project.clone().unwrap_or_default().to_lowercase()),
        Column::Ball => (
            0,
            crate::ball::Ball::text(crate::ball::Ball::of(task)).to_string(),
        ),
    }
}

fn semantic_rank(task: &BacklogTask, column: Column) -> usize {
    let value = match column {
        Column::Priority => &task.priority,
        Column::Status => &task.status,
        _ => return 0,
    };
    vocabulary_rank(column, value)
}

/// Where `value` sits in the column's vocabulary (high before low, To Do before
/// Done); unknown values and columns without a vocabulary rank last.
pub fn vocabulary_rank(column: Column, value: &str) -> usize {
    let vocabulary: &[&str] = match column {
        Column::Priority => BACKLOG_PRIORITIES,
        Column::Status => CANONICAL_STATUS_ORDER,
        _ => return usize::MAX,
    };
    vocabulary
        .iter()
        .position(|known| known.eq_ignore_ascii_case(value))
        .unwrap_or(vocabulary.len())
}

/// `TASK-12.3` sorts by 12 then 3; unparseable ids sort last.
fn id_number(id: &str) -> u64 {
    let digits = id.rsplit('-').next().unwrap_or(id);
    let mut parts = digits.split('.');
    let major: u64 = parts
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX / 1000);
    let minor: u64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    major * 1000 + minor
}
