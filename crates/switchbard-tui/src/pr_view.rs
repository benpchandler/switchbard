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
use switchbard_core::{PrChecks, PrListRow};

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
        .title(" Pull Requests ")
        .border_style(app.config.theme.style(Surface::Border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [status, body] = Layout::vertical([
        Constraint::Length(if area.width < 70 { 5 } else { 4 }),
        Constraint::Min(0),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(observation(app))
            .wrap(Wrap { trim: false })
            .style(app.config.theme.style(Surface::Hint)),
        status,
    );
    list(frame, app, body);
}

fn observation(app: &App) -> String {
    let prs = &app.pull_requests;
    let Some(snapshot) = &prs.snapshot else {
        return match &prs.error {
            Some(error) => format!("Unavailable: {error}. Use refresh to retry."),
            None => "Loading pull requests...".into(),
        };
    };
    let age = snapshot.observed_at.elapsed().map_or(0, |d| d.as_secs());
    let health = if prs.error.is_some() || age >= app.config.pr_refresh_seconds * 2 {
        "STALE"
    } else {
        "Observed"
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
        .unwrap_or(if prs.loading() { "refreshing" } else { "" });
    format!(
        "{} · {} {}s\n{}/{} shown · {}\nfilter: {}\n{}",
        snapshot.repository,
        health,
        age,
        prs.visible.len(),
        snapshot.rows.len(),
        coverage,
        if prs.filter.is_empty() {
            "all"
        } else {
            &prs.filter
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
    let slots = area.height.saturating_sub(1) as usize;
    let selected = app.pull_requests.selected;
    app.pull_requests.scroll = app.pull_requests.scroll.min(selected);
    if slots > 0 && selected >= app.pull_requests.scroll + slots {
        app.pull_requests.scroll = selected + 1 - slots;
    }
    let header = Rect {
        height: area.height.min(1),
        ..area
    };
    draw_cells(
        frame,
        header,
        [
            "PR",
            "State",
            "Tasks",
            if area.width < 70 { "Ck" } else { "Checks" },
            "Title",
        ],
        app.config.theme.style(Surface::Header),
    );
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
            row,
            rect,
            offset + app.pull_requests.scroll == selected,
        );
    }
}

fn draw_cells(frame: &mut Frame, rect: Rect, texts: [&str; 5], style: ratatui::style::Style) {
    let compact = rect.width < 70;
    let widths = [
        Constraint::Length(7),
        Constraint::Length(6),
        Constraint::Length(10),
        Constraint::Length(if compact { 2 } else { 7 }),
        Constraint::Min(1),
    ];
    for (text, cell) in texts
        .iter()
        .zip(Layout::horizontal(widths).spacing(1).split(rect).iter())
    {
        frame.render_widget(Paragraph::new(*text).style(style), *cell);
    }
}

fn draw_row(frame: &mut Frame, app: &App, row: &PrListRow, rect: Rect, selected: bool) {
    let links = app
        .pull_requests
        .links
        .get(&row.url)
        .map(|v| {
            if v.len() > 1 {
                format!("{} linked", v.len())
            } else {
                v[0].0.clone()
            }
        })
        .unwrap_or_else(|| "-".into());
    let signal = if rect.width < 70 {
        checks(row, true)
    } else if row.lifecycle != switchbard_core::PrLifecycle::Open {
        "NF"
    } else if row.checks == PrChecks::NoneObserved {
        "None"
    } else {
        checks(row, false)
    };
    let identity = format!("#{}", row.number);
    let style = app.config.theme.style(if selected {
        Surface::Selected
    } else {
        Surface::Text
    });
    draw_cells(
        frame,
        rect,
        [&identity, row.lifecycle.label(), &links, signal, &row.title],
        style,
    );
}

fn checks(row: &PrListRow, compact: bool) -> &'static str {
    if row.lifecycle != switchbard_core::PrLifecycle::Open {
        return if compact { "NF" } else { "Not fetched" };
    }
    match (row.checks, compact) {
        (PrChecks::Failed, true) => "!",
        (PrChecks::Unknown | PrChecks::NoneObserved, true) => "?",
        (PrChecks::Running, true) => "~",
        (PrChecks::Passing, true) => "+",
        (PrChecks::Failed, false) => "Failed",
        (PrChecks::Unknown, false) => "Unknown",
        (PrChecks::NoneObserved, false) => "None observed",
        (PrChecks::Running, false) => "Pending",
        (PrChecks::Passing, false) => "Passed",
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
        Line::from(format!("Checks: {}", checks(row, false))),
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
