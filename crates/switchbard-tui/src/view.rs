//! Rendering. Reads `App`, writes a frame, and leaves a text copy of the screen behind.

use std::str::FromStr;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use switchbard_core::BacklogTask;

use crate::app::{App, ColumnPurpose, Mode, Pane, PickerPurpose, ValuePicker};
use crate::config::{Action, Column};
use crate::paint::{self, PaintRule};
use crate::tasks::Filter;
use crate::views::{columns_text, Scope};

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
    if let Some(picker) = &app.picker {
        draw_picker(frame, app, picker, body);
    }
    app.last_screen = buffer_text(frame.buffer_mut());
}

fn draw_table(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let repo = app
        .repo_root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let filter = if app.filter_text.is_empty() {
        String::new()
    } else {
        format!("{} · ", app.filter_text)
    };
    let sort = app
        .sort
        .map(|sort| format!("{} · ", sort.label()))
        .unwrap_or_default();
    let columns = if app.columns == Column::DEFAULT_SHOWN {
        String::new()
    } else {
        format!("cols:{} · ", columns_text(&app.columns))
    };
    let painted = if app.paint.is_empty() {
        String::new()
    } else {
        format!("paint:{} · ", app.paint.len())
    };
    let title = format!(
        " {repo} · {} · {filter}{sort}{columns}{painted}{}/{} ",
        app.view_label(),
        app.visible.len(),
        app.total_tasks()
    );
    let header = Row::new(
        app.columns
            .iter()
            .enumerate()
            .map(|(index, column)| Cell::from(format!("{} {}", index + 1, column.header()))),
    )
    .style(Style::default().fg(theme.header));
    let rows = (0..app.visible.len()).filter_map(|index| {
        let task = app.task(index)?;
        Some(Row::new(app.columns.iter().map(|column| {
            let cell = Cell::from(cell_text(*column, task));
            match paint::cell_color(&app.paint, task, *column) {
                Some(color) => cell.style(Style::default().fg(color)),
                None => cell,
            }
        })))
    });
    let widths = app.columns.iter().map(|column| match column {
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
        Action::FilterColumn,
        Action::SortColumn,
        Action::Columns,
        Action::Paint,
        Action::Command,
        Action::Reload,
        Action::Help,
        Action::View,
        Action::Quit,
    ];
    let entries: Vec<(String, String)> = actions
        .iter()
        .map(|action| (app.config.bindings_for(action).join(" "), action.name()))
        .chain(
            app.views
                .slots()
                .into_iter()
                .enumerate()
                .map(|(index, (saved, scope))| {
                    let scope = match scope {
                        Scope::Global => "",
                        Scope::Repo => " [repo]",
                    };
                    (
                        format!("v{}", index + 1),
                        format!("{}{scope}", saved.name()),
                    )
                }),
        )
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
        Span::raw(
            "    f/s <col#> filter/sort by column; v<n> open view, vs<n> save it (vsd = default)",
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "config ~/.switchbard/tui.lua (hot reload) · views ~/.switchbard/views.lua + views/<repo>.lua · events ~/.switchbard/tui-events.jsonl",
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
            Span::styled(
                format!("   {}", app.command_completions().join("  ")),
                Style::default().fg(theme.dim),
            ),
        ]),
        Mode::PickValue if app.picker.is_some() => Line::from(Span::styled(
            format!(" {}", picker_hint(app.picker.as_ref().expect("checked"))),
            Style::default().fg(theme.dim),
        )),
        Mode::PickValue | Mode::ViewChord | Mode::ViewSaveSlot | Mode::ViewGlobalSlot => {
            Line::from(Span::styled(
                app.status.clone(),
                Style::default().fg(theme.accent),
            ))
        }
        Mode::Browse if !app.status.is_empty() => Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(theme.accent),
        )),
        Mode::Browse => Line::from(Span::styled(
            format!(
                " {}  / filter  f filter-by  s sort  c columns  p paint  v views  : command  ? keys  q quit",
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

fn draw_picker(frame: &mut Frame, app: &App, picker: &ValuePicker, body: Rect) {
    let theme = &app.config.theme;
    let width = picker
        .options
        .iter()
        .map(|(value, _)| value.len() + 11)
        .max()
        .unwrap_or(20)
        .max(60)
        .min(body.width.saturating_sub(4) as usize) as u16;
    let height = (picker.options.len() as u16 + 2).min(body.height.saturating_sub(2));
    let area = Rect {
        x: body.x + 2,
        y: body.y + 2,
        width,
        height,
    };
    let lines: Vec<Line> = picker
        .matching()
        .iter()
        .enumerate()
        .map(|(index, (value, count))| {
            let shown = match &picker.purpose {
                PickerPurpose::Filter(field) => {
                    Filter::field_allows(&app.filter_text, *field, value)
                }
                PickerPurpose::Sort(column) => app
                    .sort
                    .is_some_and(|sort| sort.order.label(*column) == *value),
                PickerPurpose::ChooseColumn(_) | PickerPurpose::Columns => {
                    Column::parse(value).is_some_and(|column| app.columns.contains(&column))
                }
                PickerPurpose::PaintTarget => false,
                PickerPurpose::PaintColumn => {
                    Column::parse(value).is_some_and(|column| app.columns.contains(&column))
                }
                PickerPurpose::PaintByField(field, cells) => {
                    paint::rule_for_value(&app.paint, field.keyword(), value, *cells).is_some()
                }
                PickerPurpose::PaintColor(target) => app
                    .paint
                    .iter()
                    .any(|rule| &rule.target == target && rule.color == *value),
            };
            let mut style = if index == picker.selected {
                Style::default()
                    .bg(theme.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            if !shown {
                style = style.fg(theme.dim);
            }
            match &picker.purpose {
                // Show the color itself: this is what the painted text will look like.
                PickerPurpose::PaintColor(_) => {
                    if let Ok(color) = ratatui::style::Color::from_str(value) {
                        style = style.fg(color);
                    }
                }
                PickerPurpose::PaintByField(field, cells) => {
                    if let Some(color) =
                        paint::rule_for_value(&app.paint, field.keyword(), value, *cells)
                            .and_then(PaintRule::parsed_color)
                    {
                        style = style.fg(color);
                    }
                }
                _ => {}
            }
            let mark = if shown { "✓" } else { " " };
            Line::from(vec![
                Span::styled(format!("{} ", index + 1), Style::default().fg(theme.accent)),
                Span::styled(
                    format!("{mark}{value:<width$}", width = width as usize - 9),
                    style,
                ),
                Span::styled(
                    if *count > 0 {
                        format!("{count:>3}")
                    } else {
                        "   ".to_string()
                    },
                    Style::default().fg(theme.dim),
                ),
            ])
        })
        .collect();
    let pending = if picker.number.is_empty() {
        String::new()
    } else {
        format!("{}▏", picker.number)
    };
    let preview = match picker.purpose {
        PickerPurpose::PaintColor(_) => ratatui::style::Color::from_str(picker.typed.trim()).ok(),
        _ => None,
    };
    let title_style = match preview {
        Some(color) => Style::default().fg(color).add_modifier(Modifier::BOLD),
        None => Style::default(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title_style(title_style)
        .title(pending + &picker_title(picker, preview.is_some()));
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// What is being picked, plus any typed text. Key hints live in the footer.
fn picker_title(picker: &ValuePicker, typed_is_color: bool) -> String {
    let subject = match &picker.purpose {
        PickerPurpose::Filter(field) => field.keyword().to_string(),
        PickerPurpose::Sort(column) => format!("sort by {}", column.header()),
        PickerPurpose::ChooseColumn(ColumnPurpose::Filter) => "filter by column".to_string(),
        PickerPurpose::ChooseColumn(ColumnPurpose::Sort) => "sort by column".to_string(),
        PickerPurpose::Columns => "columns".to_string(),
        PickerPurpose::PaintByField(field, None) => format!("rows by {}", field.keyword()),
        PickerPurpose::PaintByField(field, Some(column)) => {
            format!("{} cells by {}", column.name(), field.keyword())
        }
        PickerPurpose::PaintColumn => "paint which column".to_string(),
        PickerPurpose::PaintTarget => "paint".to_string(),
        PickerPurpose::PaintColor(_) => "color".to_string(),
    };
    if picker.typed.is_empty() {
        format!(" {subject} ")
    } else if typed_is_color {
        format!(" {} ← this is how it looks · enter applies ", picker.typed)
    } else {
        format!(" {subject}: {}▏", picker.typed)
    }
}

/// The keys that work in this picker, shown dim in the footer.
pub fn picker_hint(picker: &ValuePicker) -> &'static str {
    match &picker.purpose {
        PickerPurpose::Filter(_) => "number or name picks one · space toggles · esc",
        PickerPurpose::Sort(_) => "number or name picks · esc",
        PickerPurpose::ChooseColumn(_) => "number or name · hidden columns listed last · esc",
        PickerPurpose::Columns => "number or name toggles · space keeps open · K/J move · esc",
        PickerPurpose::PaintByField(_, _) => {
            "pick a value, then its color · repeats · esc when done"
        }
        PickerPurpose::PaintColumn => "number or name · esc",
        PickerPurpose::PaintTarget => {
            "column number · r row · f filtered · c cells · d delete all · esc"
        }
        PickerPurpose::PaintColor(_) => "name or #hex · space clears this target · esc",
    }
}

pub fn buffer_text(buffer: &Buffer) -> String {
    let width = buffer.area.width as usize;
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    for row in buffer.content.chunks(width) {
        let line: String = row.iter().map(|cell| cell.symbol()).collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
