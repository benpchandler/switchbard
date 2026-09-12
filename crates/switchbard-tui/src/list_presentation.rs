//! Bounded terminal list layout. Features supply rows, styles and selection;
//! this module never reads application state or performs feature actions.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// The feature applies this output to its own scroll state after layout.
#[derive(Clone, Copy)]
pub(crate) struct ListViewport {
    pub scroll: usize,
    pub slots: usize,
}

impl ListViewport {
    /// Row indices remain navigation identities; heights consume terminal lines.
    /// Only a viewport-sized suffix before selection is measured.
    pub fn variable(
        scroll: usize,
        selected: usize,
        count: usize,
        slots: usize,
        heading: bool,
        height: impl Fn(usize) -> usize,
    ) -> Self {
        let selected = selected.min(count.saturating_sub(1));
        let mut scroll = scroll.min(selected).max(selected.saturating_sub(slots));
        let mut used: usize = (scroll..=selected)
            .map(|row| height(row).min(slots).max(1))
            .sum();
        let before_selected = scroll..selected;
        for row in before_selected {
            if used <= slots {
                break;
            }
            used = used.saturating_sub(height(row).min(slots).max(1));
            scroll = row + 1;
        }
        if heading && scroll == selected && scroll > 0 && used + height(scroll - 1) <= slots {
            scroll -= 1;
        }
        Self { scroll, slots }
    }

    pub fn new(scroll: usize, selected: usize, count: usize, slots: usize, heading: bool) -> Self {
        let selected = selected.min(count.saturating_sub(1));
        let mut scroll = scroll.min(selected);
        if slots > 0 && selected >= scroll.saturating_add(slots) {
            scroll = selected.saturating_add(1).saturating_sub(slots);
        }
        if slots > 1 && heading && scroll == selected {
            scroll = scroll.saturating_sub(1);
        }
        Self { scroll, slots }
    }
}

pub(crate) fn cells(area: Rect, widths: &[Constraint]) -> std::rc::Rc<[Rect]> {
    Layout::horizontal(widths.iter().copied())
        .spacing(1)
        .split(area)
}

pub(crate) fn header(
    frame: &mut Frame,
    area: Rect,
    cells: &[Rect],
    labels: &[String],
    style: Style,
) {
    frame.render_widget(Paragraph::new("").style(style), area);
    for (label, cell) in labels.iter().zip(cells).take(area.width as usize) {
        frame.render_widget(Paragraph::new(label.as_str()).style(style), *cell);
    }
}

/// Explicit output for confirmation gates: true only when every line fits.
/// Ordinary pickers scroll to selection; confirmations never hide authority text.
pub(crate) fn picker(
    frame: &mut Frame,
    area: Rect,
    block: Block<'_>,
    lines: Vec<Line<'_>>,
    selected: usize,
    require_all: bool,
) -> bool {
    frame.render_widget(Clear, area);
    if require_all {
        return confirmation(frame, area, block, lines);
    }
    let slots = area.height.saturating_sub(2).max(1) as usize;
    let offset = selected.saturating_sub(slots.saturating_sub(1));
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((offset.min(u16::MAX as usize) as u16, 0))
            .block(block),
        area,
    );
    false
}

fn confirmation(frame: &mut Frame, area: Rect, block: Block<'_>, lines: Vec<Line<'_>>) -> bool {
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let fits = area.width >= 40
        && paragraph
            .line_count(area.width.saturating_sub(2))
            .saturating_add(2)
            <= area.height as usize;
    if fits {
        frame.render_widget(paragraph.block(block), area);
    } else {
        frame.render_widget(
            Paragraph::new("Enlarge terminal to confirm merge. Esc cancels.")
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
    }
    fits
}
