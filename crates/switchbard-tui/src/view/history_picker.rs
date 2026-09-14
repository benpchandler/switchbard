//! History keeps the selected arrangement readable before restoring it.
use ratatui::{
    layout::Rect,
    text::Line,
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
        (inner.height / 3).min(4).min(rows.len() as u16)
    } else {
        0
    };
    draw_list(frame, app, &rows, selected, inner, list_height);
    let preview_area = Rect {
        y: inner.y + list_height,
        height: inner.height.saturating_sub(list_height + 1),
        ..inner
    };
    draw_preview(frame, app, picker, &rows[selected], preview_area);
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
    row: &crate::picker::PickOption,
    area: Rect,
) {
    let crate::picker::Payload::HistoryView(record) = &row.payload else {
        return;
    };
    let Ok(mut state) = crate::views::ViewState::try_from_lua(record, app.registry()) else {
        return;
    };
    state.sanitize(app.page, app.registry());
    let details_height = u16::from(area.height >= 5);
    let details = super::history_title::details(&state, app.registry());
    frame.render_widget(
        Paragraph::new(details).style(app.config.theme.style(Surface::Hint)),
        Rect {
            height: details_height,
            ..area
        },
    );
    let body = Rect {
        y: area.y + details_height,
        height: area.height.saturating_sub(details_height),
        ..area
    };
    let scroll =
        super::history_preview::draw(frame, app, &state, body, usize::from(picker.preview_scroll));
    if let Some(live) = app.picker.as_mut() {
        live.preview_scroll = scroll.min(usize::from(u16::MAX)) as u16;
    }
}

fn draw_footer(frame: &mut Frame, app: &App, inner: Rect) {
    let footer = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1,
        ..inner
    };
    let hint = if inner.width >= 62 {
        "↑↓ choose · PgUp/PgDn scroll · Enter restore · Esc back"
    } else {
        "↑↓ pick PgUp/Dn scroll Enter open"
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
