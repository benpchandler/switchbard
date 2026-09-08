//! Semantic paint precedence over shared field values. Tokens stay opaque here;
//! the terminal adapter owns validating and converting them into Ratatui colors.
use crate::{columns::Column, filter::Filter, paint::PaintRule};

pub fn cell_token(
    rules: &[PaintRule],
    column: Column,
    values: impl Fn(Column) -> Vec<String>,
    matches: impl Fn(&Filter) -> bool,
) -> Option<&str> {
    rules
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, rule)| claim(rule, column, index == 0, &values, &matches))
}

fn claim<'a>(
    rule: &'a PaintRule,
    column: Column,
    base: bool,
    values: &impl Fn(Column) -> Vec<String>,
    matches: &impl Fn(&Filter) -> bool,
) -> Option<&'a str> {
    match rule {
        PaintRule::ByColumn {
            column: target,
            colors,
        } if base || *target == column => {
            let values = values(*target);
            colors
                .iter()
                .find(|(known, _)| {
                    values
                        .iter()
                        .any(|value| *known == Filter::loose_key(value))
                })
                .map(|(_, token)| token.as_str())
        }
        PaintRule::Rows { filter, color } => matches(&Filter::parse(filter)).then_some(color),
        PaintRule::Column {
            column: target,
            color,
        } if *target == column => Some(color),
        _ => None,
    }
}
