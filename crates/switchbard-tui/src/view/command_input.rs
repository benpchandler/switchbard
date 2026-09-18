//! Bounded, cursor-following command drafts. Rendering never changes the input.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::config::Surface;

const MAX_DRAFT_ROWS: usize = 4;
const MAX_RENDER_BYTES: usize = 16 * 1024;
const MAX_COMPLETION_INPUT_BYTES: usize = 64;

pub(super) fn height(app: &App, width: u16, available: u16) -> u16 {
    if width == 0 || available == 0 {
        return 0;
    }
    let rows = if width == 1 {
        1
    } else {
        draft(app).line_count(width - 1).clamp(1, MAX_DRAFT_ROWS) as u16
    };
    (rows + u16::from(!app.status.is_empty())).min(available)
}

pub(super) fn draw(frame: &mut Frame, app: &App, area: Rect) {
    if area.is_empty() {
        return;
    }
    let status_rows = u16::from(!app.status.is_empty() && area.height > 1);
    let draft_area = Rect {
        height: area.height - status_rows,
        ..area
    };
    draw_draft(frame, app, draft_area);
    if status_rows > 0 {
        frame.render_widget(
            Paragraph::new(app.status.as_str()).style(app.config.theme.style(Surface::Status)),
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
        );
    }
}

fn draw_draft(frame: &mut Frame, app: &App, area: Rect) {
    let accent = app.config.theme.style(Surface::Accent);
    if area.width == 1 {
        frame.render_widget(Paragraph::new("▏").style(accent), area);
        return;
    }
    let content = Rect::new(area.x + 1, area.y, area.width - 1, area.height);
    let paragraph = draft(app);
    let rows = paragraph.line_count(content.width);
    let scroll = rows.saturating_sub(usize::from(content.height));
    // The bounded source has fewer than u16::MAX possible rendered rows.
    frame.render_widget(paragraph.scroll((scroll as u16, 0)), content);
    frame.render_widget(
        Paragraph::new(":").style(accent),
        Rect::new(area.x, area.y, 1, 1),
    );
    if rows == 1 {
        draw_completions(frame, app, content);
    }
}

fn draft(app: &App) -> Paragraph<'_> {
    let text = render_tail(&app.input);
    let mut lines: Vec<Line<'_>> = text.split('\n').map(Line::raw).collect();
    if let Some(last) = lines.last_mut() {
        last.spans
            .push(Span::styled("▏", app.config.theme.style(Surface::Accent)));
    }
    Paragraph::new(lines).wrap(Wrap { trim: false })
}

fn render_tail(input: &str) -> &str {
    if input.len() <= MAX_RENDER_BYTES {
        return input;
    }
    let approximate = input.len() - MAX_RENDER_BYTES;
    let start = (approximate..=approximate + 3)
        .find(|offset| input.is_char_boundary(*offset))
        .unwrap_or(input.len());
    let tail = &input[start..];
    // Omit the first possibly incomplete grapheme at the bounded tail boundary.
    let span = Span::raw(tail);
    let first = span.styled_graphemes(Style::default()).next();
    let skip = first.map_or(0, |grapheme| {
        grapheme.symbol.as_ptr() as usize - tail.as_ptr() as usize + grapheme.symbol.len()
    });
    &tail[skip..]
}

fn draw_completions(frame: &mut Frame, app: &App, area: Rect) {
    if app.input.len() > MAX_COMPLETION_INPUT_BYTES || app.input.contains('\n') {
        return;
    }
    let offset = Span::raw(app.input.as_str()).width() + 4;
    if offset >= usize::from(area.width) {
        return;
    }
    let completions = app.command_completions().join("  ");
    frame.render_widget(
        Paragraph::new(completions).style(app.config.theme.style(Surface::Hint)),
        Rect::new(
            area.x + offset as u16,
            area.y,
            area.width - offset as u16,
            1,
        ),
    );
}
