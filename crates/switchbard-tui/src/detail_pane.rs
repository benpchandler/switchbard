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
/// the read-only description row (its full body renders as extra lines
/// directly beneath it), then acceptance criteria, then the read-only
/// relation lists. `crate::app` walks this same list for cursor movement
/// and row dispatch, and `crate::view` walks it for rendering — one list,
/// so the two can never disagree about what row 4 is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRow {
    Title,
    Status,
    Priority,
    Project,
    DueDate,
    Labels,
    /// Read-only for direct editing — its body renders as plain lines
    /// directly beneath this row's own line (not separately navigable);
    /// `sb edit <id> --description` is the edit path, named in the status
    /// message `Enter`/`l` on this row shows.
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
        .title(" Detail ")
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

/// Where the most recently rendered frame put the list/detail split, each
/// visible list row, and each Tasks `FieldRow`'s own first wrapped display
/// line — the one source `App::handle_mouse` consults for "which pane,
/// which row is at this coordinate", so click/scroll routing can never
/// disagree with what was actually drawn. Cleared to a closed-pane shape by
/// `set_areas` whenever no pane is open (Pane::Help, Pane::None); the PR
/// page's list/detail have no row cursor, so `row_starts` is simply left
/// empty there.
#[derive(Default, Clone)]
pub struct Hit {
    pub list_area: Rect,
    pub detail_area: Rect,
    /// `(y, index into App::rows)` for every visible list row, top to
    /// bottom; a click's row is the last entry whose `y` is at or above it.
    pub list_rows: Vec<(u16, usize)>,
    /// Each Tasks `FieldRow`'s own first wrapped display line, in the same
    /// coordinate space `App::detail_scroll` uses.
    pub row_starts: Vec<u16>,
}

impl Hit {
    pub fn set_areas(&mut self, body: Rect, pane_open: bool) {
        if pane_open {
            let [list, detail] = split(body);
            self.list_area = list;
            self.detail_area = detail;
        } else {
            self.list_area = body;
            self.detail_area = Rect::default();
            self.row_starts.clear();
        }
    }

    /// The border-inset rect the detail pane's content actually renders in.
    pub fn detail_inner(&self) -> Rect {
        inset(self.detail_area)
    }

    /// The `FieldRow` index whose wrapped span contains `display_line`
    /// (0-based from the pane's first content line, after `scroll`), or
    /// `None` before the first row or with no rows at all.
    pub fn row_at(&self, display_line: u16) -> Option<usize> {
        self.row_starts
            .iter()
            .rposition(|&start| start <= display_line)
    }

    /// The `App::rows` index at list-relative `y`, or `None` above the
    /// first rendered row.
    pub fn list_row_at(&self, y: u16) -> Option<usize> {
        self.list_rows
            .iter()
            .rev()
            .find(|&&(start, _)| start <= y)
            .map(|&(_, index)| index)
    }
}

fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}
