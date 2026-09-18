//! Full-width experiment entries: a title followed by what the owner should notice.
use crate::{
    app::App,
    config::{Surface, Theme},
    experiments::{catalog, ExperimentDecision, ExperimentReview},
    picker::{ExperimentButtonHit, Payload, PickOption, ValuePicker},
};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

const BUTTONS_WIDTH: u16 = 14;

pub(super) fn draw(frame: &mut Frame, app: &mut App, picker: &ValuePicker, body: Rect) {
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
    let inner = block.inner(area);
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
        .map(|(index, option)| entry(app, option, &keys[index], index == selected, inner.width))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
    app.experiment_hits = button_hits(app, &rows, inner, state.offset());
}

fn picker_block(
    theme: &Theme,
    picker: &ValuePicker,
    selected: usize,
    count: usize,
) -> Block<'static> {
    let mut title = format!(
        " Experiments · {}/{count} · t Try pins instructions and opens the right page",
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
        .title_bottom(" t Try + pin · space on/off · a keep · r remove · Enter more · Esc close ")
}

fn entry(
    app: &App,
    option: &PickOption,
    key: &str,
    selected: bool,
    width: u16,
) -> ListItem<'static> {
    let theme = &app.config.theme;
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
    let mut title = Line::styled(
        format!("{key} {}", option.label),
        style.add_modifier(Modifier::BOLD),
    );
    if let Payload::Experiment(id) = &option.payload {
        if width > BUTTONS_WIDTH {
            let available = width - BUTTONS_WIDTH;
            let text = fitted_title(&format!("{key} {}", option.label), available as usize);
            title = Line::styled(text, style.add_modifier(Modifier::BOLD));
            let state = app.experiments.state(id);
            let enabled_color = semantic_color(app, if state.enabled { "cyan" } else { "gray" });
            title.spans.extend(
                crate::progress::word_pill(
                    if state.enabled { "On" } else { "Off" },
                    3,
                    enabled_color,
                    style,
                    app.config.progress_style,
                )
                .spans,
            );
            title.spans.push(Span::styled(" ", style));
            let (label, color) = match state.review {
                ExperimentReview::Unreviewed => ("Keep", "green"),
                ExperimentReview::Kept => ("Kept", "green"),
                ExperimentReview::RemovalRequested => ("Remove", "red"),
            };
            title.spans.extend(
                crate::progress::word_pill(
                    label,
                    6,
                    semantic_color(app, color),
                    style,
                    app.config.progress_style,
                )
                .spans,
            );
        }
    }
    ListItem::new(vec![
        title,
        Line::styled(format!("  Try: {}", notice(&option.payload)), notice_style),
    ])
    .style(style)
}

pub(super) fn semantic_color(app: &App, name: &str) -> Color {
    app.config
        .theme
        .emphasis_style(name, &app.config.palette)
        .and_then(|style| style.bg.or(style.fg))
        .unwrap_or(Color::Gray)
}

fn fitted_title(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let mut bytes = [0; 4];
        let encoded: &str = ch.encode_utf8(&mut bytes);
        let cells = Span::raw(encoded).width();
        if used + cells > width.saturating_sub(1) {
            break;
        }
        result.push(ch);
        used += cells;
    }
    result.push_str(&" ".repeat(width.saturating_sub(used)));
    result
}

fn button_hits(
    app: &App,
    rows: &[PickOption],
    inner: Rect,
    offset: usize,
) -> Vec<ExperimentButtonHit> {
    if inner.width <= BUTTONS_WIDTH {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (index, option) in rows
        .iter()
        .skip(offset)
        .take(usize::from(inner.height / 2))
        .enumerate()
    {
        let Payload::Experiment(id) = &option.payload else {
            continue;
        };
        let state = app.experiments.state(id);
        let area = Rect {
            x: inner.right() - BUTTONS_WIDTH,
            y: inner.y + index as u16 * 2,
            width: 5,
            height: 1,
        };
        hits.push(ExperimentButtonHit {
            area,
            id: id.clone(),
            decision: Some(if state.enabled {
                ExperimentDecision::Disable
            } else {
                ExperimentDecision::Enable
            }),
        });
        hits.push(ExperimentButtonHit {
            area: Rect {
                x: area.x + 6,
                width: 8,
                ..area
            },
            id: id.clone(),
            decision: if state.review == ExperimentReview::Unreviewed {
                Some(ExperimentDecision::Keep)
            } else {
                None
            },
        });
    }
    hits
}

fn notice(payload: &Payload) -> &'static str {
    match payload {
        Payload::Experiment(id) => catalog()
            .iter()
            .find(|spec| spec.id == id)
            .map_or("This experiment is unavailable in this build.", |spec| {
                spec.description
            }),
        Payload::ExperimentHide => "Leave feature settings and saved feedback unchanged.",
        Payload::Update => "Load the installed build; your saved experiment choices carry forward.",
        _ => "Open this option for details.",
    }
}
