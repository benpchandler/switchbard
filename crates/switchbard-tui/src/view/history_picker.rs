//! Every visible history entry owns a compact, read-only miniature of its view.
use crate::{
    app::App,
    config::Surface,
    picker::{PickOption, ValuePicker},
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const CARD_HEIGHT: u16 = 7;

pub(super) fn draw(frame: &mut Frame, app: &mut App, picker: &ValuePicker, area: Rect) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let rows = picker.matching();
    let selected = picker.selected.min(rows.len().saturating_sub(1));
    let inner = draw_header(frame, app, picker, area, rows.len(), selected);
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(format!("Search: {}\nNo matching history", picker.typed))
                .style(app.config.theme.style(Surface::Hint)),
            inner,
        );
        return;
    }
    let available = inner.height.saturating_sub(1);
    let height = CARD_HEIGHT.min(available);
    if height == 0 {
        return;
    }
    let capacity = usize::from((available / height).max(1));
    let start = selected.saturating_sub(capacity.saturating_sub(1));
    for (offset, row) in rows.iter().skip(start).take(capacity).enumerate() {
        let card = Rect {
            y: inner.y + offset as u16 * height,
            height,
            ..inner
        };
        draw_card(frame, app, row, card, start + offset == selected);
    }
    draw_footer(frame, app, inner);
}

fn draw_header(
    frame: &mut Frame,
    app: &App,
    picker: &ValuePicker,
    area: Rect,
    count: usize,
    selected: usize,
) -> Rect {
    let title = format!(
        "view history {}/{} · /{}",
        usize::from(count > 0) + selected,
        count,
        picker.typed
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new("").style(app.config.theme.canvas_style()),
        area,
    );
    if area.height < 9 {
        frame.render_widget(
            Paragraph::new(title).style(app.config.theme.style(Surface::Accent)),
            Rect { height: 1, ..area },
        );
        return Rect {
            y: area.y + 1,
            height: area.height - 1,
            ..area
        };
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::styled(title, app.config.theme.style(Surface::Text)))
        .border_style(app.config.theme.style(Surface::Border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn draw_card(frame: &mut Frame, app: &App, row: &PickOption, area: Rect, selected: bool) {
    let crate::picker::Payload::HistoryView(record) = &row.payload else {
        return;
    };
    let Ok(mut state) = crate::views::ViewState::try_from_lua(record, app.registry()) else {
        return;
    };
    state.sanitize(app.page, app.registry());
    let border_style = app.config.theme.style(if selected {
        Surface::Accent
    } else {
        Surface::Border
    });
    let style = app.config.theme.style(if selected {
        Surface::Accent
    } else {
        Surface::Text
    });
    let (age, title) = row.label.split_once(" · ").unwrap_or(("", &row.label));
    let age_width = Line::from(age).width().min(usize::from(area.width)) as u16;
    let title = clipped_title(
        &format!("{} {title}", if selected { "▸" } else { " " }),
        area.width.saturating_sub(age_width + 4),
    );
    let body = if area.height < 6 {
        let [heading, age_area] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(age_width)])
                .areas(Rect { height: 1, ..area });
        frame.render_widget(Paragraph::new(title).style(style), heading);
        frame.render_widget(Paragraph::new(age).style(style), age_area);
        Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        }
    } else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::styled(title, style).left_aligned())
            .title(Line::styled(format!(" {age} "), style).right_aligned())
            .title_bottom(Line::styled(
                super::history_title::details(&state, app.registry()),
                app.config.theme.style(Surface::Hint),
            ))
            .border_style(border_style);
        let body = block.inner(area);
        frame.render_widget(block, area);
        body
    };
    super::history_preview::draw(frame, app, &state, body, 0);
}

fn draw_footer(frame: &mut Frame, app: &App, inner: Rect) {
    let footer = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1,
        ..inner
    };
    let hint = if inner.width >= 62 {
        "↑↓ choose card · PgUp/PgDn skip cards · Enter restore · Esc back"
    } else {
        "↑↓ card PgUp/Dn skip Enter restore"
    };
    frame.render_widget(
        Paragraph::new(hint).style(app.config.theme.style(Surface::Hint)),
        footer,
    );
}

fn clipped_title(text: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    if Span::raw(text).width() <= usize::from(width) {
        return text.to_owned();
    }
    let mut result = String::new();
    let mut used = 0;
    let span = Span::raw(text);
    for grapheme in span.styled_graphemes(Style::default()) {
        let size = Span::raw(grapheme.symbol).width();
        if used + size >= usize::from(width) {
            break;
        }
        result.push_str(grapheme.symbol);
        used += size;
    }
    result.push('…');
    result
}
