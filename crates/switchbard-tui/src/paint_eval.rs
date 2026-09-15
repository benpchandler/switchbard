//! Semantic paint precedence over shared field values. Tokens stay opaque here;
//! the terminal adapter owns converting them into Ratatui styles. Lower rules
//! are visited first. A matching stop marker ends evaluation for that cell;
//! otherwise more specific attributes override while other attributes merge.
use crate::{
    columns::{Column, ColumnRegistry},
    filter::Filter,
    paint::PaintRule,
};

pub fn cell_token<'a>(
    rules: &'a [PaintRule],
    column: Column,
    registry: &ColumnRegistry,
    values: impl Fn(Column) -> Vec<String>,
    matches: impl Fn(&Filter) -> bool,
) -> Option<&'a str> {
    let mut token = None;
    visit_cell_tokens(rules, column, registry, values, matches, |next| {
        if token.is_none() {
            token = Some(next);
        }
    });
    token
}

pub fn visit_cell_tokens<'a>(
    rules: &'a [PaintRule],
    column: Column,
    registry: &ColumnRegistry,
    values: impl Fn(Column) -> Vec<String>,
    matches: impl Fn(&Filter) -> bool,
    mut visit: impl FnMut(&'a str),
) {
    for (index, rule) in rules.iter().enumerate().rev() {
        if let Some(token) = claim(rule, column, registry, index == 0, &values, &matches) {
            visit(token);
            if rule.stops() {
                break;
            }
        }
    }
}

pub fn validate_rules(rules: &[PaintRule], registry: &ColumnRegistry) -> Result<(), String> {
    if rules.len() > 256 {
        return Err("a view supports at most 256 paint rules".into());
    }
    let mut band = None;
    for rule in rules {
        let lists = rule.role_lists();
        for (index, roles) in lists.iter().enumerate() {
            crate::paint::validate_roles(roles)?;
            if roles.contains('!') && index + 1 != lists.len() {
                return Err("stop marker ! must end the entire rule".into());
            }
        }
        let produces_band = rule.role_lists().iter().any(|roles| {
            roles
                .trim_end_matches('!')
                .split('+')
                .any(|token| token.trim() == "band")
        });
        if produces_band {
            if let Some(first) = band {
                return Err(format!(
                    "band already belongs to {first}; remove it before adding another band rule"
                ));
            }
            band = Some(rule.label(registry));
        }
    }
    Ok(())
}

fn claim<'a>(
    rule: &'a PaintRule,
    column: Column,
    registry: &ColumnRegistry,
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
        PaintRule::Rows { filter, color } => {
            matches(&Filter::parse(filter, registry)).then_some(color)
        }
        PaintRule::Column {
            column: target,
            color,
        } if *target == column => Some(color),
        _ => None,
    }
}
