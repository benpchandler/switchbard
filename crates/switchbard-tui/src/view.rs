//! Rendering. Reads `App`, writes a frame, and leaves a text copy of the screen behind.

use std::str::FromStr;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Mode, Pane};
use crate::columns::Column;
use crate::config::{Action, Surface};
use crate::group::Row;
use crate::page::Page;
use crate::paint::{self, PaintRule};
use crate::picker::{self, ColumnPurpose, PaintPick, Payload, PickerPurpose, ValuePicker};
use crate::tasks::Filter;
use crate::views::{columns_text, Scope};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let footer_height = if app.mode == Mode::NewTask { 3 } else { 1 };
    let [navigation, notification, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(
            !app.pull_requests.notifications.is_empty() || app.pr_merge.ongoing().is_some(),
        )),
        Constraint::Min(0),
        Constraint::Length(footer_height),
    ])
    .areas(frame.area());
    crate::navigation::draw(frame, app, navigation);
    draw_notification(frame, app, notification);
    app.page_size = body.height.saturating_sub(3).max(1) as usize;
    if app.page == Page::Inbox && app.pane != Pane::Help {
        crate::inbox::draw(frame, app, body);
    } else if app.page == Page::PullRequests && app.pane != Pane::Help {
        crate::pr_view::draw(frame, app, body);
    } else {
        match app.pane {
            Pane::None => draw_table(frame, app, body),
            Pane::Help => draw_help(frame, app, body),
            Pane::Detail => {
                let [left, right] = crate::detail_pane::split(body);
                draw_table(frame, app, left);
                draw_detail(frame, app, right);
            }
        }
    }
    draw_footer(frame, app, footer);
    app.pr_merge.confirmation_visible = false;
    if let Some(picker) = app.picker.clone() {
        draw_picker(frame, app, &picker, body);
    }
    app.last_screen = buffer_text(frame.buffer_mut());
}

fn draw_notification(frame: &mut Frame, app: &App, area: Rect) {
    let alerts = &app.pull_requests.notifications;
    let Some(message) = app.pr_merge.ongoing().or_else(|| alerts.latest()) else {
        return;
    };
    let dismiss = app
        .config
        .bindings_for(&Action::DismissNotifications)
        .join("/");
    let hint = if app.pr_merge.is_submitting() {
        " pending".to_string()
    } else if app.pr_merge.ongoing().is_some() {
        " Esc cancels".to_string()
    } else {
        format!(" {} · {dismiss} dismiss", alerts.len())
    };
    let hint_width = u16::try_from(hint.chars().count()).unwrap_or(u16::MAX);
    let [message_area, hint_area] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(hint_width.min(area.width / 2)),
    ])
    .areas(area);
    let style = app.config.theme.style(Surface::Status);
    frame.render_widget(
        Paragraph::new(format!("PR {message}")).style(style),
        message_area,
    );
    frame.render_widget(Paragraph::new(hint).style(style), hint_area);
}

