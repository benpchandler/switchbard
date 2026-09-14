//! History keeps the selected arrangement readable before restoring it.
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{app::App, config::Surface, picker::ValuePicker};

pub(super) fn draw(frame: &mut Frame, app: &mut App, picker: &ValuePicker, area: Rect) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let rows = picker.matching();
    let selected = picker.selected.min(rows.len().saturating_sub(1));
    let inner = draw_header(frame, app, picker, area, &rows, selected);
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(format!("Search: {}\nNo matching history", picker.typed))
                .style(app.config.theme.style(Surface::Hint)),
            inner,
        );
        return;
    }
    let list_height = if inner.height >= 10 {
        (inner.height / 3).min(6)
    } else {
        0
    };
    draw_list(frame, app, &rows, selected, inner, list_height);
    let preview_area = Rect {
        y: inner.y + list_height,
        height: inner.height.saturating_sub(list_height + 1),
        ..inner
    };
    draw_preview(frame, app, picker, &rows[selected].label, preview_area);
    draw_footer(frame, app, inner);
}

fn draw_header(
    frame: &mut Frame,
    app: &App,
    picker: &ValuePicker,
    area: Rect,
    rows: &[crate::picker::PickOption],
    selected: usize,
) -> Rect {
    let age = rows
        .get(selected)
        .and_then(|row| row.label.split(" · ").next())
        .unwrap_or("");
    let title = format!(
        "view history {}/{} · {age} · /{}",
        usize::from(!rows.is_empty()) + selected,
        rows.len(),
        picker.typed
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(app.config.theme.style(Surface::Accent));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    inner
}

fn draw_preview(
    frame: &mut Frame,
    app: &mut App,
    picker: &ValuePicker,
    label: &str,
    preview_area: Rect,
) {
    let wrapped = wrap(label, preview_area.width);
    let max_scroll = wrapped
        .len()
        .saturating_sub(usize::from(preview_area.height));
    let scroll = usize::from(picker.preview_scroll).min(max_scroll);
    if let Some(live) = app.picker.as_mut() {
        live.preview_scroll = scroll.min(usize::from(u16::MAX)) as u16;
    }
    let lines: Vec<Line> = wrapped
        .into_iter()
        .skip(scroll)
        .take(usize::from(preview_area.height))
        .map(Line::from)
        .collect();
    frame.render_widget(
        Paragraph::new(lines).style(app.config.theme.style(Surface::Text)),
        preview_area,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, inner: Rect) {
    let footer = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1,
        ..inner
    };
    let hint = if inner.width >= 62 {
        "↑↓ choose · PgUp/PgDn read · Enter restore · Esc back"
    } else {
        "↑↓ pick PgUp/Dn read Enter open"
    };
    frame.render_widget(
        Paragraph::new(hint).style(app.config.theme.style(Surface::Hint)),
        footer,
    );
}

fn draw_list(
    frame: &mut Frame,
    app: &App,
    rows: &[crate::picker::PickOption],
    selected: usize,
    area: Rect,
    height: u16,
) {
    if height == 0 {
        return;
    }
    let start = selected.saturating_sub(usize::from(height).saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(start)
        .take(usize::from(height))
        .map(|(index, row)| {
            Line::styled(
                row.label.clone(),
                app.config.theme.style(if index == selected {
                    Surface::Selected
                } else {
                    Surface::Hint
                }),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), Rect { height, ..area });
}

/// Grapheme-width wrapping also handles unbroken filters and non-Latin labels.
fn wrap(text: &str, width: u16) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    let span = Span::raw(text);
    for grapheme in span.styled_graphemes(Style::default()) {
        let size = Span::raw(grapheme.symbol).width();
        if grapheme.symbol == "\n" || used + size > usize::from(width) {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if grapheme.symbol != "\n" {
            line.push_str(grapheme.symbol);
            used += size;
        }
    }
    lines.push(line);
    lines
}
