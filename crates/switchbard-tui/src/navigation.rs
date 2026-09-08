//! Persistent page headings and exact repository PR count, independent of list filters.
use crate::{
    app::App,
    config::{Action, Surface},
    page::Page,
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let compact = area.width < 52;
    let labels = [
        (Page::Tasks, "Tasks"),
        (
            Page::PullRequests,
            if compact { "PRs" } else { "Pull Requests" },
        ),
        (Page::Inbox, "Inbox"),
    ];
    let mut spans = Vec::with_capacity(10);
    for (page, label) in labels {
        let active = page == app.page;
        spans.push(Span::styled(
            if active {
                format!(" [{label}] ")
            } else {
                format!(" {label} ")
            },
            app.config
                .theme
                .style(if active { Surface::Chip } else { Surface::Hint }),
        ));
        if page == Page::PullRequests {
            spans.push(Span::raw(" "));
            spans.extend(pr_count_spans(app));
        }
        spans.push(Span::raw(" "));
    }
    let mut line = Line::from(spans);
    let hint = format!(
        " {} switch page",
        app.config.bindings_for(&Action::Page).join("/")
    );
    if line.width() + hint.len() <= usize::from(area.width) {
        line.spans
            .push(Span::styled(hint, app.config.theme.style(Surface::Keys)));
    }
    frame.render_widget(Paragraph::new(line), area);
}

fn pr_count_spans(app: &App) -> Vec<Span<'static>> {
    let prs = &app.pull_requests;
    let theme = &app.config.theme;
    match prs.snapshot.as_ref().map(|snapshot| &snapshot.open_count) {
        Some(Ok(count)) => {
            let mut spans = Vec::with_capacity(2);
            if *count > 0 {
                spans.push(Span::styled(
                    format!(" {count} "),
                    theme.style(Surface::AttentionBadge),
                ));
            }
            if prs.observation_stale(app.config.pr_refresh_seconds) {
                spans.push(Span::styled(" ?", theme.style(Surface::Hint)));
            }
            spans
        }
        _ => vec![Span::styled(
            if prs.loading() { " …" } else { " ?" },
            theme.style(Surface::Hint),
        )],
    }
}