fn draw_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.config.theme.clone();
    // Cloned like the theme: the render path writes `app.scroll` part way
    // through, so nothing may hold a borrow of `app` across the whole frame.
    let registry = std::sync::Arc::clone(app.registry());
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
            column => match column.max_width(&registry) {
                Some(max) => Constraint::Length(fitted_width(app, *column, max)),
                None => Constraint::Min(20),
            },
        })
        .collect();
    let header_area = Rect { height: 1, ..inner };
    let cells = crate::list_presentation::cells(header_area, &widths);
    let headers: Vec<String> = app
        .state
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let label = if app.state.glyph_columns.contains(column) {
                app.glyph_legend(*column)
            } else {
                column.header(&registry).to_string()
            };
            format!("{} {}", index + 1, label)
        })
        .collect();
    crate::list_presentation::header(
        frame,
        header_area,
        &cells,
        &headers,
        theme.style(Surface::Header),
    );
    let heading =
        app.selected > 0 && matches!(app.rows.get(app.selected - 1), Some(Row::Heading { .. }));
    let title_width = app
        .state
        .columns
        .iter()
        .zip(cells.iter())
        .find(|(column, _)| **column == Column::Title)
        .map_or(0, |(_, cell)| cell.width);
    let row_height = |row: usize| -> usize {
        match app.rows.get(row) {
            Some(Row::Task(index)) => {
                let title = crate::row_layout::title_text(&app.tasks()[*index].title);
                usize::from(app.state.row_layout.title_height(&title.text, title_width))
                    + usize::from(app.state.row_layout.spaced && row != app.selected)
            }
            _ => 1,
        }
    };
    let viewport = crate::list_presentation::ListViewport::variable(
        app.scroll,
        app.selected,
        app.rows.len(),
        inner.height.saturating_sub(1) as usize,
        heading,
        row_height,
    );
    app.scroll = viewport.scroll;
    let window = viewport.slots;
    let mut used = 0;
    let mut visible_tasks = 0;
    let body = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    for (line, row) in app.rows.iter().skip(app.scroll).take(window).enumerate() {
        if used >= window {
            break;
        }
        let content_height = match row {
            Row::Heading { .. } => 1,
            Row::Task(index) => {
                visible_tasks += 1;
                let title = crate::row_layout::title_text(&app.tasks()[*index].title);
                app.state.row_layout.title_height(&title.text, title_width)
            }
        };
        let row_area = Rect {
            y: body.y + used as u16,
            height: content_height.min((window - used) as u16),
            ..body
        };
        used += usize::from(content_height)
            + usize::from(matches!(row, Row::Task(_)) && app.state.row_layout.spaced);
        let selected = app.scroll + line == app.selected;
        match row {
            Row::Heading { text, depth } => frame.render_widget(
                Paragraph::new(format!("{}▸ {text}", "  ".repeat(*depth)))
                    .style(theme.style(Surface::Heading)),
                row_area,
            ),
            Row::Task(index) => {
                let task = &app.tasks()[*index];
                // The pulse sits on top of the cursor band: it is bright only
                // briefly, and the cursor shows through as it fades.
                let glow = (!app.working(task).is_empty()).then(|| app.work_glow());
                let working = glow
                    .map(|glow| theme.working_style(glow))
                    .filter(|style| *style != Style::default());
                // A done task is never blocked (TASK-209.2's AC #2) even if a
                // listed dependency reads open — matches the GUI's own guard
                // (`ui/backlog/list.rs`).
                let blocked = !task.is_done() && app.relations.blocked.contains(&task.id);
                if selected {
                    frame.render_widget(
                        Paragraph::new("").style(theme.style(Surface::Selected)),
                        row_area,
                    );
                }
                if let Some(glow) = working {
                    frame.render_widget(Paragraph::new("").style(glow), row_area);
                }
                for (column, cell) in app.state.columns.iter().zip(cells.iter()) {
                    let value =
                        column.cell_text(&registry, task, &app.goals, &app.relations.blocked);
                    let text = if app.state.glyph_columns.contains(column) && !value.is_empty() {
                        app.config.glyph(*column, &value)
                    } else {
                        app.cell(*column, task)
                    };
                    let mut style = theme.column_style(*column);
                    if blocked {
                        style = style.patch(theme.style(Surface::Hint));
                    }
                    if let Some(color) = paint::cell_color(
                        &app.state.paint,
                        &registry,
                        task,
                        *column,
                        &app.goals,
                        &app.relations.blocked,
                    ) {
                        style = style.fg(color);
                    }
                    if selected {
                        style = style.patch(theme.style(Surface::Selected));
                    }
                    if let Some(band) = working {
                        style = style.patch(band);
                    }
                    if let Some(glow) = glow {
                        // The text breathes with the band: lifted toward
                        // white at the peak, its rest colour in the trough.
                        style = style.fg(theme.working_fg(style.fg, glow));
                    }
                    let cell_area = Rect {
                        y: row_area.y,
                        height: row_area.height,
                        ..*cell
                    };
                    if *column == Column::Title {
                        draw_task_title(frame, &text, app.state.row_layout, style, cell_area);
                    } else {
                        frame.render_widget(
                            Paragraph::new(text).style(style),
                            Rect {
                                height: 1,
                                ..cell_area
                            },
                        );
                    }
                }
            }
        }
    }
    app.page_size = visible_tasks.max(1);
}

