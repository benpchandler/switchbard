//! Sort orders picked with `s <column>`: plain ascending/descending, or the semantic
//! order a column's vocabulary already implies (high before low, To Do before Done).

use std::cmp::Ordering;

use switchbard_core::BacklogTask;

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
        Column::Priority | Column::Status => {
            vec![Order::Semantic, Order::Ascending, Order::Descending]
        }
        _ => vec![Order::Ascending, Order::Descending],
    }
}

pub fn apply(tasks: &[BacklogTask], visible: &mut [usize], sort: Sort, top: &[String]) {
    visible.sort_by(|&a, &b| compare(&tasks[a], &tasks[b], sort, top));
}

fn compare(a: &BacklogTask, b: &BacklogTask, sort: Sort, top: &[String]) -> Ordering {
    let ordering = match sort.order {
        Order::Semantic => semantic_rank(a, sort.column).cmp(&semantic_rank(b, sort.column)),
        Order::Ascending => plain_key(a, sort.column, top).cmp(&plain_key(b, sort.column, top)),
        Order::Descending => plain_key(b, sort.column, top).cmp(&plain_key(a, sort.column, top)),
    };
    ordering.then_with(|| id_number(&a.id).cmp(&id_number(&b.id)))
}

fn plain_key(task: &BacklogTask, column: Column, top: &[String]) -> (u64, String) {
    match column {
        Column::Id => (id_number(&task.id), String::new()),
        Column::Rank => (
            top.iter()
                .position(|id| *id == task.id)
                .map(|p| p as u64)
                .unwrap_or(u64::MAX),
            String::new(),
        ),
        other => (0, other.cell_text(task).to_lowercase()),
    }
}

fn semantic_rank(task: &BacklogTask, column: Column) -> usize {
    match column.values(task).first() {
        Some(value) => column.vocabulary_rank(value),
        None => usize::MAX,
    }
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
