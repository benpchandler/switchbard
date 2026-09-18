//! Full-width experiment entries: a title followed by what the owner should notice.
use crate::{
    app::App,
    config::{Surface, Theme},
    experiments::catalog,
    picker::{Payload, PickOption, ValuePicker},
};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

pub(super) fn draw(frame: &mut Frame, app: &App, picker: &ValuePicker, body: Rect) {
    let rows = picker.matching();
    let selected = picker.selected.min(rows.len().saturating_sub(1));
    let area = Rect {
        height: body.height.min(
            rows.len()
                .saturating_mul(2)
                .saturating_add(2)
                .clamp(4, u16::MAX as usize) as u16,
        ),
        ..body
    };
    let theme = &app.config.theme;
    let block = picker_block(theme, picker, selected, rows.len());
    frame.render_widget(Clear, area);
    if area.height < 4 || area.width < 8 {
        frame.render_widget(
            Paragraph::new("Enlarge terminal · Esc closes").block(block),
            area,
        );
        return;
    }
    if rows.is_empty() {
        frame.render_widget(Paragraph::new("No matching experiments").block(block), area);
        return;
    }
    let keys = picker.row_keys();
    let items = rows
        .iter()
        .enumerate()
        .map(|(index, option)| entry(theme, option, &keys[index], index == selected))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

fn picker_block(
    theme: &Theme,
    picker: &ValuePicker,
    selected: usize,
    count: usize,
) -> Block<'static> {
    let mut title = format!(
        " Experiments · {}/{count}",
        if count == 0 { 0 } else { selected + 1 }
    );
    if !picker.typed.is_empty() {
        title.push_str(&format!(" · /{}", picker.typed));
    }
    if !picker.number.is_empty() {
        title.push_str(&format!(" · key {}", picker.number));
    }
    Block::default()
        .borders(Borders::ALL)
        .style(theme.canvas_style())
        .border_style(theme.style(Surface::Accent))
        .title(format!("{title} "))
        .title_bottom(" ↑↓ select · number/Enter open · Esc close ")
}

fn entry(theme: &Theme, option: &PickOption, key: &str, selected: bool) -> ListItem<'static> {
    let style = theme.style(if selected {
        Surface::Selected
    } else {
        Surface::Text
    });
    let notice_style = if selected {
        style
    } else {
        theme.style(Surface::Hint)
    };
    ListItem::new(vec![
        Line::styled(
            format!("{key} {}", option.label),
            style.add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            format!("  Notice: {}", notice(&option.payload)),
            notice_style,
        ),
    ])
    .style(style)
}

fn notice(payload: &Payload) -> &'static str {
    match payload {
        Payload::Experiment(id) => catalog()
            .iter()
            .find(|spec| spec.id == id)
            .map_or("This experiment is unavailable in this build.", |spec| {
                spec.description
            }),
        Payload::Update => "Load the installed build; your saved experiment choices carry forward.",
        _ => "Open this option for details.",
    }
}