fn draw_task_title(
    frame: &mut Frame,
    title: &str,
    layout: crate::row_layout::RowLayout,
    style: Style,
    area: Rect,
) {
    let title_text = crate::row_layout::title_text(title);
    let text = title_text.text;
    if layout.lines() == 1 {
        frame.render_widget(Paragraph::new(text).style(style), area);
        return;
    }
    let paragraph = crate::row_layout::paragraph(&text).style(style);
    let clipped =
        paragraph.line_count(area.width) > usize::from(area.height) || title_text.truncated;
    frame.render_widget(paragraph, area);
    if clipped && area.width > 0 && area.height > 0 {
        // Do not leave half a wide glyph underneath the overflow indicator.
        if area.width > 1 {
            if let Some(cell) = frame
                .buffer_mut()
                .cell_mut((area.right() - 2, area.bottom() - 1))
            {
                if Span::raw(cell.symbol()).width() > 1 {
                    cell.set_symbol(" ");
                }
            }
        }
        frame.render_widget(
            Paragraph::new("…").style(style),
            Rect::new(area.right() - 1, area.bottom() - 1, 1, 1),
        );
    }
}

/// A fixed column is as wide as its header or its widest visible value, never
/// more than the catalog allows, so a column of `Done` does not reserve room
/// for `In Progress`.
fn fitted_width(app: &App, column: Column, max: u16) -> u16 {
    let header = column.header(app.registry()).chars().count() + 2;
    let widest = app
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Task(index) => Some(app.cell(column, &app.tasks()[*index]).chars().count()),
            Row::Heading { .. } => None,
        })
        .max()
        .unwrap_or(0);
    (header.max(widest) as u16).min(max.max(header as u16))
}

fn table_title(app: &App) -> String {
    let mut parts: Vec<String> = vec![app.view_label()];
    if !app.state.filter.is_empty() {
        parts.push(app.state.filter.clone());
    }
    if let Some(sort) = app.state.sort {
        parts.push(sort.label(app.registry()));
    }
    if app.state.columns != Column::DEFAULT_SHOWN {
        parts.push(format!(
            "cols:{}",
            columns_text(&app.state.columns, app.registry())
        ));
    }
    if !app.state.glyph_columns.is_empty() {
        parts.push(format!(
            "glyphs:{}",
            columns_text(&app.state.glyph_columns, app.registry())
        ));
    }
    if let Some(label) = app.state.abbreviated_label(app.registry()) {
        parts.push(label);
    }
    if !app.state.pin_top {
        parts.push("nopin".to_string());
    }
    if let Some(label) = app.state.row_layout.label() {
        parts.push(label);
    }
    if let Some(label) = app.settings.effective().label() {
        parts.push(label);
    }
    if !app.state.paint.is_empty() {
        parts.push(format!("paint:{}", app.state.paint.len()));
    }
    if !app.state.group.is_flat() {
        let label = app.state.group.label(app.registry(), &app.group_levels);
        parts.push(format!("outline:{label}"));
        if app.group_levels.contains(&Column::Project) {
            parts.extend(app.initiatives());
        }
    }
    match app.working_sessions() {
        0 => {}
        n => parts.push(format!("working:{n}")),
    }
    parts.push(format!("{}/{}", app.visible.len(), app.total_tasks()));
    format!(" {} ", parts.join(" · "))
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(task) = app.selected_task() {
        lines.push(crate::detail_pane::title(task.title.clone()));
        lines.push(crate::detail_pane::metadata(
            format!(
                "{} · {} · {} · {}",
                task.id,
                task.status,
                task.priority,
                task.labels.join(",")
            ),
            theme,
        ));
        for session in app.working(task) {
            lines.push(Line::from(Span::styled(
                format!(
                    "working · {} {} (pid {}) since {}",
                    session.agent,
                    session.short_id(),
                    session.pid,
                    claimed_clock(session, &task.id)
                ),
                theme.style(Surface::Working),
            )));
        }
        lines.push(Line::from(""));
        for paragraph in task.description.lines() {
            lines.push(Line::from(paragraph.to_string()));
        }
        if !task.acceptance_criteria.is_empty() {
            lines.push(Line::from(""));
            lines.push(crate::detail_pane::section("acceptance", theme));
            for item in &task.acceptance_criteria {
                let mark = if item.checked { "x" } else { " " };
                lines.push(Line::from(format!("[{mark}] {}", item.text)));
            }
        }
        // "Blocked by" (open dependencies only) and "Blocks" (the reverse
        // edge), mirroring the GUI's `ui/backlog/detail_lists.rs`
        // (`render_dependencies`'/`render_blocks`' sections) off the same
        // `tasks::TaskRelations` cache the row dimming and the `blocked`
        // filter read.
        if let Some(deps) = app.relations.blocked_by.get(&task.id) {
            lines.push(Line::from(""));
            lines.push(crate::detail_pane::section("blocked by", theme));
            for (id, title) in deps {
                lines.push(Line::from(format!("{id} {title}")));
            }
        }
        if let Some(dependents) = app.relations.blocks.get(&task.id) {
            lines.push(Line::from(""));
            lines.push(crate::detail_pane::section("blocks", theme));
            for (id, title, done) in dependents {
                let status = if *done { "done" } else { "open" };
                lines.push(Line::from(format!("{id} {title} ({status})")));
            }
        }
    } else {
        lines.push(Line::from("nothing selected"));
    }
    crate::detail_pane::draw(frame, theme, area, lines, 0);
}

