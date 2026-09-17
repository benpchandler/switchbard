//! PR list and right-hand detail, rendered only from cached observations.
use crate::{
    app::{App, Pane},
    config::Surface,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use switchbard_core::PrListRow;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.pane == Pane::Detail {
        let [left, right] = crate::detail_pane::split(area);
        draw_list(frame, app, left);
        detail(frame, app, right);
    } else {
        draw_list(frame, app, area);
    }
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(ratatui::text::Span::styled(
            " Pull Requests ",
            app.config
                .theme
                .style(Surface::Title)
                .patch(crate::paint::scoped_style(
                    &app.state.paint,
                    &app.config.theme,
                    &app.config.palette,
                    crate::paint::PaintScope::Title,
                )),
        ))
        .border_style(app.config.theme.style(Surface::Border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [status, body] = Layout::vertical([
        Constraint::Length(if area.width < 70 { 5 } else { 4 }),
        Constraint::Min(0),
    ])
    .areas(inner);
    draw_observation(frame, app, status);
    list(frame, app, body);
}

fn draw_observation(frame: &mut Frame, app: &App, area: Rect) {
    let observation = observation(app);
    let style = app.config.theme.style(Surface::Hint);
    let Some(snapshot) = &app.pull_requests.snapshot else {
        frame.render_widget(
            Paragraph::new(observation)
                .wrap(Wrap { trim: false })
                .style(style),
            area,
        );
        return;
    };
    let health = if app
        .pull_requests
        .observation_stale(app.config.pr_refresh_seconds)
    {
        "STALE"
    } else {
        "Observed"
    };
    let suffix = format!(" · {health} {}", app.pull_requests.refresh_label());
    let [header, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    let timer_width = Line::from(suffix.as_str()).width() as u16;
    let repository_width = (Line::from(snapshot.repository.as_str()).width() as u16)
        .min(header.width.saturating_sub(timer_width));
    let [repository, timer, _] = Layout::horizontal([
        Constraint::Length(repository_width),
        Constraint::Length(timer_width),
        Constraint::Min(0),
    ])
    .areas(header);
    frame.render_widget(
        Paragraph::new(snapshot.repository.as_str()).style(style),
        repository,
    );
    frame.render_widget(Paragraph::new(suffix).style(style), timer);
    frame.render_widget(
        Paragraph::new(observation)
            .wrap(Wrap { trim: false })
            .style(style),
        body,
    );
}

fn observation(app: &App) -> String {
    let prs = &app.pull_requests;
    let Some(snapshot) = &prs.snapshot else {
        return match &prs.error {
            Some(error) if error.starts_with("Cannot run gh:") => format!(
                "{} · GitHub CLI is optional; install gh for PRs, then use refresh to retry.",
                prs.refresh_label()
            ),
            Some(error) => format!(
                "{} · Unavailable: {error}. Use refresh to retry.",
                prs.refresh_label()
            ),
            None => format!("{} · Loading pull requests...", prs.refresh_label()),
        };
    };
    let coverage = if snapshot.truncated {
        if snapshot.limit < switchbard_core::MAX_PULL_REQUESTS {
            "PARTIAL · :more"
        } else {
            "PARTIAL · fetch cap"
        }
    } else {
        "all loaded"
    };
    let suffix = prs
        .error
        .as_deref()
        .or(snapshot.enrichment_warning.as_deref())
        .unwrap_or("");
    format!(
        "{}/{} shown · {}\nfilter: {}{}\n{}",
        prs.visible.len(),
        snapshot.rows.len(),
        coverage,
        if prs.filter.is_empty() {
            "all"
        } else {
            &prs.filter
        },
        {
            let mut active = String::new();
            if !app.state.sort.is_empty() {
                active.push_str(&format!(
                    " · {}",
                    crate::sort::stack_label(&app.state.sort, app.registry())
                ));
            }
            if !app.state.paint.is_empty() {
                active.push_str(&format!(" · paint:{}", app.state.paint.len()));
            }
            active
        },
        suffix
    )
}

fn list(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(snapshot) = &app.pull_requests.snapshot else {
        return;
    };
    if app.pull_requests.visible.is_empty() {
        let text = if snapshot.truncated {
            "No matches in loaded PRs; history is incomplete."
        } else if app.pull_requests.error.is_some() {
            "No matches in stale observations. Current state unknown."
        } else {
            "No PRs match the current filter."
        };
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        return;
    }
    let selected = app.pull_requests.selected;
    let viewport = crate::list_presentation::ListViewport::new(
        app.pull_requests.scroll,
        selected,
        app.pull_requests.visible.len(),
        area.height.saturating_sub(1) as usize,
        false,
    );
    app.pull_requests.scroll = viewport.scroll;
    let slots = viewport.slots;
    let header = Rect {
        height: area.height.min(1),
        ..area
    };
    let headers: Vec<String> = app
        .state
        .columns
        .iter()
        .map(|column| {
            if *column == crate::columns::Column::Checks && area.width < 70 {
                "Ck".to_string()
            } else {
                column.header(app.registry()).to_string()
            }
        })
        .collect();
    draw_cells(frame, app, header, &headers);
    for (offset, index) in app
        .pull_requests
        .visible
        .iter()
        .skip(app.pull_requests.scroll)
        .take(slots)
        .enumerate()
    {
        let row = &snapshot.rows[*index];
        let rect = Rect {
            y: area.y + 1 + offset as u16,
            height: 1,
            ..area
        };
        draw_row(
            frame,
            app,
            &app.state,
            row,
            rect,
            offset + app.pull_requests.scroll == selected,
        );
    }
}

fn draw_cells(frame: &mut Frame, app: &App, rect: Rect, labels: &[String]) {
    let widths = column_widths(app, &app.state, rect.width);
    let cells = crate::list_presentation::cells(rect, &widths);
    let header_paint = crate::paint::scoped_style(
        &app.state.paint,
        &app.config.theme,
        &app.config.palette,
        crate::paint::PaintScope::Header,
    );
    let header_style = app.config.theme.style(Surface::Header).patch(header_paint);
    // TASK-234: same split as the Tasks header — the number reads as the key
    // that selects the column, so it gets the `keys` ink instead of sharing
    // the label's style (one convention, one helper: list_presentation).
    let key_style = app.config.theme.style(Surface::Keys).patch(header_paint);
    let headers: Vec<Line<'static>> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            crate::list_presentation::keyed_header(index, label, key_style, header_style)
        })
        .collect();
    crate::list_presentation::header(frame, rect, &cells, &headers, header_style);
    for sort in &app.state.sort {
        if let Some(index) = app
            .state
            .columns
            .iter()
            .position(|column| *column == sort.column)
        {
            frame.buffer_mut().set_style(
                cells[index],
                ratatui::style::Style::default().add_modifier(
                    ratatui::style::Modifier::UNDERLINED | ratatui::style::Modifier::BOLD,
                ),
            );
        }
    }
}

pub(crate) fn column_widths(
    app: &App,
    state: &crate::views::ViewState,
    width: u16,
) -> Vec<Constraint> {
    state
        .columns
        .iter()
        .map(|column| match column {
            crate::columns::Column::Id if !app.pull_requests.selection.marked.is_empty() => {
                Constraint::Length(11)
            }
            crate::columns::Column::Id => Constraint::Length(7),
            crate::columns::Column::Tasks if width < 70 => Constraint::Length(10),
            crate::columns::Column::Checks if width < 70 => Constraint::Length(4),
            column => column
                .max_width(app.registry())
                .map(Constraint::Length)
                .unwrap_or(Constraint::Min(1)),
        })
        .collect()
}

pub(crate) fn draw_row(
    frame: &mut Frame,
    app: &App,
    state: &crate::views::ViewState,
    row: &PrListRow,
    rect: Rect,
    selected: bool,
) {
    let links = app
        .pull_requests
        .links
        .get(&row.url)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let texts: Vec<String> = state
        .columns
        .iter()
        .map(|column| {
            let values = column.pr_values(row, links);
            match column {
                crate::columns::Column::Id if !app.pull_requests.selection.marked.is_empty() => {
                    let mark = if app.pull_requests.is_marked(row) {
                        "[x]"
                    } else {
                        "[ ]"
                    };
                    format!("{mark} #{}", row.number)
                }
                crate::columns::Column::Id => format!("#{}", row.number),
                // TASK-204: GitHub's list read is eventually consistent, so a
                // just-confirmed merge still reads Open here for a while;
                // name that explicitly rather than showing a stale state.
                crate::columns::Column::Lifecycle if app.pull_requests.expecting_merge(row) => {
                    "merging".to_string()
                }
                crate::columns::Column::Tasks if values.len() > 1 => {
                    format!("{} linked", values.len())
                }
                crate::columns::Column::Tasks if values.is_empty() => "-".to_string(),
                crate::columns::Column::Checks
                    if row.lifecycle != switchbard_core::PrLifecycle::Open =>
                {
                    "NF".to_string()
                }
                crate::columns::Column::Checks
                    if rect.width < 70 && !state.glyph_columns.contains(column) =>
                {
                    match row.checks {
                        switchbard_core::PrChecks::Failed => "!",
                        switchbard_core::PrChecks::Running => "~",
                        switchbard_core::PrChecks::Passing => "+",
                        _ => "?",
                    }
                    .to_string()
                }
                _ if state.glyph_columns.contains(column) => {
                    app.config.glyph(*column, &values.join(","))
                }
                _ => values.join(","),
            }
        })
        .collect();
    let style = app.config.theme.style(if selected {
        Surface::Selected
    } else {
        Surface::Text
    });
    frame.render_widget(Paragraph::new("").style(style), rect);
    let cells = crate::list_presentation::cells(rect, &column_widths(app, state, rect.width));
    for ((column, text), cell) in state.columns.iter().zip(&texts).zip(cells.iter()) {
        let mut style = app.config.theme.column_style(*column);
        style = style.patch(crate::paint::cell_style_with(
            &state.paint,
            &app.config.palette,
            &app.config.theme,
            *column,
            app.registry(),
            |column| app.pull_requests.values(column, row),
            |filter| app.pull_requests.matches(filter, row),
        ));
        if selected {
            style = style.patch(app.config.theme.style(Surface::Selected));
        }
        frame.render_widget(Paragraph::new(text.as_str()).style(style), *cell);
    }
}

fn detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let lines = detail_lines(app);
    app.pull_requests.detail_scroll = crate::detail_pane::draw(
        frame,
        &app.config.theme,
        area,
        lines,
        app.pull_requests.detail_scroll,
    );
}

