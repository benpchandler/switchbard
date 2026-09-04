//! Rendering. Reads `App`, writes a frame, and leaves a text copy of the screen behind.

use std::str::FromStr;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Mode, Pane};
use crate::columns::Column;
use crate::config::{Action, Surface};
use crate::group::Row;
use crate::paint::{self, PaintRule};
use crate::picker::{self, ColumnPurpose, PaintPick, Payload, PickerPurpose, ValuePicker};
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

fn draw_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.config.theme.clone();
    let repo = app
        .repo_root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let title = Line::from(vec![
        Span::styled(format!(" {repo} "), theme.style(Surface::TitleRepo)),
        Span::styled(table_title(app), theme.style(Surface::Title)),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style(Surface::Border))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let widths: Vec<Constraint> = app
        .state
        .columns
        .iter()
        .map(|column| match column {
            _ if app.state.glyph_columns.contains(column) => {
                Constraint::Length((2 + app.glyph_legend(*column).chars().count()).max(3) as u16)
            }
            column => match column.max_width() {
                Some(max) => Constraint::Length(fitted_width(app, *column, max)),
                None => Constraint::Min(20),
            },
        })
        .collect();
    let header_area = Rect { height: 1, ..inner };
    let cells = Layout::horizontal(widths).spacing(1).split(header_area);
    frame.render_widget(
        Paragraph::new("").style(theme.style(Surface::Header)),
        header_area,
    );
    for (index, (column, cell)) in app.state.columns.iter().zip(cells.iter()).enumerate() {
        let text = if app.state.glyph_columns.contains(column) {
            format!("{} {}", index + 1, app.glyph_legend(*column))
        } else {
            format!("{} {}", index + 1, column.header())
        };
        frame.render_widget(
            Paragraph::new(text).style(theme.style(Surface::Header)),
            *cell,
        );
    }
    let window = inner.height.saturating_sub(1) as usize;
    app.scroll = scroll_to_show(app.scroll, app.selected, window, &app.rows);
    let body = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    for (line, row) in app.rows.iter().skip(app.scroll).take(window).enumerate() {
        let row_area = Rect {
            y: body.y + line as u16,
            height: 1,
            ..body
        };
        let selected = app.scroll + line == app.selected;
        match row {
            Row::Heading(text) => frame.render_widget(
                Paragraph::new(format!("▸ {text}")).style(theme.style(Surface::Heading)),
                row_area,
            ),
            Row::Task(index) => {
                let task = &app.tasks()[*index];
                if selected {
                    frame.render_widget(
                        Paragraph::new("").style(theme.style(Surface::Selected)),
                        row_area,
                    );
                }
                for (column, cell) in app.state.columns.iter().zip(cells.iter()) {
                    let value = column.cell_text(task);
                    let text = if app.state.glyph_columns.contains(column) && !value.is_empty() {
                        app.config.glyph(*column, &value)
                    } else {
                        app.cell(*column, task)
                    };
                    let mut style = theme.column_style(*column);
                    if let Some(color) = paint::cell_color(&app.state.paint, task, *column) {
                        style = style.fg(color);
                    }
                    if selected {
                        style = style.patch(theme.style(Surface::Selected));
                    }
                    frame.render_widget(
                        Paragraph::new(text).style(style),
                        Rect {
                            y: row_area.y,
                            ..*cell
                        },
                    );
                }
            }
        }
    }
}

/// A fixed column is as wide as its header or its widest visible value, never
/// more than the catalog allows, so a column of `Done` does not reserve room
/// for `In Progress`.
fn fitted_width(app: &App, column: Column, max: u16) -> u16 {
    let header = column.header().chars().count() + 2;
    let widest = app
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Task(index) => Some(app.cell(column, &app.tasks()[*index]).chars().count()),
            Row::Heading(_) => None,
        })
        .max()
        .unwrap_or(0);
    (header.max(widest) as u16).min(max.max(header as u16))
}

/// The first row on screen: keeps `selected` in the window, and shows the
/// heading above it when the selected task opens its section.
fn scroll_to_show(scroll: usize, selected: usize, window: usize, rows: &[Row]) -> usize {
    if window == 0 {
        return scroll;
    }
    let mut scroll = scroll.min(selected);
    if selected >= scroll + window {
        scroll = selected + 1 - window;
    }
    let heading_above = selected > 0 && matches!(rows.get(selected - 1), Some(Row::Heading(_)));
    if heading_above && scroll == selected {
        scroll -= 1;
    }
    scroll
}

