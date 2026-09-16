//! A bounded capsule projection of the authoritative cached checklist count.
use crate::config::{Surface, Theme};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use switchbard_core::ChecklistProgress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressStyle {
    Pill,
    Ascii,
    Icons,
}

impl ProgressStyle {
    pub(crate) fn parse(value: Option<&str>, warnings: &mut Vec<String>) -> Self {
        match value {
            None | Some("pill") => Self::Pill,
            Some("ascii") => Self::Ascii,
            Some("icons") => Self::Icons,
            Some(value) => {
                warnings.push(format!("unknown progress_style '{value}'; using pill"));
                Self::Pill
            }
        }
    }
}

fn fill_units(progress: &ChecklistProgress) -> usize {
    if progress.checked == 0 {
        return 0;
    }
    if progress.checked >= progress.total {
        return 32;
    }
    ((progress.checked as u128 * 32 / progress.total as u128) as usize).clamp(1, 31)
}

pub(crate) fn pill(
    progress: Option<&ChecklistProgress>,
    theme: &Theme,
    row: Style,
    mode: ProgressStyle,
) -> Line<'static> {
    let Some(progress) = progress.filter(|p| p.total > 0) else {
        return Line::from(Span::styled("-", row));
    };
    let units = fill_units(progress);
    let fill = row.patch(theme.style(Surface::ProgressFill));
    let empty = row.patch(theme.style(Surface::ProgressEmpty));
    let shell = theme.style(Surface::ProgressShell).fg;
    let cap_empty = shell.map_or(empty, |color| row.fg(color));
    let caps = if mode == ProgressStyle::Ascii {
        ["(", ")"]
    } else {
        ["\u{e0b6}", "\u{e0b4}"]
    };
    let mut spans = Vec::with_capacity(6);
    spans.push(Span::styled(
        caps[0],
        if units > 0 { fill } else { cap_empty },
    ));
    for index in 0..4 {
        let amount = units.saturating_sub(index * 8).min(8);
        let symbol = if mode == ProgressStyle::Ascii {
            match amount {
                0 => ".",
                8 => "#",
                _ => "+",
            }
        } else {
            ["░", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"][amount]
        };
        let mut style = if amount == 0 { empty } else { fill };
        if let Some(bg) = shell {
            style = style.bg(bg);
        }
        spans.push(Span::styled(symbol, style));
    }
    spans.push(Span::styled(
        caps[1],
        if units == 32 { fill } else { cap_empty },
    ));
    Line::from(spans)
}

/// One-cell ASCII fallback when the complete capsule cannot fit.
pub(crate) fn compact_ascii(progress: Option<&ChecklistProgress>) -> &'static str {
    match progress
        .filter(|progress| progress.total > 0)
        .map(fill_units)
    {
        None => "-",
        Some(0) => "0",
        Some(32) => "#",
        Some(_) => "+",
    }
}

pub(crate) fn icon_state(progress: Option<&ChecklistProgress>) -> &'static str {
    match progress.and_then(|progress| progress.percentage()) {
        None => "unmeasured",
        Some(_) if progress.is_some_and(|p| p.checked == p.total) => "complete",
        Some(0.0) => "empty",
        Some(percent) if percent < 34.0 => "low",
        Some(percent) if percent < 67.0 => "medium",
        Some(_) => "high",
    }
}

pub(crate) fn compact_pill(progress: Option<&ChecklistProgress>) -> &'static str {
    match icon_state(progress) {
        "empty" => "○",
        "low" => "◔",
        "medium" => "◑",
        "high" => "◕",
        "complete" => "●",
        _ => "-",
    }
}
