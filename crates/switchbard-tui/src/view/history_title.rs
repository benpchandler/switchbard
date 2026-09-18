//! Short, truthful titles. The miniature carries the arrangement's visual detail.
use crate::{
    columns::{Column, ColumnRegistry},
    page::Page,
    paint::PaintRule,
    views::ViewState,
};

pub(crate) fn title(state: &ViewState, page: Page, registry: &ColumnRegistry) -> String {
    if !state.name.trim().is_empty() {
        return state.name.clone();
    }
    let subject = if page == Page::PullRequests {
        "Pull requests"
    } else {
        "Tasks"
    };
    let filter = state.filter.trim();
    let mut title = if filter.is_empty() {
        subject.to_string()
    } else if !filter.contains(':') && filter.split_whitespace().count() == 1 {
        format!("{subject} matching “{filter}”")
    } else if filter == "status:!done" && page == Page::Tasks {
        "Tasks excluding Done".into()
    } else if matches!(filter, "status:open" | "lifecycle:open") && page == Page::PullRequests {
        "Open pull requests".into()
    } else if matches!(filter, "status:closed" | "lifecycle:closed") && page == Page::PullRequests {
        "Closed pull requests".into()
    } else if matches!(filter, "status:merged" | "lifecycle:merged") && page == Page::PullRequests {
        "Merged pull requests".into()
    } else {
        format!("Filtered {}", subject.to_lowercase())
    };
    if state.group.is_auto() {
        title.push_str(" · automatically outlined");
    } else if !state.group.is_flat() {
        title.push_str(&format!(
            " · outlined by {}",
            state
                .group
                .levels()
                .iter()
                .map(|column| display_column(*column, registry))
                .collect::<Vec<_>>()
                .join(" then ")
        ));
    } else if let Some(rule) = state.paint.first() {
        title.push_str(&match rule {
            PaintRule::ByColumn { column, .. } => {
                format!(" · colored by {}", display_column(*column, registry))
            }
            PaintRule::Rows { .. } => " · highlighted rows".into(),
            PaintRule::Heading { .. } => " · styled outline headings".into(),
            PaintRule::Header { .. } => " · styled headers".into(),
            PaintRule::Title { .. } => " · styled navigation".into(),
            PaintRule::Column { column, .. } => {
                format!(" · highlighted {}", display_column(*column, registry))
            }
        });
    }
    title
}

pub(super) fn details(state: &ViewState, registry: &ColumnRegistry) -> String {
    let mut parts = Vec::new();
    if !state.sort.is_empty() {
        parts.push(format!(
            "Sorted by {}",
            state
                .sort
                .iter()
                .map(|sort| display_column(sort.column, registry))
                .collect::<Vec<_>>()
                .join(", then ")
        ));
    }
    if !state.paint.is_empty() {
        parts.push("Saved colors".into());
    }
    parts.push(format!("{} columns", state.columns.len()));
    parts.join(" · ")
}

fn display_column(column: Column, registry: &ColumnRegistry) -> String {
    let name = match column {
        Column::Status => "Status",
        Column::Priority => "Priority",
        Column::Project => "Project",
        Column::Goal => "Goal",
        Column::Lifecycle => "State",
        other => other.header(registry),
    };
    let mut letters = name.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + letters.as_str())
        .unwrap_or_default()
}
