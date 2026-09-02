//! Paint rules: a color for rows matching a filter, or for a whole column.
//! Saved with the view; a manually painted row is a rule on its exact id.

use std::str::FromStr;

use ratatui::style::Color;
use switchbard_core::BacklogTask;

use crate::config::Column;
use crate::tasks::Filter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintTarget {
    /// Every row the filter text matches.
    Rows(String),
    Column(Column),
    /// One column's cells, only on rows the filter text matches.
    Cell(Column, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintRule {
    pub target: PaintTarget,
    pub color: String,
}

impl PaintRule {
    pub fn parsed_color(&self) -> Option<Color> {
        Color::from_str(&self.color).ok()
    }

    /// `rows:status:done=gray` / `column:priority=yellow` / `cell:priority:pri:high=red`,
    /// the saved and displayed form.
    pub fn to_text(&self) -> String {
        match &self.target {
            PaintTarget::Rows(filter) => format!("rows:{filter}={}", self.color),
            PaintTarget::Column(column) => format!("column:{}={}", column.name(), self.color),
            PaintTarget::Cell(column, filter) => {
                format!("cell:{}:{filter}={}", column.name(), self.color)
            }
        }
    }

    pub fn parse(text: &str) -> Option<PaintRule> {
        let (target, color) = text.trim().rsplit_once('=')?;
        let color = color.trim().to_string();
        Color::from_str(&color).ok()?;
        let target = if let Some(filter) = target.strip_prefix("rows:") {
            PaintTarget::Rows(filter.trim().to_string())
        } else if let Some(column) = target.strip_prefix("column:") {
            PaintTarget::Column(Column::parse(column.trim())?)
        } else if let Some(rest) = target.strip_prefix("cell:") {
            let (column, filter) = rest.split_once(':')?;
            PaintTarget::Cell(Column::parse(column.trim())?, filter.trim().to_string())
        } else {
            return None;
        };
        Some(PaintRule { target, color })
    }
}

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

/// The rule painting `field:value`, on rows or (when `cells` is set) on one column's cells.
pub fn rule_for_value<'a>(
    rules: &'a [PaintRule],
    field: &str,
    value: &str,
    cells: Option<Column>,
) -> Option<&'a PaintRule> {
    let filter = format!("{field}:{}", Filter::loose_key(value));
    let target = match cells {
        Some(column) => PaintTarget::Cell(column, filter),
        None => PaintTarget::Rows(filter),
    };
    rules.iter().rev().find(|rule| rule.target == target)
}

/// The color for one cell. Specificity wins: a cell rule beats a column rule
/// beats a row rule; within a tier the last matching rule wins.
pub fn cell_color(rules: &[PaintRule], task: &BacklogTask, column: Column) -> Option<Color> {
    let tier = |rule: &PaintRule| match &rule.target {
        PaintTarget::Rows(_) => 0,
        PaintTarget::Column(_) => 1,
        PaintTarget::Cell(_, _) => 2,
    };
    let mut best: Option<(u8, Color)> = None;
    for rule in rules {
        let applies = match &rule.target {
            PaintTarget::Rows(filter) => Filter::parse(filter).matches(task),
            PaintTarget::Column(painted) => *painted == column,
            PaintTarget::Cell(painted, filter) => {
                *painted == column && Filter::parse(filter).matches(task)
            }
        };
        if !applies {
            continue;
        }
        if let Some(color) = rule.parsed_color() {
            if best.is_none_or(|(rank, _)| tier(rule) >= rank) {
                best = Some((tier(rule), color));
            }
        }
    }
    best.map(|(_, color)| color)
}

/// Drops every rule on `target`, then appends the new one unless `color` is `none`.
pub fn with_rule(rules: &[PaintRule], target: PaintTarget, color: &str) -> Vec<PaintRule> {
    let mut kept: Vec<PaintRule> = rules
        .iter()
        .filter(|rule| rule.target != target)
        .cloned()
        .collect();
    if color != "none" {
        kept.push(PaintRule {
            target,
            color: color.to_string(),
        });
    }
    kept
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
