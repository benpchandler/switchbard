//! Paint rules in a hierarchy. The first rule is the base and colors whole rows;
//! every rule below it colors only its own scope, so lower means more specific.
//! Reordering the list flips which paint dominates. Saved with the view.

use std::str::FromStr;

use ratatui::style::Color;
use switchbard_core::BacklogTask;

use crate::config::Column;
use crate::tasks::{Filter, FilterField};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintRule {
    /// One color per value of a categorical column. As the base it paints whole
    /// rows; lower down it paints that column's cells.
    ByColumn {
        column: Column,
        colors: Vec<(String, String)>,
    },
    /// Every row the filter matches, whole row, wherever it sits.
    Rows { filter: String, color: String },
    /// A whole column, one color, wherever it sits.
    Column { column: Column, color: String },
}

/// Distinct colors handed out by `auto`, most common value first.
pub const AUTO_PALETTE: [&str; 8] = [
    "yellow",
    "cyan",
    "green",
    "magenta",
    "blue",
    "red",
    "lightyellow",
    "lightcyan",
];

/// Colors offered by name; hex works too when typed.
pub const NAMED_COLORS: [&str; 16] = [
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "gray",
    "white",
    "lightred",
    "lightgreen",
    "lightyellow",
    "lightblue",
    "lightmagenta",
    "lightcyan",
    "darkgray",
    "black",
];

