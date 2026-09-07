//! PR list and full-width detail, rendered only from cached observations.
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
use switchbard_core::{PrChecks, PrListRow, PrMerge, PrReview};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pull Requests ")
        .border_style(app.config.theme.style(Surface::Border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [status, body] = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(observation(app))
            .wrap(Wrap { trim: false })
            .style(app.config.theme.style(Surface::Hint)),
        status,
    );
    if app.pane == Pane::Detail {
        detail(frame, app, body);
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
        "PARTIAL: first 100 open PRs"
    } else {
        "open PRs"
    };
    let suffix = prs
        .error
        .as_deref()
        .unwrap_or(if prs.loading() { "refreshing" } else { "" });
    format!(
        "{} · {} {}s ago · {}\n{}",
        snapshot.repository, health, age, coverage, suffix
    )
}

fn list(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(snapshot) = &app.pull_requests.snapshot else {
        return;
    };
    if snapshot.rows.is_empty() {
        let text = if snapshot.truncated {
            "No PR rows observed; coverage is incomplete."
        } else if app.pull_requests.error.is_some() {
            "Last observation had no open PRs. Current state unknown."
        } else {
            "No open pull requests in this repository at last observation."
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
            "Linked tasks",
            if area.width < 70 { "!" } else { "Delivery" },
            "Title",
        ],
        app.config.theme.style(Surface::Header),
    );
    for (offset, row) in snapshot
        .rows
        .iter()
        .skip(app.pull_requests.scroll)
        .take(slots)
        .enumerate()
    {
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

fn draw_cells(frame: &mut Frame, rect: Rect, texts: [&str; 4], style: ratatui::style::Style) {
    let compact = rect.width < 70;
    let widths = [
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
    let signal = if rect.width < 70 {
        match row.attention_rank() {
            0 => "!",
            1 => "?",
            2 => "~",
            _ => "+",
        }
        .into()
    } else {
        delivery(row)
    };
    let identity = format!("#{}", row.number);
    let style = app.config.theme.style(if selected {
        Surface::Selected
    } else {
        Surface::Text
    });
    draw_cells(frame, rect, [&identity, &links, &signal, &row.title], style);
}

fn delivery(row: &PrListRow) -> String {
    if row.merge == PrMerge::Conflicting {
        return "! Conflict".into();
    }
    if row.review == PrReview::ChangesRequested {
        return "! Changes asked".into();
    }
    match row.checks {
        PrChecks::Failed => "! Checks failed",
        PrChecks::Unknown => "? Checks unknown",
        PrChecks::NoneObserved => "? No checks seen",
        PrChecks::Running => "Checks pending",
        PrChecks::Passing if row.review == PrReview::Unknown || row.merge == PrMerge::Unknown => {
            "? Review / merge"
        }
        PrChecks::Passing if row.review == PrReview::Required => "Review required",
        PrChecks::Passing if row.draft => "Draft",
        PrChecks::Passing => "Checks passed",
    }
    .into()
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
            "{} · {}",
            if row.draft { "Draft" } else { "Open" },
            delivery(row)
        )),
        Line::from(format!(
            "{} · {} · {}",
            row.checks.label(),
            row.review.label(),
            row.merge.label()
        )),
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
