//! Shared task and PR detail presentation.
use crate::config::{Surface, Theme};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

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
