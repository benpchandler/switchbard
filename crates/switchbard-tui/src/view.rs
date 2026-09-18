//! Rendering. Reads `App`, writes a frame, and leaves a text copy of the screen behind.

mod detail_content;
mod experiment_picker;
mod history_picker;
mod history_preview;
pub(crate) mod history_title;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Mode, Pane};
use crate::columns::Column;
use crate::config::{Action, Surface, Theme};
use crate::detail_pane::FieldRow;
use crate::group::Row;
use crate::page::Page;
use crate::paint::{self};
use crate::picker::{self, ColumnPurpose, Payload, PickerPurpose, ValuePicker};
use crate::tasks::Filter;
use crate::views::{columns_text, Scope};

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.experiment_hits.clear();
    frame.render_widget(
        Paragraph::new("").style(app.config.theme.canvas_style()),
        frame.area(),
    );
    let footer_height = match app.mode {
        Mode::NewTask => 3,
        Mode::DetailInput(_) => 2,
        Mode::Filter if app.filter_completion_hint().is_some() => 2,
        Mode::Command if !app.status.is_empty() => 2,
        _ => 1,
    };
    let [navigation, notification, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(
            !app.pull_requests.notifications.is_empty()
                || app.pr_merge.ongoing().is_some()
                || app.report.message().is_some()
                || app.experiment_notice().is_some(),
        )),
        Constraint::Min(0),
        Constraint::Length(footer_height),
    ])
    .areas(frame.area());
    app.detail_hit.set_areas(body, app.pane == Pane::Detail);
    crate::navigation::draw(frame, app, navigation);
    draw_notification(frame, app, notification);
    app.page_size = body.height.saturating_sub(3).max(1) as usize;
    if app.page == Page::Inbox && app.pane != Pane::Help {
        crate::inbox::draw(frame, app, body);
    } else if app.page == Page::Agents && app.pane != Pane::Help {
        crate::agents::draw(frame, app, body);
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
    app.task_cancel.confirmation_visible = false;
    app.agent_kill.confirmation_visible = false;
    if let Some(picker) = app.picker.clone() {
        draw_picker(frame, app, &picker, body);
    }
    app.last_screen = buffer_text(frame.buffer_mut());
}

