//! Rendering. Reads `App`, writes a frame, and leaves a text copy of the screen behind.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use switchbard_core::BacklogTask;

use crate::app::{App, Mode, Pane};
use crate::config::{Action, Column};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [body, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
    app.page_size = body.height.saturating_sub(3).max(1) as usize;
    match app.pane {
        Pane::None => draw_table(frame, app, body),
        Pane::Help => draw_help(frame, app, body),
        Pane::Detail => {
            let [left, right] =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(body);
            draw_table(frame, app, left);
            draw_detail(frame, app, right);
        }
    }
    draw_footer(frame, app, footer);
    app.last_screen = buffer_text(frame.buffer_mut());
}

fn draw_table(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let repo = app
        .repo_root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let title = format!(
        " {repo} · {} · {}/{} ",
        app.view,
        app.visible.len(),
        app.total_tasks()
    );
    let header = Row::new(
        app.config
            .columns
            .iter()
            .map(|column| Cell::from(column.header())),
    )
    .style(Style::default().fg(theme.dim));
    let rows = (0..app.visible.len()).filter_map(|index| {
        let task = app.task(index)?;
        Some(Row::new(
            app.config
                .columns
                .iter()
                .map(|column| Cell::from(cell_text(*column, task))),
        ))
    });
    let widths = app.config.columns.iter().map(|column| match column {
        Column::Id => Constraint::Length(9),
        Column::Status => Constraint::Length(12),
        Column::Priority => Constraint::Length(7),
        Column::Title => Constraint::Min(20),
        Column::Labels => Constraint::Length(20),
        Column::Project => Constraint::Length(18),
    });
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .title(Span::styled(title, Style::default().fg(theme.accent))),
        )
        .row_highlight_style(
            Style::default()
                .bg(theme.selected)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = TableState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn cell_text(column: Column, task: &BacklogTask) -> String {
    match column {
        Column::Id => task.id.clone(),
        Column::Status => task.status.clone(),
        Column::Priority => task.priority.clone(),
        Column::Title => task.title.clone(),
        Column::Labels => task.labels.join(","),
        Column::Project => task.project.clone().unwrap_or_default(),
    }
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(task) = app.selected_task() {
        lines.push(Line::from(Span::styled(
            task.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "{} · {} · {} · {}",
                task.id,
                task.status,
                task.priority,
                task.labels.join(",")
            ),
            Style::default().fg(theme.dim),
        )));
        lines.push(Line::from(""));
        for paragraph in task.description.lines() {
            lines.push(Line::from(paragraph.to_string()));
        }
        if !task.acceptance_criteria.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "acceptance",
                Style::default().fg(theme.accent),
            )));
            for item in &task.acceptance_criteria {
                let mark = if item.checked { "x" } else { " " };
                lines.push(Line::from(format!("[{mark}] {}", item.text)));
            }
        }
    } else {
        lines.push(Line::from("nothing selected"));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let actions = [
        Action::Down,
        Action::Up,
        Action::Top,
        Action::Bottom,
        Action::PageDown,
        Action::PageUp,
        Action::Open,
        Action::Back,
        Action::Filter,
        Action::Command,
        Action::Reload,
        Action::Help,
        Action::Quit,
    ];
    let entries: Vec<(String, String)> = actions
        .iter()
        .map(|action| (app.config.bindings_for(action).join(" "), action.name()))
        .chain(app.config.views.iter().map(|(name, filter)| {
            let keys = app
                .config
                .bindings_for(&Action::View(name.clone()))
                .join(" ");
            (keys, format!("view:{name} ({filter})"))
        }))
        .collect();
    let per_line = (area.width as usize / 32).max(1);
    let mut lines: Vec<Line> = entries
        .chunks(per_line)
        .map(|chunk| {
            let spans = chunk.iter().flat_map(|(keys, name)| {
                [
                    Span::styled(format!("{keys:<8}"), Style::default().fg(theme.accent)),
                    Span::raw(format!("{name:<24}")),
                ]
            });
            Line::from(spans.collect::<Vec<_>>())
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(":bug <doing>  ", Style::default().fg(theme.accent)),
        Span::raw("file a bug with this screen    "),
        Span::styled(":idea <want>  ", Style::default().fg(theme.accent)),
        Span::raw("file an idea with this screen"),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            ":view <name>  :reload  :q",
            Style::default().fg(theme.accent),
        ),
        Span::raw("    filter words: status: pri: label: project:"),
    ]));
    lines.push(Line::from(Span::styled(
        "config: ~/.switchbard/tui.lua (hot reload)    events: ~/.switchbard/tui-events.jsonl",
        Style::default().fg(theme.dim),
    )));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" keys ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let line = match app.mode {
        Mode::Filter => Line::from(vec![
            Span::styled("/", Style::default().fg(theme.accent)),
            Span::raw(app.filter_text.clone()),
            Span::styled("▏", Style::default().fg(theme.accent)),
        ]),
        Mode::Command => Line::from(vec![
            Span::styled(":", Style::default().fg(theme.accent)),
            Span::raw(app.input.clone()),
            Span::styled("▏", Style::default().fg(theme.accent)),
        ]),
        Mode::Browse if !app.status.is_empty() => Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(theme.accent),
        )),
        Mode::Browse => Line::from(Span::styled(
            format!(
                " {}  / filter  : command  ? keys  q quit",
                if app.filter_text.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", app.filter_text)
                }
            ),
            Style::default().fg(theme.dim),
        )),
    };
    frame.render_widget(Paragraph::new(line), area);
}

pub fn buffer_text(buffer: &Buffer) -> String {
    let width = buffer.area.width as usize;
    let mut out = String::new();
    for row in buffer.content.chunks(width) {
        let line: String = row.iter().map(|cell| cell.symbol()).collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