fn table_title(app: &App) -> String {
    let mut parts: Vec<String> = vec![app.view_label()];
    if !app.state.filter.is_empty() {
        parts.push(app.state.filter.clone());
    }
    if let Some(sort) = app.state.sort {
        parts.push(sort.label());
    }
    if app.state.columns != Column::DEFAULT_SHOWN {
        parts.push(format!("cols:{}", columns_text(&app.state.columns)));
    }
    if !app.state.glyph_columns.is_empty() {
        parts.push(format!("glyphs:{}", columns_text(&app.state.glyph_columns)));
    }
    if let Some(label) = app.state.abbreviated_label() {
        parts.push(label);
    }
    if !app.state.pin_top {
        parts.push("nopin".to_string());
    }
    if let Some(label) = app.settings.effective().label() {
        parts.push(label);
    }
    if !app.state.paint.is_empty() {
        parts.push(format!("paint:{}", app.state.paint.len()));
    }
    if let Some(group) = app.state.group {
        parts.push(format!("group:{}", group.name()));
        if group == Column::Project {
            parts.extend(app.initiatives());
        }
    }
    parts.push(format!("{}/{}", app.visible.len(), app.total_tasks()));
    format!(" {} ", parts.join(" · "))
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
            theme.style(Surface::Hint),
        )));
        lines.push(Line::from(""));
        for paragraph in task.description.lines() {
            lines.push(Line::from(paragraph.to_string()));
        }
        if !task.acceptance_criteria.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "acceptance",
                theme.style(Surface::Accent),
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
        .border_style(theme.style(Surface::Border));
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
        Action::Ball,
        Action::Group,
        Action::Settings,
        Action::Rank,
        Action::Command,
        Action::Reload,
        Action::Help,
        Action::View,
        Action::Quit,
    ];
    let entries: Vec<(String, String)> = actions
        .iter()
        .map(|action| (app.config.bindings_for(action).join(" "), action.name()))
        .chain(std::iter::once((
            "1-9".to_string(),
            "column actions".to_string(),
        )))
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
                    Span::styled(format!("{keys:<8}"), theme.style(Surface::Accent)),
                    Span::raw(format!("{name:<24}")),
                ]
            });
            Line::from(spans.collect::<Vec<_>>())
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(":bug <doing>  ", theme.style(Surface::Accent)),
        Span::raw("file a bug with this screen    "),
        Span::styled(":idea <want>  ", theme.style(Surface::Accent)),
        Span::raw("file an idea with this screen"),
    ]));
    lines.push(Line::from(vec![
        Span::styled(":view <name>  :reload  :q", theme.style(Surface::Accent)),
        Span::raw(
            "    f/s <col#> filter/sort by column; v<n> open view, vs<n> save it (vsd = default)",
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "config ~/.switchbard/tui.lua (hot reload) · views ~/.switchbard/views.lua + views/<repo>.lua · events ~/.switchbard/tui-events.jsonl",
        theme.style(Surface::Hint),
    )));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style(Surface::Border))
        .title(" keys ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let line = match app.mode {
        Mode::Filter => Line::from(vec![
            Span::styled("/", theme.style(Surface::Accent)),
            Span::raw(app.state.filter.clone()),
            Span::styled("▏", theme.style(Surface::Accent)),
        ]),
        Mode::Command => Line::from(vec![
            Span::styled(":", theme.style(Surface::Accent)),
            Span::raw(app.input.clone()),
            Span::styled("▏", theme.style(Surface::Accent)),
            Span::styled(
                format!("   {}", app.command_completions().join("  ")),
                theme.style(Surface::Hint),
            ),
        ]),
        Mode::PickValue if app.picker.is_some() => Line::from(Span::styled(
            format!(" {}", picker::hint(app.picker.as_ref().expect("checked"))),
            theme.style(Surface::Hint),
        )),
        Mode::PickValue
        | Mode::ViewChord
        | Mode::ViewSaveSlot
        | Mode::ViewGlobalSlot
        | Mode::RankChord => Line::from(Span::styled(
            app.status.clone(),
            theme.style(Surface::Status),
        )),
        Mode::Browse if !app.status.is_empty() => Line::from(Span::styled(
            app.status.clone(),
            theme.style(Surface::Status),
        )),
        Mode::Browse => browse_footer(app),
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// The footer while browsing: what is in effect as a chip, the situation, then
/// the keys with their letters on the `keys` surface.
fn browse_footer(app: &App) -> Line<'static> {
    let theme = &app.config.theme;
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    if !app.state.filter.is_empty() {
        spans.push(Span::styled(
            format!(" {} ", app.state.filter),
            theme.style(Surface::Chip),
        ));
        spans.push(Span::raw("  "));
    }
    if app.state.group.is_none() && app.grouping_is_useful() {
        spans.push(Span::styled(
            format!("{} projects · ", app.projects.len()),
            theme.style(Surface::Hint),
        ));
        spans.push(Span::styled("o", theme.style(Surface::Keys)));
        spans.push(Span::styled(
            " groups by project  ",
            theme.style(Surface::Hint),
        ));
    }
    for (key, name) in [
        ("/", "filter"),
        ("f", "filter-by"),
        ("s", "sort"),
        ("o", "group"),
        ("c", "columns"),
        ("p", "paint"),
        ("b", "ball"),
        ("t", "rank"),
        ("v", "views"),
        (",", "settings"),
        (":", "command"),
        ("?", "keys"),
        ("q", "quit"),
    ] {
        spans.push(Span::styled(key.to_string(), theme.style(Surface::Keys)));
        spans.push(Span::styled(
            format!(" {name}  "),
            theme.style(Surface::Hint),
        ));
    }
    Line::from(spans)
}