impl PaintRule {
    /// `by:status=todo:yellow,inprogress:cyan` / `rows:status:done=gray` /
    /// `column:id=darkgray`, the saved and displayed form.
    pub fn to_text(&self) -> String {
        match self {
            PaintRule::ByColumn { column, colors } => format!(
                "by:{}={}",
                column.name(),
                colors
                    .iter()
                    .map(|(value, color)| format!("{value}:{color}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            PaintRule::Rows { filter, color } => format!("rows:{filter}={color}"),
            PaintRule::Column { column, color } => format!("column:{}={color}", column.name()),
        }
    }

    pub fn parse(text: &str) -> Option<PaintRule> {
        let (target, rhs) = text.trim().split_once('=')?;
        if let Some(column) = target.strip_prefix("by:") {
            let colors = rhs
                .split(',')
                .filter_map(|pair| {
                    let (value, color) = pair.rsplit_once(':')?;
                    Color::from_str(color).ok()?;
                    Some((value.to_string(), color.to_string()))
                })
                .collect();
            return Some(PaintRule::ByColumn {
                column: Column::parse(column.trim())?,
                colors,
            });
        }
        Color::from_str(rhs.trim()).ok()?;
        let color = rhs.trim().to_string();
        if let Some(filter) = target.strip_prefix("rows:") {
            Some(PaintRule::Rows {
                filter: filter.trim().to_string(),
                color,
            })
        } else if let Some(column) = target.strip_prefix("column:") {
            Some(PaintRule::Column {
                column: Column::parse(column.trim())?,
                color,
            })
        } else {
            None
        }
    }

    /// How the rule reads in the hierarchy list.
    pub fn label(&self) -> String {
        match self {
            PaintRule::ByColumn { column, colors } => {
                format!("by {} ({} values)", column.name(), colors.len())
            }
            PaintRule::Rows { filter, color } => format!("rows {filter} → {color}"),
            PaintRule::Column { column, color } => format!("column {} → {color}", column.name()),
        }
    }

    /// A representative color for the list: the first value's, or the rule's own.
    pub fn swatch(&self) -> Option<Color> {
        match self {
            PaintRule::ByColumn { colors, .. } => colors
                .first()
                .and_then(|(_, color)| Color::from_str(color).ok()),
            PaintRule::Rows { color, .. } | PaintRule::Column { color, .. } => {
                Color::from_str(color).ok()
            }
        }
    }

    /// The color this rule gives a cell, honoring `is_base` for by-column rules.
    fn claim(&self, task: &BacklogTask, column: Column, is_base: bool) -> Option<Color> {
        match self {
            PaintRule::ByColumn {
                column: painted,
                colors,
            } => {
                if *painted != column && !is_base {
                    return None;
                }
                let field = painted.filter_field()?;
                let value = field_value(task, field)?;
                colors
                    .iter()
                    .find(|(known, _)| *known == Filter::loose_key(&value))
                    .and_then(|(_, color)| Color::from_str(color).ok())
            }
            PaintRule::Rows { filter, color } => Filter::parse(filter)
                .matches(task)
                .then(|| Color::from_str(color).ok())
                .flatten(),
            PaintRule::Column {
                column: painted,
                color,
            } => (*painted == column)
                .then(|| Color::from_str(color).ok())
                .flatten(),
        }
    }
}

fn field_value(task: &BacklogTask, field: FilterField) -> Option<String> {
    match field {
        FilterField::Id => Some(task.id.clone()),
        FilterField::Status => Some(task.status.clone()),
        FilterField::Priority => Some(task.priority.clone()),
        FilterField::Label => task.labels.first().cloned(),
        FilterField::Project => task.project.clone(),
    }
}

/// The color for one cell: the lowest (most specific) rule that claims it wins;
/// the top rule is the base and claims whole rows.
pub fn cell_color(rules: &[PaintRule], task: &BacklogTask, column: Column) -> Option<Color> {
    rules
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, rule)| rule.claim(task, column, index == 0))
}

/// The color a by-column rule assigns `value`, if any.
pub fn value_color(rules: &[PaintRule], column: Column, value: &str) -> Option<String> {
    rules.iter().find_map(|rule| match rule {
        PaintRule::ByColumn {
            column: painted,
            colors,
        } if *painted == column => colors
            .iter()
            .find(|(known, _)| *known == Filter::loose_key(value))
            .map(|(_, color)| color.clone()),
        _ => None,
    })
}

/// Sets (or with `None`, clears) one value's color on `column`'s by-column rule,
/// creating the rule at the bottom when it does not exist yet.
pub fn set_value_color(
    rules: &mut Vec<PaintRule>,
    column: Column,
    value: &str,
    color: Option<&str>,
) {
    let key = Filter::loose_key(value);
    let position = rules.iter().position(
        |rule| matches!(rule, PaintRule::ByColumn { column: painted, .. } if *painted == column),
    );
    let index = match position {
        Some(index) => index,
        None => {
            rules.push(PaintRule::ByColumn {
                column,
                colors: Vec::new(),
            });
            rules.len() - 1
        }
    };
    if let PaintRule::ByColumn { colors, .. } = &mut rules[index] {
        colors.retain(|(known, _)| *known != key);
        if let Some(color) = color {
            colors.push((key, color.to_string()));
        }
    }
    if matches!(&rules[index], PaintRule::ByColumn { colors, .. } if colors.is_empty()) {
        rules.remove(index);
    }
}

/// Replaces (or with `none`, removes) the rule for a rows filter or a whole column.
pub fn set_rule(rules: &mut Vec<PaintRule>, rule: PaintRule) {
    let same_target = |existing: &PaintRule| match (existing, &rule) {
        (PaintRule::Rows { filter: a, .. }, PaintRule::Rows { filter: b, .. }) => a == b,
        (PaintRule::Column { column: a, .. }, PaintRule::Column { column: b, .. }) => a == b,
        _ => false,
    };
    let color = match &rule {
        PaintRule::Rows { color, .. } | PaintRule::Column { color, .. } => color.clone(),
        PaintRule::ByColumn { .. } => String::new(),
    };
    match rules.iter().position(same_target) {
        Some(index) if color == "none" => {
            rules.remove(index);
        }
        Some(index) => rules[index] = rule,
        None if color == "none" => {}
        None => rules.push(rule),
    }
}

pub fn rules_text(rules: &[PaintRule]) -> String {
    rules
        .iter()
        .map(PaintRule::to_text)
        .collect::<Vec<_>>()
        .join(";")
}

pub fn parse_rules(text: &str) -> Vec<PaintRule> {
    text.split(';').filter_map(PaintRule::parse).collect()
}
