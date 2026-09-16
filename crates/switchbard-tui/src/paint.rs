//! Paint rules in a hierarchy. The first rule is the base and colors whole rows;
//! every rule below it colors only its own scope, so lower means more specific.
//! Reordering the list flips which paint dominates. Saved with the view.

use std::collections::HashSet;
use std::str::FromStr;

use crate::config::Theme;
use ratatui::style::{Color, Style};
use switchbard_core::{BacklogTask, GoalDef};

use crate::columns::{Column, ColumnRegistry};
use crate::tasks::Filter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintRule {
    /// One color per value of a categorical column. As the base it paints whole
    /// rows; lower down it paints that column's cells.
    ByColumn {
        column: Column,
        colors: Vec<(String, String)>,
    },
    /// Every row the filter matches, whole row, wherever it sits.
    Rows {
        filter: String,
        color: String,
    },
    /// A whole column, one color, wherever it sits.
    Column {
        column: Column,
        color: String,
    },
    Heading {
        value: String,
        color: String,
    },
    Header {
        color: String,
    },
    Title {
        color: String,
    },
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
    pub fn to_text(&self, registry: &ColumnRegistry) -> String {
        match self {
            PaintRule::ByColumn { column, colors } => format!(
                "by:{}={}",
                column.save_name(registry),
                colors
                    .iter()
                    .map(|(value, color)| format!("{value}:{color}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            PaintRule::Rows { filter, color } => format!("rows:{filter}={color}"),
            PaintRule::Heading { value, color } => format!("heading:{value}={color}"),
            PaintRule::Header { color } => format!("header={color}"),
            PaintRule::Title { color } => format!("title={color}"),
            PaintRule::Column { column, color } => {
                format!("column:{}={color}", column.save_name(registry))
            }
        }
    }

    pub fn parse(text: &str, registry: &ColumnRegistry) -> Option<PaintRule> {
        Self::try_parse(text, registry).ok()
    }

    pub fn try_parse(text: &str, registry: &ColumnRegistry) -> Result<PaintRule, String> {
        let (target, rhs) = text.trim().split_once('=').ok_or("expected target=roles")?;
        if let Some(column) = target.strip_prefix("by:") {
            let colors = parse_value_roles(rhs)?;
            return Ok(Self::ByColumn {
                column: registry
                    .parse(column.trim())
                    .ok_or("unknown paint column")?,
                colors,
            });
        }
        validate_roles(rhs.trim())?;
        let color = rhs.trim().to_string();
        if let Some(filter) = target.strip_prefix("rows:") {
            Ok(Self::Rows {
                filter: filter.trim().to_string(),
                color,
            })
        } else if let Some(column) = target.strip_prefix("column:") {
            Ok(Self::Column {
                column: registry
                    .parse(column.trim())
                    .ok_or("unknown paint column")?,
                color,
            })
        } else if let Some(value) = target.strip_prefix("heading:") {
            Ok(Self::Heading {
                value: value.trim().to_string(),
                color,
            })
        } else {
            match target.trim() {
                "header" => Ok(Self::Header { color }),
                "title" => Ok(Self::Title { color }),
                _ => Err("unknown paint target".into()),
            }
        }
    }

    pub fn role_lists(&self) -> Vec<&str> {
        match self {
            Self::ByColumn { colors, .. } => {
                colors.iter().map(|(_, roles)| roles.as_str()).collect()
            }
            Self::Rows { color, .. }
            | Self::Column { color, .. }
            | Self::Heading { color, .. }
            | Self::Header { color }
            | Self::Title { color } => vec![color],
        }
    }

    pub fn stops(&self) -> bool {
        let roles = match self {
            Self::ByColumn { colors, .. } => colors.last().map(|(_, roles)| roles.as_str()),
            Self::Rows { color, .. }
            | Self::Column { color, .. }
            | Self::Heading { color, .. }
            | Self::Header { color }
            | Self::Title { color } => Some(color.as_str()),
        };
        roles.is_some_and(|roles| roles.trim_end().ends_with('!'))
    }

    /// How the rule reads in the hierarchy list.
    pub fn label(&self, registry: &ColumnRegistry) -> String {
        match self {
            PaintRule::ByColumn { column, colors } => {
                format!("by {} ({} values)", column.name(registry), colors.len())
            }
            PaintRule::Rows { filter, color } => format!("rows {filter} → {color}"),
            PaintRule::Heading { value, color } => format!("heading {value} → {color}"),
            PaintRule::Header { color } => format!("header → {color}"),
            PaintRule::Title { color } => format!("title → {color}"),
            PaintRule::Column { column, color } => {
                format!("column {} → {color}", column.name(registry))
            }
        }
    }

    /// A representative color for the list: the first value's, or the rule's own.
    pub fn swatch(&self, palette: &[String]) -> Option<Color> {
        self.role_lists().first().and_then(|roles| {
            roles
                .trim_end_matches('!')
                .split('+')
                .find_map(|token| resolve_color(token.trim(), palette))
        })
    }
}

/// The color for one cell: the lowest (most specific) rule that claims it wins;
/// the top rule is the base and claims whole rows.
pub fn cell_color(
    rules: &[PaintRule],
    palette: &[String],
    registry: &ColumnRegistry,
    task: &BacklogTask,
    column: Column,
    goals: &[GoalDef],
    blocked: &HashSet<String>,
) -> Option<Color> {
    cell_color_with(
        rules,
        palette,
        column,
        registry,
        |column| {
            let values = column.values(registry, task, goals, blocked);
            if column.is_date() {
                values
            } else {
                values.into_iter().take(1).collect()
            }
        },
        |filter| filter.matches(registry, task, goals, blocked),
    )
}

pub fn cell_color_with(
    rules: &[PaintRule],
    palette: &[String],
    column: Column,
    registry: &ColumnRegistry,
    values: impl Fn(Column) -> Vec<String>,
    matches: impl Fn(&Filter) -> bool,
) -> Option<Color> {
    crate::paint_eval::cell_token(rules, column, registry, values, matches)
        .and_then(|token| resolve_color(token, palette))
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

/// Resolve a one-based palette slot against the current palette. Slot identity
/// survives palette edits; shorter palettes cycle and an empty palette uses the
/// built-in fallback. Literal named colors and legacy hex are never rewritten.
pub fn resolve_color(token: &str, palette: &[String]) -> Option<Color> {
    if let Some(slot) = token.strip_prefix('p') {
        if !slot.is_empty() && slot.bytes().all(|byte| byte.is_ascii_digit()) {
            return palette_color(slot.parse::<usize>().ok()?, palette);
        }
    }
    Color::from_str(token).ok()
}

/// The color in one-based palette slot `index`, the same cycling `p<n>` uses.
/// Taking the number rather than its spelling keeps the callers that already
/// have one, the highlight slots among them, off the allocation path.
pub fn palette_color(index: usize, palette: &[String]) -> Option<Color> {
    let index = index.checked_sub(1)?;
    let color = if palette.is_empty() {
        AUTO_PALETTE[index % AUTO_PALETTE.len()]
    } else {
        palette[index % palette.len()].as_str()
    };
    Color::from_str(color).ok()
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
        let stop = colors.last().is_some_and(|(_, roles)| roles.ends_with('!'))
            || color.is_some_and(|roles| roles.ends_with('!'));
        if let Some((_, roles)) = colors.last_mut() {
            *roles = roles.trim_end_matches('!').to_string();
        }
        colors.retain(|(known, _)| *known != key);
        if let Some(color) = color {
            colors.push((key, color.trim_end_matches('!').to_string()));
        }
        if stop {
            if let Some((_, roles)) = colors.last_mut() {
                roles.push('!');
            }
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
        (PaintRule::Heading { value: a, .. }, PaintRule::Heading { value: b, .. }) => a == b,
        (PaintRule::Header { .. }, PaintRule::Header { .. })
        | (PaintRule::Title { .. }, PaintRule::Title { .. }) => true,
        _ => false,
    };
    let color = match &rule {
        PaintRule::Rows { color, .. }
        | PaintRule::Column { color, .. }
        | PaintRule::Heading { color, .. }
        | PaintRule::Header { color }
        | PaintRule::Title { color } => color.clone(),
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

pub fn rules_text(rules: &[PaintRule], registry: &ColumnRegistry) -> String {
    rules
        .iter()
        .map(|rule| rule.to_text(registry))
        .collect::<Vec<_>>()
        .join(";")
}

pub fn parse_rules(text: &str, registry: &ColumnRegistry) -> Vec<PaintRule> {
    try_parse_rules(text, registry).unwrap_or_default()
}

pub fn try_parse_rules(text: &str, registry: &ColumnRegistry) -> Result<Vec<PaintRule>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    if text.len() > 65_536 {
        return Err("paint rules exceed 64 KiB".into());
    }
    let rules = text
        .split(';')
        .filter(|rule| !rule.trim().is_empty())
        .enumerate()
        .map(|(index, text)| {
            PaintRule::try_parse(text, registry)
                .map_err(|error| format!("paint rule {}: {error}", index + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_rules(&rules)?;
    Ok(rules)
}

fn parse_value_roles(rhs: &str) -> Result<Vec<(String, String)>, String> {
    if rhs.trim().is_empty() {
        return Ok(Vec::new());
    }
    let pairs: Vec<_> = rhs.split(',').collect();
    pairs
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            let (value, roles) = pair.rsplit_once(':').ok_or("expected value:roles")?;
            if value.trim().is_empty() {
                return Err("paint value cannot be empty".into());
            }
            validate_roles(roles.trim())?;
            if roles.contains('!') && index + 1 != pairs.len() {
                return Err("stop marker ! must end the entire rule".into());
            }
            Ok((value.trim().to_string(), roles.trim().to_string()))
        })
        .collect()
}

/// Every token in a rule must name an emphasis role, a highlight slot or a
/// color. Which of them fills the cell and which writes on it is the theme's
/// answer (`paint_eval::compose`); saved rules stay valid across theme changes,
/// so this check is deliberately theme-independent.
pub fn validate_roles(roles: &str) -> Result<(), String> {
    let roles = roles.trim().strip_suffix('!').unwrap_or(roles.trim());
    let tokens: Vec<&str> = roles.split('+').map(str::trim).collect();
    if tokens.len() > crate::paint_eval::MAX_ROLE_TOKENS {
        return Err(format!(
            "a paint rule composes at most {} roles",
            crate::paint_eval::MAX_ROLE_TOKENS
        ));
    }
    for token in tokens {
        let known = crate::config::EMPHASIS_ROLES.contains(&token)
            || crate::highlight::slot_index(token).is_some()
            || resolve_color(token, &[]).is_some();
        if !known {
            return Err(format!("unknown emphasis role or color: {token}"));
        }
    }
    Ok(())
}

pub fn validate_rules(rules: &[PaintRule]) -> Result<(), String> {
    crate::paint_eval::validate_rules(rules)
}

/// One rule's roles as a terminal style: the theme composes fill and ink, this
/// layer only decides what to do when a token names nothing it knows, which
/// only an externally edited saved view can produce.
pub fn resolve_style(roles: &str, theme: &Theme, palette: &[String]) -> Style {
    theme.emphasis_style(roles, palette).unwrap_or_default()
}

pub fn cell_style_with(
    rules: &[PaintRule],
    palette: &[String],
    theme: &Theme,
    column: Column,
    registry: &ColumnRegistry,
    values: impl Fn(Column) -> Vec<String>,
    matches: impl Fn(&Filter) -> bool,
) -> Style {
    let mut style = Style::default();
    crate::paint_eval::visit_cell_tokens(rules, column, registry, values, matches, |token| {
        style = resolve_style(token, theme, palette).patch(style);
    });
    style
}

pub fn cell_style(
    rules: &[PaintRule],
    config: &crate::config::Config,
    registry: &ColumnRegistry,
    task: &BacklogTask,
    column: Column,
    goals: &[GoalDef],
    blocked: &HashSet<String>,
) -> Style {
    cell_style_with(
        rules,
        &config.palette,
        &config.theme,
        column,
        registry,
        |target| {
            let values = target.values(registry, task, goals, blocked);
            if target.is_date() {
                values
            } else {
                values.into_iter().take(1).collect()
            }
        },
        |filter| filter.matches(registry, task, goals, blocked),
    )
}

pub enum PaintScope<'a> {
    Heading(&'a str),
    Header,
    Title,
}

pub fn scoped_style(
    rules: &[PaintRule],
    theme: &Theme,
    palette: &[String],
    scope: PaintScope<'_>,
) -> Style {
    let mut style = Style::default();
    for rule in rules.iter().rev() {
        let roles = match (rule, &scope) {
            (PaintRule::Heading { value, color }, PaintScope::Heading(heading))
                if value == "*" || Filter::loose_key(value) == Filter::loose_key(heading) =>
            {
                Some(color)
            }
            (PaintRule::Header { color }, PaintScope::Header)
            | (PaintRule::Title { color }, PaintScope::Title) => Some(color),
            _ => None,
        };
        if let Some(roles) = roles {
            style = resolve_style(roles, theme, palette).patch(style);
            if rule.stops() {
                break;
            }
        }
    }
    style
}