fn draw_picker(frame: &mut Frame, app: &App, picker: &ValuePicker, body: Rect) {
    let theme = &app.config.theme;
    let hint = picker::hint(picker);
    let width = picker
        .options
        .iter()
        .map(|option| option.label.chars().count() + 11)
        .chain(std::iter::once(hint.chars().count() + 4))
        .max()
        .unwrap_or(20)
        .max(60)
        .min(body.width.saturating_sub(4) as usize) as u16;
    let rows = picker.matching();
    let height = (rows.len() as u16 + 4).min(body.height.saturating_sub(2));
    let area = Rect {
        x: body.x + 2,
        y: body.y + 2,
        width,
        height,
    };
    let keys = picker.row_keys();
    let mut lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let value = &option.label;
            let shown = match (&picker.purpose, &option.payload) {
                (PickerPurpose::Filter(field), Payload::Text(value)) => {
                    Filter::field_allows(&app.state.filter, *field, value)
                }
                (PickerPurpose::Sort(_), Payload::Order(order)) => {
                    app.state.sort.is_some_and(|sort| sort.order == *order)
                }
                (PickerPurpose::Sort(_), Payload::NoSort) => app.state.sort.is_none(),
                (
                    PickerPurpose::ChooseColumn(_)
                    | PickerPurpose::Columns
                    | PickerPurpose::PaintColumn
                    | PickerPurpose::PaintTarget,
                    Payload::Column(column),
                ) => app.state.columns.contains(column),
                (PickerPurpose::MoveColumns(placed), _) => placed.contains(&(index + 1)),
                (PickerPurpose::PaintRules, Payload::Rule(rule)) => *rule == 0,
                (PickerPurpose::PaintValues(column), Payload::Text(value)) => {
                    paint::value_color(&app.state.paint, *column, value).is_some()
                }
                (PickerPurpose::PaintColor(pick), Payload::Text(color)) => match pick {
                    PaintPick::Value(column, painted) => {
                        paint::value_color(&app.state.paint, *column, painted).as_deref() == Some(color)
                    }
                    PaintPick::Rows(filter) => app.state.paint.iter().any(|rule| {
                        matches!(rule, PaintRule::Rows { filter: f, color: c } if f == filter && c == color)
                    }),
                    PaintPick::Column(column) => app.state.paint.iter().any(|rule| {
                        matches!(rule, PaintRule::Column { column: col, color: c } if col == column && c == color)
                    }),
                },
                _ => false,
            };
            let mut style = if index == picker.selected {
                theme.style(Surface::Selected)
            } else {
                Style::default()
            };
            if !shown {
                style = style.patch(theme.style(Surface::Hint));
            }
            match (&picker.purpose, &option.payload) {
                // Show the color itself: this is what the painted text will look like.
                (PickerPurpose::PaintColor(_), Payload::Text(color)) => {
                    if let Ok(color) = ratatui::style::Color::from_str(color) {
                        style = style.fg(color);
                    }
                }
                (PickerPurpose::PaintValues(column), Payload::Text(value)) => {
                    if let Some(color) = paint::value_color(&app.state.paint, *column, value)
                        .and_then(|color| ratatui::style::Color::from_str(&color).ok())
                    {
                        style = style.fg(color);
                    }
                }
                (PickerPurpose::PaintRules, Payload::Rule(rule)) => {
                    if let Some(color) = app.state.paint.get(*rule).and_then(PaintRule::swatch) {
                        style = style.fg(color);
                    }
                }
                _ => {}
            }
            let mark = if shown { "✓" } else { " " };
            Line::from(vec![
                Span::styled(
                    format!("{:<2}", keys.get(index).cloned().unwrap_or_default()),
                    theme.style(Surface::Accent),
                ),
                Span::styled(
                    format!("{mark}{value:<width$}", width = width as usize - 9),
                    style,
                ),
                Span::styled(
                    if option.count > 0 {
                        format!("{:>3}", option.count)
                    } else {
                        "   ".to_string()
                    },
                    theme.style(Surface::Hint),
                ),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(" {hint}"),
        theme.style(Surface::Hint),
    )));
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
        .border_style(theme.style(Surface::Accent))
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
        PickerPurpose::MoveColumns(placed) => format!(
            "move columns: {}",
            placed
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("")
        ),
        PickerPurpose::PaintValues(column) => format!("by {}", column.name()),
        PickerPurpose::PaintColumn => "paint which column".to_string(),
        PickerPurpose::PaintTarget => "paint".to_string(),
        PickerPurpose::PaintColor(_) => "color".to_string(),
        PickerPurpose::PaintRules => "paint rules · top is the base".to_string(),
        PickerPurpose::ColumnActions(column) => column.name().to_string(),
        PickerPurpose::Settings => "settings".to_string(),
    };
    if picker.typed.is_empty() {
        format!(" {subject} ")
    } else if typed_is_color {
        format!(" {} ← this is how it looks · enter applies ", picker.typed)
    } else {
        format!(" {subject}: {}▏", picker.typed)
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
