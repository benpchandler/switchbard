//! PR list and right-hand detail, rendered only from cached observations.
use crate::{
    app::{App, Pane},
    config::Surface,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use switchbard_core::{PrChecks, PrListRow};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
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
    if app.pane == Pane::Detail {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(body);
        list(frame, app, left);
        let detail_block = Block::default()
            .borders(Borders::ALL)
            .title(" PR details ")
            .border_style(app.config.theme.style(Surface::Border));
        let detail_area = detail_block.inner(right);
        frame.render_widget(detail_block, right);
        detail(frame, app, detail_area);
    } else {
        list(frame, app, body);
    }
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
            "Status",
            "Linked tasks",
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
        Constraint::Length(7),
        Constraint::Length(if compact { 12 } else { 16 }),
        Constraint::Length(if compact { 2 } else { 18 }),
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
                format!("{} +{}", v[0].0, v.len() - 1)
            } else {
                v[0].0.clone()
            }
        })
        .unwrap_or_else(|| {
            if rect.width < 70 {
                "-"
            } else {
                "No recorded link"
            }
            .into()
        });
    let signal = checks(row, rect.width < 70);
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
    let Some(row) = app.pull_requests.row() else {
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!("#{} {}", row.number, row.title),
            app.config.theme.style(Surface::Heading),
        )),
        Line::from(row.url.clone()),
        Line::from(format!(
            "{}{}",
            row.lifecycle.label(),
            if row.draft { " (draft)" } else { "" }
        )),
        Line::from(format!("Checks: {}", checks(row, false))),
        Line::from(format!("Review: {}", row.review.label())),
        Line::from(format!("Merge: {}", row.merge.label())),
        Line::from(format!("Head: {}", row.head_oid)),
        Line::from("Linked tasks (loaded task references):"),
    ];
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
    lines.push(Line::from(
        "Required-check coverage unknown. Checks do not prove task completion or continued progress.",
    ));
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .style(app.config.theme.style(Surface::Text));
    let max_scroll = paragraph
        .line_count(area.width)
        .saturating_sub(area.height as usize)
        .min(u16::MAX as usize) as u16;
    app.pull_requests.detail_scroll = app.pull_requests.detail_scroll.min(max_scroll);
    frame.render_widget(paragraph.scroll((app.pull_requests.detail_scroll, 0)), area);
}