fn detail_lines(app: &App) -> Vec<Line<'static>> {
    let Some(row) = app.pull_requests.row() else {
        return vec![Line::from("nothing selected")];
    };
    let theme = &app.config.theme;
    let mut lines = vec![
        crate::detail_pane::title(row.title.clone()),
        crate::detail_pane::metadata(
            format!(
                "#{} · {}{}",
                row.number,
                row.lifecycle.label(),
                if row.draft { " · draft" } else { "" }
            ),
            theme,
        ),
        Line::from(""),
        Line::from(row.url.clone()),
        Line::from(""),
        crate::detail_pane::section("checks and review", theme),
        Line::from(format!(
            "Checks: {}",
            app.pull_requests
                .values(crate::columns::Column::Checks, row)
                .join(",")
        )),
        Line::from(format!("Review: {}", row.review.label())),
        Line::from(format!("Merge: {}", row.merge.label())),
        Line::from(format!("Head: {}", row.head_oid)),
        Line::from(""),
        crate::detail_pane::section("linked tasks", theme),
    ];
    append_links(&mut lines, app, row);
    lines.push(Line::from(""));
    lines.push(crate::detail_pane::metadata(
        "Required-check coverage unknown. Checks do not prove task completion or continued progress.".into(), theme,
    ));
    lines
}

fn append_links(lines: &mut Vec<Line<'static>>, app: &App, row: &PrListRow) {
    if let Some(links) = app.pull_requests.links.get(&row.url) {
        lines.extend(
            links
                .iter()
                .take(20)
                .map(|(id, title)| Line::from(format!("{id} · {title}"))),
        );
        if links.len() > 20 {
            lines.push(Line::from(format!(
                "{} more linked tasks",
                links.len() - 20
            )));
        }
    } else {
        lines.push(Line::from("No matching PR reference in loaded tasks"));
    }
}