/// `HH:MM` of the claim on `task_id`, from its RFC 3339 stamp.
fn claimed_clock(session: &switchbard_core::WorkSession, task_id: &str) -> String {
    session
        .claims
        .iter()
        .find(|claim| claim.task_id == task_id)
        .and_then(|claim| chrono::DateTime::parse_from_rfc3339(&claim.claimed_at).ok())
        .map(|stamp| stamp.format("%H:%M").to_string())
        .unwrap_or_default()
}

fn draw_help(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.page == Page::Inbox {
        crate::inbox::draw_help(frame, app, area);
        return;
    }
    let theme = &app.config.theme;
    let entries: Vec<(String, String)> = Action::all()
        .filter(|action| app.page.allows(action))
        .map(|action| (app.config.bindings_for(&action).join(" "), action.name()))
        .chain((app.page == Page::Tasks).then(|| {
            let keys = app
                .config
                .bindings_for(&Action::Rank)
                .iter()
                .map(|key| format!("{key} a"))
                .collect::<Vec<_>>()
                .join(" ");
            (keys, "link parent task".to_string())
        }))
        .chain(std::iter::once((
            "1-9".to_string(),
            "column actions".to_string(),
        )))
        .chain(app.views.slots().into_iter().map(|(index, saved, scope)| {
            let scope = match scope {
                Scope::Global => "",
                Scope::Repo => " [repo]",
            };
            (
                format!("v{}", index + 1),
                format!("{}{scope}", saved.display_name(app.registry())),
            )
        }))
        .collect();
    let per_line = (area.width.saturating_sub(2) as usize / 32).max(1);
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
    for (command, description) in [
        (":bug <doing>", "file a bug with this screen"),
        (":idea <want>", "file an idea with this screen"),
        (":theme <name>", "how sbt itself looks"),
        (":palette <name>", "colors `auto` paints with"),
        (":view <name>  :reload  :q", ""),
        ("f/s <col#>", "filter/sort by column"),
        ("v<n>", "open view; vs<n> save it (vsd = default)"),
    ] {
        lines.push(Line::from(vec![
            Span::styled(format!("{command}  "), theme.style(Surface::Accent)),
            Span::raw(description),
        ]));
    }
    if app.page == Page::PullRequests {
        lines.push(Line::from("PR fields: status/lifecycle, id, title, tasks, checks, review, merge, draft; PR views use .prs.lua files."));
    }
    lines.push(Line::from(Span::styled(
        "config ~/.switchbard/tui.lua (hot reload) · views ~/.switchbard/views.lua + views/<repo>.lua · events ~/.switchbard/tui-events.jsonl",
        theme.style(Surface::Hint),
    )));
    let up = app.config.bindings_for(&Action::Up).join("/");
    let down = app.config.bindings_for(&Action::Down).join("/");
    let back = app.config.bindings_for(&Action::Back).join("/");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style(Surface::Border))
        .title(" keys ")
        .title_bottom(format!(" {up}/{down} scroll · {back} close "));
    let inner = block.inner(area);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let max_scroll = paragraph
        .line_count(inner.width)
        .saturating_sub(inner.height as usize);
    app.help_scroll = app
        .help_scroll
        .min(max_scroll.min(u16::MAX as usize) as u16);
    frame.render_widget(paragraph.scroll((app.help_scroll, 0)).block(block), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    if app.mode == Mode::NewTask {
        draw_new_task(frame, app, area);
        return;
    }
    let theme = &app.config.theme;
    let line = match app.mode {
        Mode::Filter => Line::from(vec![
            Span::styled("/", theme.style(Surface::Accent)),
            Span::raw(app.filter_text().to_string()),
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
        Mode::PickValue if app.picker.is_some() => {
            Line::from(Span::styled(app.status.clone(), theme.style(Surface::Hint)))
        }
        Mode::NewTask | Mode::PickValue => Line::from(Span::styled(
            app.status.clone(),
            theme.style(Surface::Status),
        )),
        Mode::BallName => Line::from(vec![
            Span::styled(" ball person: ", theme.style(Surface::Accent)),
            Span::raw(app.input.clone()),
            Span::styled("▏", theme.style(Surface::Accent)),
        ]),
        Mode::RenameView => Line::from(vec![
            Span::styled(" view name: ", theme.style(Surface::Accent)),
            Span::raw(app.input.clone()),
            Span::styled("▏", theme.style(Surface::Accent)),
        ]),
        Mode::Browse if !app.status.is_empty() => Line::from(Span::styled(
            app.status.clone(),
            theme.style(Surface::Status),
        )),
        Mode::Browse => browse_footer(app),
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// Keep the end of the bounded UTF-8 draft and its cursor visible while typing.
fn draw_new_task(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let prefix = if area.width >= 20 { "new task: " } else { ":" };
    let room = usize::from(area.width).saturating_sub(prefix.len() + 1);
    let mut start = app.input.len();
    let mut width = 0;
    let draft = Span::raw(app.input.as_str());
    let graphemes: Vec<_> = draft.styled_graphemes(Style::default()).collect();
    for grapheme in graphemes.iter().rev() {
        let character_width = Span::raw(grapheme.symbol).width();
        if width + character_width > room {
            break;
        }
        width += character_width;
        start -= grapheme.symbol.len();
    }
    let input = Line::from(vec![
        Span::styled(prefix, theme.style(Surface::Accent)),
        Span::raw(app.input[start..].to_string()),
        Span::styled("▏", theme.style(Surface::Accent)),
    ]);
    let lines = vec![
        input,
        Line::styled(app.status.clone(), theme.style(Surface::Status)),
        Line::styled("Enter create · Esc cancel", theme.style(Surface::Hint)),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// The footer while browsing: what is in effect as a chip, the situation, then
/// the keys with their letters on the `keys` surface.
fn browse_footer(app: &App) -> Line<'static> {
    let actions = if app.page == Page::Inbox {
        vec![(Action::Page, "page"), (Action::Help, "keys")]
    } else if app.page == Page::Tasks {
        vec![
            (Action::Rank, "tasks"),
            (Action::View, "views"),
            (Action::Help, "keys"),
        ]
    } else {
        vec![(Action::View, "views"), (Action::Help, "keys")]
    };
    let text = actions
        .iter()
        .map(|(action, label)| format!("{} {label}", app.config.bindings_for(action).join("/")))
        .collect::<Vec<_>>()
        .join(" · ");
    Line::from(Span::styled(text, app.config.theme.style(Surface::Hint)))
}

fn draw_picker(frame: &mut Frame, app: &mut App, picker: &ValuePicker, body: Rect) {
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
    let width = if picker.purpose == PickerPurpose::Merge {
        body.width.saturating_sub(4)
    } else {
        width
    };
    let rows = picker.matching();
    let height = rows
        .len()
        .saturating_add(4)
        .min(body.height.saturating_sub(2) as usize) as u16;
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
                    Filter::field_allows(app.filter_text(), *field, value, app.registry())
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
                (PickerPurpose::TaskParent(id), Payload::Parent(parent)) => app.tasks().iter().any(|task| task.id == *id && task.parent == *parent),
                (PickerPurpose::TaskProject(id), Payload::Project(project)) => app.tasks().iter().any(|task| task.id == *id && task.project == *project),
                (PickerPurpose::TaskStatus(id), Payload::Text(status)) => app.tasks().iter().any(|task| task.id == *id && task.status.eq_ignore_ascii_case(status)),
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
                    format!("{:<2}", if matches!(picker.purpose, PickerPurpose::TaskParent(_)) { String::new() } else { keys.get(index).cloned().unwrap_or_default() }),
                    theme.style(Surface::Accent),
                ),
                Span::styled(
                    format!("{mark}{value:<width$}", width = (width as usize).saturating_sub(9)),
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
    if rows.is_empty() && matches!(picker.purpose, PickerPurpose::TaskParent(_)) {
        lines.push(Line::from("No matching parent tasks"));
    }
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
        .title(pending + &picker_title(picker, preview.is_some(), app.registry()));
    let block = if picker.purpose != PickerPurpose::Merge
        && rows.len().saturating_add(4) > height as usize
    {
        let navigation = if matches!(picker.purpose, PickerPurpose::TaskParent(_)) && width >= 28 {
            "↑↓ Enter saves Esc"
        } else if width >= 28 {
            "↑↓ →open ←back Esc"
        } else {
            "↑↓ Esc"
        };
        let position = format!(
            "{navigation} · {}/{}",
            picker.selected.saturating_add(1).min(rows.len()),
            rows.len()
        );
        // The in-box hint line (pushed after the option rows) scrolls off
        // once there are more rows than fit, so the overflow footer is the
        // only place left to say what the keys do — keep it there when it
        // fits, rather than silently dropping the purpose hint.
        let with_hint = format!(" {position} · {hint} ");
        let footer = if with_hint.chars().count() as u16 <= width {
            with_hint
        } else {
            format!(" {position} ")
        };
        block.title_bottom(Line::from(footer))
    } else {
        block
    };
    if picker.purpose == PickerPurpose::Merge {
        let mut confirmation: Vec<Line> = app
            .pr_merge
            .confirmation_lines()
            .into_iter()
            .map(Line::from)
            .collect();
        confirmation.push(Line::from(""));
        confirmation.extend(lines);
        let area = Rect {
            height: body.height.saturating_sub(2),
            ..area
        };
        app.pr_merge.confirmation_visible = crate::list_presentation::picker(
            frame,
            area,
            block,
            confirmation,
            picker.selected,
            true,
        );
    } else {
        crate::list_presentation::picker(frame, area, block, lines, picker.selected, false);
    }
}

/// What is being picked, plus any typed text. Key hints live in the footer.
fn picker_title(
    picker: &ValuePicker,
    typed_is_color: bool,
    registry: &crate::columns::ColumnRegistry,
) -> String {
    let subject = match &picker.purpose {
        PickerPurpose::Filter(field) => field.keyword(registry).to_string(),
        PickerPurpose::Sort(column) => format!("sort by {}", column.header(registry)),
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
        PickerPurpose::PaintValues(column) => format!("by {}", column.name(registry)),
        PickerPurpose::PaintColumn => "paint which column".to_string(),
        PickerPurpose::PaintTarget => "paint".to_string(),
        PickerPurpose::PaintColor(_) => "color".to_string(),
        PickerPurpose::PaintRules => "paint rules · top is the base".to_string(),
        PickerPurpose::ChoosePaintRule(action) => format!("{action:?} paint rule"),
        PickerPurpose::ColumnActions(column) => column.name(registry).to_string(),
        PickerPurpose::Settings => "settings".to_string(),
        PickerPurpose::Goals(id) => format!("{id} · goals"),
        PickerPurpose::Organize => "organize by".to_string(),
        PickerPurpose::Ball => "ball".to_string(),
        PickerPurpose::Merge => "Confirm PR merge".to_string(),
        PickerPurpose::Task => "task".to_string(),
        PickerPurpose::TopList => "task · top list".to_string(),
        PickerPurpose::TaskParent(id) => format!("{id} · parent"),
        PickerPurpose::TaskProject(id) => format!("{id} · project"),
        PickerPurpose::TaskStatus(id) => format!("{id} · status"),
        PickerPurpose::Views => "views".to_string(),
        PickerPurpose::SaveView => "save view".to_string(),
        PickerPurpose::GlobalView => "make view global".to_string(),
        PickerPurpose::RenameView => "name which view".to_string(),
        PickerPurpose::DeleteView => "delete which view".to_string(),
        PickerPurpose::ChooseColumnAction(action) => action.label().to_string(),
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
        let mut line = String::new();
        let mut next = 0;
        for (column, cell) in row.iter().enumerate() {
            if column < next {
                continue;
            }
            line.push_str(cell.symbol());
            next = column + Span::raw(cell.symbol()).width().max(1);
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
