//! Semantic paint precedence over shared field values. Tokens stay opaque here;
//! the terminal adapter owns converting them into Ratatui styles. Lower rules
//! are visited first. A matching stop marker ends evaluation for that cell;
//! otherwise more specific attributes override while other attributes merge.
//! Within one rule, `compose` is the only place that decides which token fills
//! the cell and which writes on it.
use crate::{
    columns::{Column, ColumnRegistry},
    filter::Filter,
    paint::PaintRule,
};

/// Tokens per rule. Long enough for a fill and every modifier that reads on it,
/// short enough that an accidental paste is refused rather than rendered.
pub const MAX_ROLE_TOKENS: usize = 16;

/// What a token does to the cell it claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Paints the cell behind the text: a highlight slot, or a role carrying a
    /// background of its own.
    Fill,
    /// Writes on whatever fill is there: a color, a palette slot, or a role
    /// that only styles ink.
    Ink,
}

/// One rule's role list split into the fill it paints and the ink over it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composition<'a> {
    /// The rightmost fill in the list; an earlier fill is painted over.
    pub fill: Option<&'a str>,
    /// Every other token, in written order, whichever side of the fill it sits.
    pub ink: Vec<&'a str>,
}

/// Split `roles` into the fill it paints and the ink written over it. The
/// rightmost fill wins and every other token is ink wherever it sits, so
/// `band+red` and `red+band` both write red on the band and a fill's own
/// default ink shows only when nothing else supplies ink. `None` when a token
/// names nothing `kind` knows, or when the list is longer than
/// [`MAX_ROLE_TOKENS`]: a rule is valid only when every token resolves.
pub fn compose<'a>(
    roles: &'a str,
    kind: impl Fn(&str) -> Option<TokenKind>,
) -> Option<Composition<'a>> {
    let roles = roles.trim();
    let roles = roles.strip_suffix('!').unwrap_or(roles);
    let tokens: Vec<&str> = roles
        .split('+')
        .map(|token| token.trim().trim_end_matches('!'))
        .collect();
    if tokens.len() > MAX_ROLE_TOKENS {
        return None;
    }
    let mut composition = Composition {
        fill: None,
        ink: Vec::new(),
    };
    for token in tokens {
        match kind(token)? {
            TokenKind::Fill => composition.fill = Some(token),
            TokenKind::Ink => composition.ink.push(token),
        }
    }
    Some(composition)
}

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

/// Fills are per cell, not per view: any number of rules may carry one, and a
/// cell two of them claim resolves the same way ink does, to the most specific
/// rule. Only the rule count and each rule's own tokens are checked here.
pub fn validate_rules(rules: &[PaintRule]) -> Result<(), String> {
    if rules.len() > 256 {
        return Err("a view supports at most 256 paint rules".into());
    }
    for rule in rules {
        let lists = rule.role_lists();
        for (index, roles) in lists.iter().enumerate() {
            crate::paint::validate_roles(roles)?;
            if roles.contains('!') && index + 1 != lists.len() {
                return Err("stop marker ! must end the entire rule".into());
            }
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