fn draw_notification(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(message) = app.report.message() {
        let message = if app.report.is_pending() {
            message.to_string()
        } else {
            format!(
                "{message} · {} dismiss",
                app.config
                    .bindings_for(&Action::DismissNotifications)
                    .join("/")
            )
        };
        frame.render_widget(
            Paragraph::new(message).style(app.config.theme.style(Surface::Status)),
            area,
        );
        return;
    }
    let alerts = &app.pull_requests.notifications;
    let Some(message) = app.pr_merge.ongoing().or_else(|| alerts.latest()) else {
        if let Some(message) = app.experiment_notice() {
            frame.render_widget(
                Paragraph::new(message).style(app.config.theme.style(Surface::Status)),
                area,
            );
        }
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
    let repo = app
        .repo_root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let budget = usize::from(area.width / 3).clamp(3, 24);
    let repo = if repo.chars().count() > budget {
        format!(
            "{}…",
            repo.chars()
                .take(budget.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        repo
    };
    let title_paint = paint::scoped_style(
        &app.state.paint,
        &theme,
        &app.config.palette,
        paint::PaintScope::Title,
    );
    // TASK-235: the title line carries only the repo chip, the shown count and
    // the view name. The filter and every view setting used to be packed onto
    // this one line (`table_title`, below) and stopped fitting once a filter
    // grew past a few terms; they now render inside the frame body and the
    // footer hint bar (see the filter line and `view_settings_summary` below).
    let title = Line::from(vec![
        Span::styled(format!(" {repo} "), theme.style(Surface::TitleRepo)),
        Span::styled(
            format!(" {}/{} shown ", app.visible.len(), app.total_tasks()),
            theme.style(Surface::Header),
        ),
        Span::styled(
            format!(" {} ", app.view_label()),
            theme.style(Surface::Title).patch(title_paint),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style(Surface::Border))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        app.detail_hit.list_rows.clear();
        return;
    }
    let table_area = draw_filter_line(frame, app, &theme, inner);
    let mut cursor = TableCursor {
        scroll: app.scroll,
        selected: app.selected,
        page_size: app.page_size,
        highlight: true,
    };
    let list_rows = draw_task_rows(frame, app, &app.state, &app.rows, &mut cursor, table_area);
    app.scroll = cursor.scroll;
    app.page_size = cursor.page_size;
    app.detail_hit.list_rows = list_rows;
}

/// The active filter's own full-width line inside the list frame, directly
/// below the title border and above the header row: wraps at `inner`'s width,
/// omitted (no reserved row) when there is no filter. Returns the remaining
/// area for the header and task rows. `app.state.filter` updates on every
/// keystroke while `/` is open (`Mode::Filter`), so this line already mirrors
/// a live edit in progress, not only the last committed filter — it is the
/// same value `draw_footer`'s `Mode::Filter` line echoes.
///
/// Capped to `inner.height - 2` so the header and at least one task row
/// always survive a long filter: a filter that still doesn't fit is cut with
/// a trailing ellipsis rather than starving every task row off screen (a
/// filter matching a task could hide it with no visible cue why).
fn draw_filter_line(frame: &mut Frame, app: &App, theme: &Theme, inner: Rect) -> Rect {
    if app.state.filter.is_empty() {
        return inner;
    }
    let text = format!("/ {}", app.state.filter);
    let budget = inner.height.saturating_sub(2);
    let full_height = wrapped_height(&Line::from(text.as_str()), inner.width);
    let (text, height) = if full_height <= budget {
        (text, full_height)
    } else {
        // Bounded shrink search (Power-of-10 rule 2): nothing longer than
        // `budget * inner.width` characters can possibly fit in `budget` rows
        // regardless of where word-wrap breaks it, so start there instead of
        // at the filter's full length — tests exercise filters up to 64 KiB
        // (persistence_edges.rs), and starting the search at the full length
        // turned this into a multi-minute O(length²) scan.
        let max_visible = usize::from(budget).saturating_mul(usize::from(inner.width));
        let mut chars = text.chars().count().min(max_visible);
        loop {
            let candidate = truncate_with_ellipsis(&text, chars);
            let fits = wrapped_height(&Line::from(candidate.as_str()), inner.width) <= budget;
            if fits || chars == 0 {
                break (candidate, budget);
            }
            chars -= 1;
        }
    };
    let [filter_area, table_area] =
        Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).areas(inner);
    if height > 0 {
        frame.render_widget(
            Paragraph::new(text)
                .style(theme.style(Surface::Context))
                .wrap(Wrap { trim: false }),
            filter_area,
        );
    }
    table_area
}

struct TableCursor {
    scroll: usize,
    selected: usize,
    page_size: usize,
    highlight: bool,
}

fn draw_task_rows(
    frame: &mut Frame,
    app: &App,
    state: &crate::views::ViewState,
    rows: &[Row],
    cursor: &mut TableCursor,
    inner: Rect,
) -> Vec<(u16, usize)> {
    let theme = &app.config.theme;
    let registry = app.registry();
    let widths: Vec<Constraint> = state
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| match column {
            _ if state.glyph_columns.contains(column) => {
                Constraint::Length((2 + app.glyph_legend(*column).chars().count()).max(3) as u16)
            }
            column => match column.max_width(registry) {
                Some(max) => {
                    Constraint::Length(fitted_width(app, state, rows, *column, max, index + 1))
                }
                None => Constraint::Min(20),
            },
        })
        .collect();
    let header_area = Rect { height: 1, ..inner };
    let cells = crate::list_presentation::cells(header_area, &widths);
    let header_paint = paint::scoped_style(
        &state.paint,
        theme,
        &app.config.palette,
        paint::PaintScope::Header,
    );
    let header_style = theme.style(Surface::Header).patch(header_paint);
    // TASK-234: the number reads as the key that selects the column, so it
    // gets the same ink as other key hints (`Surface::Keys`) instead of
    // sharing the label's style.
    let key_style = theme.style(Surface::Keys).patch(header_paint);
    let headers: Vec<Line<'static>> = state
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let label = if state.glyph_columns.contains(column) {
                app.glyph_legend(*column)
            } else {
                column.header(registry).to_string()
            };
            crate::list_presentation::keyed_header(index, &label, key_style, header_style)
        })
        .collect();
    crate::list_presentation::header(frame, header_area, &cells, &headers, header_style);
    if let Some(sort) = state.sort {
        if let Some(index) = state
            .columns
            .iter()
            .position(|column| *column == sort.column)
        {
            frame.buffer_mut().set_style(
                cells[index],
                Style::default().add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
            );
        }
    }
    let heading =
        cursor.selected > 0 && matches!(rows.get(cursor.selected - 1), Some(Row::Heading { .. }));
    let title_width = state
        .columns
        .iter()
        .zip(cells.iter())
        .find(|(column, _)| **column == Column::Title)
        .map_or(0, |(_, cell)| cell.width);
    let row_height = |row: usize| -> usize {
        match rows.get(row) {
            Some(Row::Task(index)) => {
                let title = crate::row_layout::title_text(&app.tasks()[*index].title);
                usize::from(state.row_layout.title_height(&title.text, title_width))
                    + usize::from(
                        state.row_layout.spaced && (row != cursor.selected || !cursor.highlight),
                    )
            }
            _ => 1,
        }
    };
    let viewport = crate::list_presentation::ListViewport::variable(
        cursor.scroll,
        cursor.selected,
        rows.len(),
        inner.height.saturating_sub(1) as usize,
        heading,
        row_height,
    );
    cursor.scroll = viewport.scroll;
    let window = viewport.slots;
    let mut used = 0;
    let mut visible_tasks = 0;
    let mut list_rows: Vec<(u16, usize)> = Vec::new();
    let body = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    for (line, row) in rows.iter().skip(cursor.scroll).take(window).enumerate() {
        if used >= window {
            break;
        }
        let content_height = match row {
            Row::Heading { .. } => 1,
            Row::Task(index) => {
                visible_tasks += 1;
                let title = crate::row_layout::title_text(&app.tasks()[*index].title);
                state.row_layout.title_height(&title.text, title_width)
            }
        };
        let row_area = Rect {
            y: body.y + used as u16,
            height: content_height.min((window - used) as u16),
            ..body
        };
        if cursor.highlight {
            list_rows.push((row_area.y, cursor.scroll + line));
        }
        used += usize::from(content_height)
            + usize::from(matches!(row, Row::Task(_)) && state.row_layout.spaced);
        let selected = cursor.highlight && cursor.scroll + line == cursor.selected;
        match row {
            Row::Heading { text, depth, value } => frame.render_widget(
                Paragraph::new(format!("{}▸ {text}", "  ".repeat(*depth))).style(
                    theme.style(Surface::Heading).patch(paint::scoped_style(
                        &state.paint,
                        theme,
                        &app.config.palette,
                        paint::PaintScope::Heading(value),
                    )),
                ),
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
                for (column, cell) in state.columns.iter().zip(cells.iter()) {
                    let value =
                        column.cell_text(registry, task, &app.goals, &app.relations.blocked);
                    let text = if state.glyph_columns.contains(column) && !value.is_empty() {
                        app.config.glyph(*column, &value)
                    } else {
                        app.cell_for_view(state, *column, task)
                    };
                    let mut style = theme.column_style(*column);
                    let role = match column {
                        Column::Title if task.is_done() => "quiet",
                        Column::Priority if task.is_done() => "quiet",
                        Column::Title if task.priority.eq_ignore_ascii_case("high") => "strong",
                        Column::Priority if task.priority.eq_ignore_ascii_case("high") => "alert",
                        Column::Priority if task.priority.eq_ignore_ascii_case("low") => "quiet",
                        _ => "",
                    };
                    if let Some(emphasis) = theme.emphasis_style(role, &app.config.palette) {
                        style = style.patch(emphasis);
                    }
                    if blocked {
                        style = style.patch(theme.style(Surface::Hint));
                    }
                    style = style.patch(paint::cell_style(
                        &state.paint,
                        &app.config,
                        registry,
                        task,
                        *column,
                        &app.goals,
                        &app.relations.blocked,
                    ));
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
                    if *column == Column::Progress
                        && app.config.progress_style != crate::progress::ProgressStyle::Icons
                    {
                        frame.render_widget(
                            Paragraph::new(if cell_area.width < 6 {
                                ratatui::text::Line::styled(
                                    if app.config.progress_style
                                        == crate::progress::ProgressStyle::Ascii
                                    {
                                        crate::progress::compact_ascii(app.checklist.get(&task.id))
                                            .to_string()
                                    } else {
                                        crate::progress::compact_pill(app.checklist.get(&task.id))
                                            .to_string()
                                    },
                                    style,
                                )
                            } else {
                                crate::progress::pill(
                                    app.checklist.get(&task.id),
                                    theme,
                                    style,
                                    app.config.progress_style,
                                )
                            }),
                            Rect {
                                height: 1,
                                ..cell_area
                            },
                        );
                    } else if *column == Column::Title {
                        draw_task_title(frame, &text, state.row_layout, style, cell_area);
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
    cursor.page_size = visible_tasks.max(1);
    list_rows
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
fn fitted_width(
    app: &App,
    state: &crate::views::ViewState,
    rows: &[Row],
    column: Column,
    max: u16,
    position: usize,
) -> u16 {
    let header = column.header(app.registry()).chars().count() + position.to_string().len() + 1;
    let widest = rows
        .iter()
        .filter_map(|row| match row {
            Row::Task(index) => Some(
                app.cell_for_view(state, column, &app.tasks()[*index])
                    .chars()
                    .count(),
            ),
            Row::Heading { .. } => None,
        })
        .max()
        .unwrap_or(0);
    (header.max(widest) as u16).min(max.max(header as u16))
}

/// Every view setting (sort, shown columns, glyph mode, hidden statuses,
/// pin, row layout, paint rule count, outline and its initiatives, live
/// work count) as one `·`-joined summary. TASK-235 moved this off the list
/// title line — it did not fit once a filter grew past a few terms — into
/// the bottom hint bar (`browse_footer`); `None` when nothing is set, so
/// the caller adds no stray separator.
fn view_settings_summary(app: &App) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(sort) = app.state.sort {
        parts.push(sort.label(app.registry()));
    }
    if let Some(label) = app.state.columns_label(app.registry()) {
        parts.push(label);
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

    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The editable task detail pane (TASK-222): one line per `FieldRow`, in the
/// fixed order `crate::detail_pane::field_rows` defines, plus decorative
/// section headers and the live-work lines above them. The cursor row (while
/// `app.detail_focused()`) gets the selection style; scroll follows it into
/// view using the same wrapped-line accounting `draw_help` already uses for
/// its own scroll clamp.
fn draw_detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.config.theme.clone();
    let focused = app.detail_edit_focused();
    app.detail_viewport = area.height.saturating_sub(2);
    let Some(task) = app.selected_task().cloned() else {
        crate::detail_pane::draw(frame, &theme, area, vec![Line::from("nothing selected")], 0);
        app.detail_scroll = 0;
        app.detail_hit.row_starts.clear();
        return;
    };
    let rows = app.detail_rows();
    let DetailLines {
        lines,
        row_lines,
        section_lines,
    } = build_detail_lines(app, &task, &theme, focused, app.detail_cursor, &rows);
    let width = area.width.saturating_sub(2);
    app.detail_hit.row_starts = row_lines
        .iter()
        .map(|&line_index| wrapped_offset(&lines, line_index, width))
        .collect();
    app.detail_hit.section_starts = section_lines
        .into_iter()
        .map(|(line, section)| (wrapped_offset(&lines, line, width), section))
        .collect();
    if focused {
        adjust_detail_scroll(app, &lines, &row_lines, &rows, &task.id, area);
    }
    let scroll = app.detail_scroll;
    app.detail_scroll = crate::detail_pane::draw(frame, &theme, area, lines, scroll);
}

/// Every line the pane shows for `task`: the id/read-only header, live-work
/// lines, then one line per `FieldRow` (with a decorative section header
/// inserted before the first acceptance/blocked-by/blocks row, and the
/// description's own body lines directly beneath its row), styling the
/// cursor row when focused. `row_lines[n]` is the index into the returned
/// `lines` for `FieldRow` number `n`, since a section header or a body line
/// is not itself a navigable row.
struct DetailLines {
    lines: Vec<Line<'static>>,
    row_lines: Vec<usize>,
    section_lines: Vec<(usize, crate::detail_pane::Section)>,
}

fn build_detail_lines(
    app: &App,
    task: &switchbard_core::BacklogTask,
    theme: &Theme,
    focused: bool,
    cursor: usize,
    rows: &[FieldRow],
) -> DetailLines {
    let mut lines: Vec<Line> = Vec::new();
    let mut row_lines: Vec<usize> = Vec::new();
    let mut section_lines = Vec::new();
    let mut header = task.id.clone();
    if !task.editable() {
        header.push_str(" · read-only");
    }
    lines.push(crate::detail_pane::metadata(header, theme));
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
    let mut previous_section = None;
    for (index, row) in rows.iter().enumerate() {
        let section = row.section();
        if previous_section != Some(section) {
            section_lines.push((lines.len(), section));
            let marker = if app.detail_collapsed.contains(&section) {
                ">"
            } else {
                "v"
            };
            let mut style = theme.style(Surface::Accent);
            if focused && index == cursor && matches!(row, FieldRow::Section(_)) {
                style = theme.style(Surface::Selected).patch(style);
            }
            lines.push(Line::from(Span::styled(
                format!("{marker} {}", section.label()),
                style,
            )));
            previous_section = Some(section);
        }
        if matches!(row, FieldRow::Section(_)) {
            row_lines.push(lines.len() - 1);
            continue;
        }
        if let FieldRow::Content(section) = row {
            row_lines.push(lines.len());
            let style = if focused && index == cursor {
                theme.style(Surface::Selected)
            } else {
                theme.style(Surface::Hint)
            };
            for text in detail_content::lines(app, task, *section) {
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(line.to_string(), style)));
                }
            }
            continue;
        }
        let text = detail_row_text(app, task, *row);
        let mut style = match row {
            FieldRow::Description
            | FieldRow::Content(_)
            | FieldRow::BlockedBy(_)
            | FieldRow::Blocks(_) => theme.style(Surface::Hint),
            _ => Style::default(),
        };
        if *row == FieldRow::Title {
            style = style.add_modifier(Modifier::BOLD);
        }
        if focused && index == cursor {
            style = theme.style(Surface::Selected).patch(style);
        }
        lines.push(Line::from(Span::styled(text, style)));
        row_lines.push(lines.len() - 1);
        if *row == FieldRow::Description {
            for body_line in task.description.lines() {
                lines.push(Line::from(Span::styled(
                    body_line.to_string(),
                    theme.style(Surface::Text),
                )));
            }
        }
    }
    DetailLines {
        lines,
        row_lines,
        section_lines,
    }
}

/// Keep the cursor row's own line inside the viewport, scrolling up or down
/// the minimum needed — never resets `app.detail_scroll` outright, so it
/// also self-corrects after a resize without losing an in-view row.
///
/// The description row is the one exception: its body can run to many
/// times the viewport's height, directly beneath its own (one-line) row.
/// Once that row's line has been brought into view, `PageDown`/`PageUp`
/// (`App::page_detail_scroll`) or a mouse wheel may scroll on past it into
/// the body without this snapping back — tracked by `detail_scroll_anchor`,
/// which only forgets a manual scroll once the cursor actually leaves the
/// row (or a different task is selected).
fn adjust_detail_scroll(
    app: &mut App,
    lines: &[Line],
    row_lines: &[usize],
    rows: &[FieldRow],
    task_id: &str,
    area: Rect,
) {
    let Some(&line_index) = row_lines.get(app.detail_cursor) else {
        return;
    };
    let width = area.width.saturating_sub(2);
    let viewport = area.height.saturating_sub(2);
    let offset = wrapped_offset(lines, line_index, width);
    let row_height = wrapped_height(&lines[line_index], width);
    let anchor = (task_id.to_string(), app.detail_cursor);
    let settled = app.detail_scroll_anchor.as_ref() == Some(&anchor);
    app.detail_scroll_anchor = Some(anchor);
    let long_content = matches!(
        rows.get(app.detail_cursor),
        Some(FieldRow::Description | FieldRow::Content(_))
    );
    if offset < app.detail_scroll {
        if !(long_content && settled) {
            app.detail_scroll = offset;
        }
    } else if offset.saturating_add(row_height) > app.detail_scroll.saturating_add(viewport) {
        app.detail_scroll = offset + row_height - viewport;
    }
}

/// How many wrapped display lines `line` takes at `width` — used to keep the
/// detail pane's cursor row on screen (each `FieldRow` is exactly one
/// logical `Line`, but a long title wraps to several display rows).
fn wrapped_height(line: &Line<'_>, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    Paragraph::new(line.clone())
        .wrap(Wrap { trim: false })
        .line_count(width)
        .min(u16::MAX as usize) as u16
}

/// The wrapped display-line offset of `lines[upto]`: the sum of every prior
/// row's own wrapped height, since ratatui wraps each `Line` independently.
fn wrapped_offset(lines: &[Line<'_>], upto: usize, width: u16) -> u16 {
    lines[..upto]
        .iter()
        .map(|line| wrapped_height(line, width))
        .fold(0u16, u16::saturating_add)
}

/// `text` verbatim within `max_chars`; otherwise clipped with a trailing `…`
/// so the result never exceeds `max_chars`. Empty at `max_chars == 0`
/// (TASK-235: the footer drops the view-settings summary rather than crowd
/// the key hints it never truncates).
fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut clipped: String = text.chars().take(max_chars - 1).collect();
    clipped.push('…');
    clipped
}

/// The rendered text for one detail-pane row. Structured fields carry a
/// `label: value` prefix; the read-only rows (description header, blocked-by,
/// blocks) match the non-editable presentation the pane used before TASK-222.
/// The description's own body is separate — `build_detail_lines` appends it
/// as extra, non-navigable lines directly beneath this row's line.
fn detail_row_text(app: &App, task: &switchbard_core::BacklogTask, row: FieldRow) -> String {
    match row {
        FieldRow::Section(section) => section.label().to_string(),
        FieldRow::Content(section) => format!("{}:", section.label()),
        FieldRow::Title => task.title.clone(),
        FieldRow::Status => format!("status: {}", task.status),
        FieldRow::Planning => format!("planning: {}", task.planning),
        FieldRow::Checklist => format!(
            "checklist: {} (task + descendants)",
            app.checklist_text(task)
        ),
        FieldRow::Priority => format!("priority: {}", task.priority),
        FieldRow::Project => format!("project: {}", task.project.as_deref().unwrap_or("Not set")),
        FieldRow::DueDate => format!(
            "due date: {}",
            task.due_date.as_deref().unwrap_or("Not set")
        ),
        FieldRow::Labels => format!(
            "labels: {}",
            if task.labels.is_empty() {
                "Not set".to_string()
            } else {
                task.labels.join(", ")
            }
        ),
        FieldRow::Description => {
            if task.description.trim().is_empty() {
                "description: Not set".to_string()
            } else {
                "description:".to_string()
            }
        }
        FieldRow::Acceptance(index) => {
            let item = task.acceptance_criteria.get(index).expect(
                "invariant: FieldRow::Acceptance(index) only exists for index < \
                 acceptance_criteria.len() — detail_pane::field_rows built this list \
                 from the same task",
            );
            format!("[{}] {}", if item.checked { "x" } else { " " }, item.text)
        }
        FieldRow::BlockedBy(index) => {
            let (id, title) = app
                .relations
                .blocked_by
                .get(&task.id)
                .and_then(|deps| deps.get(index))
                .expect(
                    "invariant: FieldRow::BlockedBy(index) only exists for a task with that \
                     many blocked_by entries — detail_pane::field_rows built this list from \
                     the same relations",
                );
            format!("blocked by: {id} {title}")
        }
        FieldRow::Blocks(index) => {
            let (id, title, done) = app
                .relations
                .blocks
                .get(&task.id)
                .and_then(|deps| deps.get(index))
                .expect(
                    "invariant: FieldRow::Blocks(index) only exists for a task with that many \
                     blocks entries — detail_pane::field_rows built this list from the same \
                     relations",
                );
            format!(
                "blocks: {id} {title} ({})",
                if *done { "done" } else { "open" }
            )
        }
    }
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

fn help_entry(keys: &str, name: &str, theme: &crate::config::Theme) -> Line<'static> {
    let key_width = Span::raw(keys).width();
    let padding = " ".repeat(8_usize.saturating_sub(key_width).max(1));
    Line::from(vec![
        Span::styled(format!("{keys}{padding}"), theme.style(Surface::Accent)),
        Span::raw(name.to_string()),
    ])
}

fn help_entry_rows(
    entries: &[(String, String)],
    width: u16,
    theme: &crate::config::Theme,
) -> Vec<Line<'static>> {
    let columns = (usize::from(width) / 32).max(1);
    let cell_width = usize::from(width) / columns;
    let mut rows = Vec::new();
    let mut spans = Vec::new();
    let mut filled = 0;
    for (keys, name) in entries {
        let cell = help_entry(keys, name, theme);
        if cell.width() + 2 > cell_width {
            if !spans.is_empty() {
                rows.push(Line::from(std::mem::take(&mut spans)));
                filled = 0;
            }
            rows.push(cell);
            continue;
        }
        let padding = cell_width.saturating_sub(cell.width());
        spans.extend(cell.spans);
        spans.push(Span::raw(" ".repeat(padding)));
        filled += 1;
        if filled == columns {
            rows.push(Line::from(std::mem::take(&mut spans)));
            filled = 0;
        }
    }
    if !spans.is_empty() {
        rows.push(Line::from(spans));
    }
    rows
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
        .chain((app.page == Page::Tasks).then(|| {
            let keys = app
                .config
                .bindings_for(&Action::View)
                .iter()
                .map(|key| format!("{key} l"))
                .collect::<Vec<_>>()
                .join(" ");
            (keys, "cycle line wrap".to_string())
        }))
        .chain((app.page == Page::Tasks).then(|| {
            (
                format!("{} l / r", app.config.bindings_for(&Action::Rank).join(" ")),
                "planning / Planned order".to_string(),
            )
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
    let mut lines = help_entry_rows(&entries, area.width.saturating_sub(2), theme);
    lines.push(Line::from(""));
    for (command, description) in [
        (":bug <doing>", "file a bug with this screen"),
        (":idea <want>", "file an idea with this screen"),
        (":theme <name>", "how sbt itself looks"),
        (
            ":paint <rules>",
            "replace rules; + combines roles, ! stops; off clears",
        ),
        (
            "p <scope>",
            "highlight, then text: space adds a token, Enter applies, ← back",
        ),
        (":palette <name>", "colors `auto` paints with"),
        (":view <name>  :reload  :q", ""),
        ("f/s <col#>", "filter/sort by column"),
        (
            "/ … Tab",
            "complete a filter key or value; Esc keeps it as typed",
        ),
        ("v<n>", "open view; vs<n> save it (vsd = default)"),
        ("v h", "view history; Enter restores; vs<n> saves a slot"),
        (
            ":experiments",
            "enable, disable, keep or remove new features",
        ),
        (":update", "use the latest installed build when ready"),
        (
            "sbt --fresh",
            "launch saved default instead of last session",
        ),
    ] {
        lines.push(Line::from(vec![
            Span::styled(format!("{command}  "), theme.style(Surface::Accent)),
            Span::raw(description),
        ]));
    }
    if app.page == Page::Tasks {
        if app.experiments.is_enabled("task-edit-shortcut") {
            lines.push(help_entry("t e", "edit selected task", theme));
        }
        lines.push(help_entry(
            "t c c",
            "cancel task with confirmation; Esc keeps it",
            theme,
        ));
        lines.push(help_entry(
            "z/Z/A",
            "detail: toggle section / collapse all / expand all",
            theme,
        ));
    }
    if app.page == Page::PullRequests {
        lines.push(Line::from("PR fields: status/lifecycle, id, title, tasks, checks, review, merge, draft; PR views use .prs.lua files."));
    }
    lines.push(Line::from(Span::styled(
        "config ~/.switchbard/tui.lua (hot reload) · views ~/.switchbard/views.lua + views/<repo>.lua · events ~/.switchbard/tui-events.jsonl",
        theme.style(Surface::Hint),
    )));
    lines.push(Line::from(Span::styled(
        format!("build {}", switchbard_core::version_line()),
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
    if let Mode::DetailInput(kind) = app.mode {
        draw_detail_input(frame, app, area, kind);
        return;
    }
    let theme = &app.config.theme;
    let lines: Vec<Line> = match app.mode {
        Mode::Filter => {
            let mut lines = vec![Line::from(vec![
                Span::styled("/", theme.style(Surface::Accent)),
                Span::raw(app.filter_text().to_string()),
                Span::styled("▏", theme.style(Surface::Accent)),
            ])];
            if let Some(hint) = app.filter_completion_hint() {
                lines.push(filter_completion_line(theme, &hint, area.width));
            }
            lines
        }
        Mode::Command => {
            let mut lines = vec![Line::from(vec![
                Span::styled(":", theme.style(Surface::Accent)),
                Span::raw(app.input.clone()),
                Span::styled("▏", theme.style(Surface::Accent)),
                Span::styled(
                    format!("   {}", app.command_completions().join("  ")),
                    theme.style(Surface::Hint),
                ),
            ])];
            if !app.status.is_empty() {
                lines.push(Line::styled(app.status.clone(), theme.style(Surface::Status)));
            }
            lines
        }
        Mode::PickValue if app.picker.is_some() => {
            vec![Line::from(Span::styled(
                app.status.clone(),
                theme.style(Surface::Hint),
            ))]
        }
        Mode::NewTask | Mode::PickValue => vec![Line::from(Span::styled(
            app.status.clone(),
            theme.style(Surface::Status),
        ))],
        Mode::BallName => vec![Line::from(vec![
            Span::styled(" ball person: ", theme.style(Surface::Accent)),
            Span::raw(app.input.clone()),
            Span::styled("▏", theme.style(Surface::Accent)),
        ])],
        Mode::RenameView => vec![Line::from(vec![
            Span::styled(" view name: ", theme.style(Surface::Accent)),
            Span::raw(app.input.clone()),
            Span::styled("▏", theme.style(Surface::Accent)),
        ])],
        // Handled by `draw_detail_input` above; unreachable via this match.
        Mode::DetailInput(_) => vec![Line::default()],
        Mode::DetailFocus if !app.status.is_empty() => vec![Line::from(Span::styled(
            app.status.clone(),
            theme.style(Surface::Status),
        ))],
        Mode::DetailFocus => vec![Line::from(vec![
            Span::styled(" pane focused ", theme.style(Surface::Accent)),
            Span::styled(
                "j/k move · enter edit · space check · z fold · Z fold all · A expand all · esc back",
                theme.style(Surface::Hint),
            ),
        ])],
        Mode::Browse => vec![browse_footer(app, theme, area.width)],
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// The `/` editor's Tab-completion candidates, one line beneath the filter
/// text: bounded to what `filter_completion_hint` already capped, then
/// further trimmed so a narrow terminal never wraps or panics on width.
fn filter_completion_line(
    theme: &Theme,
    hint: &crate::app::FilterCompletionHint,
    width: u16,
) -> Line<'static> {
    let mut text = hint.items.join("  ");
    if hint.more > 0 {
        text.push_str(&format!("  +{} more", hint.more));
    }
    let room = usize::from(width).saturating_sub(1);
    if text.chars().count() > room {
        let truncated: String = text.chars().take(room.saturating_sub(1)).collect();
        text = format!("{truncated}…");
    }
    Line::from(Span::styled(format!(" {text}"), theme.style(Surface::Hint)))
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

/// The detail pane's single-line field capture (title, due date, a new
/// label): the draft on one line, `app.status` (a validation or save error)
/// on the next — unlike `Mode::Filter`/`Mode::BallName`'s single line, a
/// rejected save here must stay visible next to the draft that caused it.
fn draw_detail_input(frame: &mut Frame, app: &App, area: Rect, kind: crate::app::DetailInputKind) {
    let theme = &app.config.theme;
    let prefix = format!(" {}: ", kind.label());
    let room = usize::from(area.width).saturating_sub(prefix.chars().count() + 1);
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
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// The footer while browsing: the status message from the last action, or key
/// hints when there isn't one, then (Tasks only, TASK-235)
/// [`view_settings_summary`] — row layout, hidden statuses, outline, pin,
/// glyphs, paint count, working count, sort, shown columns — which used to
/// pack onto the list title line. A status message used to hide that summary
/// outright, since it took over this whole line; it now gets its own trailing
/// span instead, always muted (`Surface::Hint`) and truncated from the right
/// with an ellipsis when `width` is too narrow, so the primary text (status
/// or key hints) never loses room to it.
fn browse_footer(app: &App, theme: &Theme, width: u16) -> Line<'static> {
    let (primary, primary_style) = if !app.status.is_empty() {
        (app.status.clone(), theme.style(Surface::Status))
    } else {
        (browse_hints(app), theme.style(Surface::Hint))
    };
    let mut spans = vec![Span::styled(primary.clone(), primary_style)];
    if app.page == Page::Tasks {
        if let Some(settings) = view_settings_summary(app) {
            let separator = "   ";
            let budget = usize::from(width)
                .saturating_sub(primary.chars().count())
                .saturating_sub(separator.len());
            let settings = truncate_with_ellipsis(&settings, budget);
            if !settings.is_empty() {
                spans.push(Span::styled(
                    format!("{separator}{settings}"),
                    theme.style(Surface::Hint),
                ));
            }
        }
    }
    Line::from(spans)
}

/// The key-hint text `browse_footer` shows when there is no status message:
/// the page's own action keys, plus the split/focus hints once a detail pane
/// is open.
fn browse_hints(app: &App) -> String {
    let actions = if app.page == Page::Inbox {
        vec![(Action::Page, "page"), (Action::Help, "keys")]
    } else if app.page == Page::Agents {
        vec![
            (Action::KillAgent, "kill"),
            (Action::Open, "detail"),
            (Action::Reload, "poll"),
            (Action::Page, "page"),
            (Action::Help, "keys"),
        ]
    } else if app.page == Page::Tasks {
        vec![
            (Action::Rank, "tasks"),
            (Action::View, "views"),
            (Action::Help, "keys"),
        ]
    } else {
        vec![(Action::View, "views"), (Action::Help, "keys")]
    };
    let mut text = actions
        .iter()
        .map(|(action, label)| format!("{} {label}", app.config.bindings_for(action).join("/")))
        .collect::<Vec<_>>()
        .join(" · ");
    if app.page.has_list_view() && app.pane == Pane::Detail {
        text.push_str(if app.detail_focused() {
            " · Detail active"
        } else {
            " · List active"
        });
        if app.page == Page::Tasks {
            text.push_str(" · enter/l edits");
        }
        text.push_str(&format!(
            " · {} focus",
            app.config.bindings_for(&Action::FocusPane).join("/")
        ));
    }
    text
}

fn draw_picker(frame: &mut Frame, app: &mut App, picker: &ValuePicker, body: Rect) {
    if picker.purpose == PickerPurpose::Experiments {
        experiment_picker::draw(frame, app, picker, body);
        return;
    }
    if picker.purpose == PickerPurpose::History {
        history_picker::draw(frame, app, picker, body);
        return;
    }
    let theme = &app.config.theme;
    let hint = picker::hint(picker);
    let labels: Vec<String> = picker
        .options
        .iter()
        .map(|option| {
            if let (PickerPurpose::ChooseColumn(ColumnPurpose::Filter), Payload::Column(column)) =
                (&picker.purpose, &option.payload)
            {
                if let Some(badge) = app.column_filter_badge(*column) {
                    return format!("{} {badge}", option.label);
                }
            }
            option.label.clone()
        })
        .collect();
    let width = labels
        .iter()
        .map(|label| label.chars().count() + 11)
        .chain(std::iter::once(hint.chars().count() + 4))
        .max()
        .unwrap_or(20)
        .max(60)
        .min(body.width.saturating_sub(4) as usize) as u16;
    let width = if matches!(
        picker.purpose,
        PickerPurpose::Merge
            | PickerPurpose::TaskCancel
            | PickerPurpose::AgentKill
            | PickerPurpose::Experiment(_)
    ) {
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
            let value = picker
                .options
                .iter()
                .position(|candidate| candidate == option)
                .map(|position| labels[position].as_str())
                .unwrap_or(&option.label);
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
                (PickerPurpose::TaskParent(id), Payload::Parent(parent)) => app
                    .tasks()
                    .iter()
                    .any(|task| task.id == *id && task.parent == *parent),
                (PickerPurpose::TaskProject(id), Payload::Project(project)) => app
                    .tasks()
                    .iter()
                    .any(|task| task.id == *id && task.project == *project),
                (
                    PickerPurpose::TaskPlanning(id) | PickerPurpose::DetailPlanning(id),
                    Payload::Text(planning),
                ) => app.tasks().iter().any(|task| {
                    task.id == *id && task.planning.as_str().eq_ignore_ascii_case(planning)
                }),
                (PickerPurpose::TaskStatus(id), Payload::Text(status)) => app
                    .tasks()
                    .iter()
                    .any(|task| task.id == *id && task.status.eq_ignore_ascii_case(status)),
                (PickerPurpose::DetailStatus(id), Payload::Text(status)) => app
                    .tasks()
                    .iter()
                    .any(|task| task.id == *id && task.status.eq_ignore_ascii_case(status)),
                (PickerPurpose::DetailPriority(id), Payload::Text(priority)) => app
                    .tasks()
                    .iter()
                    .any(|task| task.id == *id && task.priority.eq_ignore_ascii_case(priority)),
                (PickerPurpose::DetailProject(id), Payload::Project(project)) => app
                    .tasks()
                    .iter()
                    .any(|task| task.id == *id && task.project == *project),
                (PickerPurpose::DetailLabels(id), Payload::Text(label)) => {
                    app.tasks().iter().any(|task| {
                        task.id == *id && task.labels.iter().any(|l| l.eq_ignore_ascii_case(label))
                    })
                }
                (PickerPurpose::MoveColumns(placed), _) => placed.contains(&(index + 1)),
                (PickerPurpose::PaintRules, Payload::Rule(rule)) => *rule == 0,
                (PickerPurpose::PaintValues(column), Payload::Text(value)) => {
                    paint::value_color(&app.state.paint, *column, value).is_some()
                }
                // Both style steps mark what the draft holds, which starts as
                // the rule the scope already wears.
                (
                    PickerPurpose::PaintHighlight | PickerPurpose::PaintText,
                    Payload::Text(token),
                ) => app.paint_draft_holds(&picker.purpose, Some(token)),
                (PickerPurpose::PaintHighlight | PickerPurpose::PaintText, Payload::NoColor) => {
                    app.paint_draft_holds(&picker.purpose, None)
                }
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
                // Every row of either step is drawn as the cell would look if
                // that row were picked: its own token composed with the rest of
                // the draft, so a swatch shows the fill under the drafted ink.
                (
                    PickerPurpose::PaintHighlight | PickerPurpose::PaintText,
                    Payload::Text(token),
                ) => {
                    if let Some(emphasis) = app
                        .paint_row_preview(&picker.purpose, Some(token))
                        .and_then(|roles| theme.emphasis_style(&roles, &app.config.palette))
                    {
                        style = style.patch(emphasis);
                    }
                }
                (PickerPurpose::PaintHighlight | PickerPurpose::PaintText, Payload::NoColor) => {
                    if let Some(emphasis) = app
                        .paint_row_preview(&picker.purpose, None)
                        .and_then(|roles| theme.emphasis_style(&roles, &app.config.palette))
                    {
                        style = style.patch(emphasis);
                    }
                }
                (PickerPurpose::PaintValues(column), Payload::Text(value)) => {
                    if let Some(emphasis) = paint::value_color(&app.state.paint, *column, value)
                        .and_then(|roles| theme.emphasis_style(&roles, &app.config.palette))
                    {
                        style = style.patch(emphasis);
                    }
                }
                (PickerPurpose::PaintRules, Payload::Rule(rule)) => {
                    if let Some(roles) = app
                        .state
                        .paint
                        .get(*rule)
                        .and_then(|rule| rule.role_lists().first().copied())
                    {
                        style =
                            style.patch(paint::resolve_style(roles, theme, &app.config.palette));
                    }
                }
                _ => {}
            }
            let mark = if shown { "✓" } else { " " };
            Line::from(vec![
                Span::styled(
                    format!(
                        "{:<2}",
                        if matches!(picker.purpose, PickerPurpose::TaskParent(_)) {
                            String::new()
                        } else {
                            keys.get(index).cloned().unwrap_or_default()
                        }
                    ),
                    theme.style(Surface::Accent),
                ),
                Span::styled(
                    format!(
                        "{mark}{value:<width$}",
                        width = (width as usize).saturating_sub(9)
                    ),
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
    // The title previews the composed cell live: what has been typed while it
    // is a valid rule, else the fill and ink drafted so far.
    let previewed = match picker.purpose {
        PickerPurpose::PaintHighlight | PickerPurpose::PaintText => app.paint_preview(picker),
        _ => None,
    };
    let preview = previewed
        .as_ref()
        .and_then(|roles| theme.emphasis_style(roles, &app.config.palette));
    let title_style = preview.unwrap_or_default();
    let previewed = previewed.filter(|_| preview.is_some());
    let block = Block::default()
        .style(theme.canvas_style())
        .borders(Borders::ALL)
        .border_style(theme.style(Surface::Accent))
        .title_style(title_style)
        .title(
            pending
                + &picker_title(
                    picker,
                    previewed.as_deref(),
                    app.registry(),
                    app.legacy_order,
                ),
        );
    let block = if !matches!(
        picker.purpose,
        PickerPurpose::Merge | PickerPurpose::TaskCancel | PickerPurpose::AgentKill
    ) && rows.len().saturating_add(4) > height as usize
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
    if matches!(
        picker.purpose,
        PickerPurpose::Merge | PickerPurpose::TaskCancel | PickerPurpose::AgentKill
    ) {
        let confirmation_lines = if picker.purpose == PickerPurpose::AgentKill {
            app.agent_kill.confirmation_lines()
        } else if picker.purpose == PickerPurpose::TaskCancel {
            app.task_cancel.confirmation_lines()
        } else {
            app.pr_merge.confirmation_lines()
        };
        let mut confirmation: Vec<Line> = confirmation_lines.into_iter().map(Line::from).collect();
        confirmation.push(Line::from(""));
        confirmation.extend(lines);
        let area = Rect {
            height: body.height.saturating_sub(2),
            ..area
        };
        let visible = crate::list_presentation::picker(
            frame,
            area,
            block,
            confirmation,
            picker.selected,
            true,
        );
        if picker.purpose == PickerPurpose::AgentKill {
            app.agent_kill.confirmation_visible = visible;
        } else if picker.purpose == PickerPurpose::TaskCancel {
            app.task_cancel.confirmation_visible = visible;
        } else {
            app.pr_merge.confirmation_visible = visible;
        }
    } else {
        crate::list_presentation::picker(frame, area, block, lines, picker.selected, false);
    }
}

/// What is being picked, plus any typed text. Key hints live in the footer.
fn picker_title(
    picker: &ValuePicker,
    previewed: Option<&str>,
    registry: &crate::columns::ColumnRegistry,
    legacy_order: bool,
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
        PickerPurpose::PaintRowValues => "selected row values".to_string(),
        PickerPurpose::PaintHeadings => "paint outline heading".to_string(),
        PickerPurpose::PaintHighlight => "highlight · step 1 of 2".to_string(),
        PickerPurpose::PaintText => "text · step 2 of 2".to_string(),
        PickerPurpose::PaintRules => "paint rules · top is the base".to_string(),
        PickerPurpose::ChoosePaintRule(action) => format!("{action:?} paint rule"),
        PickerPurpose::ColumnActions(column) => column.name(registry).to_string(),
        PickerPurpose::Settings => "settings".to_string(),
        PickerPurpose::Experiments => "Experiments".to_string(),
        PickerPurpose::Experiment(id) => crate::experiments::catalog()
            .iter()
            .find(|spec| spec.id == id)
            .map_or_else(
                || "Experiment".to_string(),
                |spec| format!("E{:03} · {}", spec.number, spec.title),
            ),
        PickerPurpose::Goals(id) => format!("{id} · goals"),
        PickerPurpose::Organize => "organize by".to_string(),
        PickerPurpose::Ball => "ball".to_string(),
        PickerPurpose::Merge => "Confirm PR merge".to_string(),
        PickerPurpose::TaskCancel => "Cancel task?".to_string(),
        PickerPurpose::AgentKill => "Signal selected agent?".to_string(),
        PickerPurpose::Task => "task".to_string(),
        PickerPurpose::TopList => if legacy_order {
            "task · top list"
        } else {
            "task · Planned order"
        }
        .to_string(),
        PickerPurpose::TaskParent(id) => format!("{id} · parent"),
        PickerPurpose::TaskProject(id) => format!("{id} · project"),
        PickerPurpose::TaskPlanning(id) | PickerPurpose::DetailPlanning(id) => {
            format!("{id} · planning")
        }
        PickerPurpose::TaskStatus(id) => format!("{id} · status"),
        PickerPurpose::DetailStatus(id) => format!("{id} · status"),
        PickerPurpose::DetailPriority(id) => format!("{id} · priority"),
        PickerPurpose::DetailProject(id) => format!("{id} · project"),
        PickerPurpose::DetailLabels(id) => format!("{id} · labels"),
        PickerPurpose::Views => "views".to_string(),
        PickerPurpose::History => "view history".to_string(),
        PickerPurpose::SaveView => "save view".to_string(),
        PickerPurpose::GlobalView => "make view global".to_string(),
        PickerPurpose::RenameView => "name which view".to_string(),
        PickerPurpose::DeleteView => "delete which view".to_string(),
        PickerPurpose::ChooseColumnAction(action) => action.label().to_string(),
    };
    // A preview never replaces the step it belongs to: at 40 columns the title
    // truncates, and which step this is has to survive that.
    let step = match picker.purpose {
        PickerPurpose::PaintHighlight => "highlight · ",
        PickerPurpose::PaintText => "text · ",
        _ => "",
    };
    match previewed {
        Some(roles) => format!(" {step}{roles} ← this is how it looks · enter applies "),
        None if picker.typed.is_empty() => format!(" {subject} "),
        None => format!(" {subject}: {}▏", picker.typed),
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
