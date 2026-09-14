//! Shared task and PR detail presentation.
use crate::config::{Surface, Theme};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use switchbard_core::BacklogTask;

/// One navigable row of the editable task detail pane (TASK-222), in the
/// fixed order the pane always renders them: structured fields first, then
/// the read-only description hint, then acceptance criteria, then the
/// read-only relation lists. `crate::app` walks this same list for cursor
/// movement and row dispatch, and `crate::view` walks it for rendering — one
/// list, so the two can never disagree about what row 4 is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRow {
    Title,
    Status,
    Priority,
    Project,
    DueDate,
    Labels,
    /// Read-only: multiline prose editing is out of TASK-222's scope.
    Description,
    /// Index into `BacklogTask::acceptance_criteria`.
    Acceptance(usize),
    /// Index into the blocking-dependency list for this task.
    BlockedBy(usize),
    /// Index into the dependents list for this task.
    Blocks(usize),
}

impl FieldRow {
    /// Whether Enter/Space on this row can ever mutate the task — governs
    /// both the read-only-task refusal and which rows the read-only
    /// relation/description sections are excluded from.
    pub fn editable(self) -> bool {
        matches!(
            self,
            Self::Title
                | Self::Status
                | Self::Priority
                | Self::Project
                | Self::DueDate
                | Self::Labels
                | Self::Acceptance(_)
        )
    }
}

/// The row list for one task: fixed fields, then one row per acceptance
/// item, then one row per blocking dependency, then one row per dependent.
/// `blocked_by`/`blocks` are counts rather than borrowed slices so this stays
/// a pure function of sizes the caller already has to hand
/// (`TaskRelations::blocked_by`/`blocks` are keyed maps of vecs).
pub fn field_rows(task: &BacklogTask, blocked_by: usize, blocks: usize) -> Vec<FieldRow> {
    let mut rows = vec![
        FieldRow::Title,
        FieldRow::Status,
        FieldRow::Priority,
        FieldRow::Project,
        FieldRow::DueDate,
        FieldRow::Labels,
        FieldRow::Description,
    ];
    rows.extend((0..task.acceptance_criteria.len()).map(FieldRow::Acceptance));
    rows.extend((0..blocked_by).map(FieldRow::BlockedBy));
    rows.extend((0..blocks).map(FieldRow::Blocks));
    rows
}

pub fn split(area: Rect) -> [Rect; 2] {
    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area)
}

pub fn title(text: String) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

pub fn metadata(text: String, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(text, theme.style(Surface::Hint)))
}

pub fn section(text: &'static str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(text, theme.style(Surface::Accent)))
}

pub fn draw(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll: u16,
) -> u16 {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style(Surface::Border));
    let inner = block.inner(area);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let scroll = if scroll == 0 {
        0
    } else {
        let max_scroll = paragraph
            .line_count(inner.width)
            .saturating_sub(inner.height as usize)
            .min(u16::MAX as usize) as u16;
        scroll.min(max_scroll)
    };
    frame.render_widget(paragraph.block(block).scroll((scroll, 0)), area);
    scroll
}
