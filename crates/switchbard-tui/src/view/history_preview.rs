//! Read-only use of the production row renderers over the selected saved view.
use crate::{app::App, config::Surface, page::Page, views::ViewState};
use ratatui::{layout::Rect, text::Line, widgets::Paragraph, Frame};

pub(super) fn draw(
    frame: &mut Frame,
    app: &App,
    state: &ViewState,
    area: Rect,
    scroll: usize,
) -> usize {
    if area.height < 2 || area.width == 0 {
        return 0;
    }
    if app.page == Page::PullRequests {
        return draw_prs(frame, app, state, area, scroll);
    }
    let mut projection = app.project_tasks(state);
    projection.rows = compact_headings(projection.rows);
    let scroll = scroll.min(projection.rows.len().saturating_sub(1));
    let body = current_data(frame, app, area, projection.visible.len(), "");
    if projection.visible.is_empty() {
        frame.render_widget(
            Paragraph::new("No current tasks match this view")
                .style(app.config.theme.style(Surface::Hint)),
            body,
        );
    } else {
        let mut cursor = super::TableCursor {
            scroll,
            selected: scroll,
            page_size: 1,
            highlight: false,
        };
        let _ = super::draw_task_rows(frame, app, state, &projection.rows, &mut cursor, body);
    }
    scroll
}

fn draw_prs(frame: &mut Frame, app: &App, state: &ViewState, area: Rect, scroll: usize) -> usize {
    let visible = app.pull_requests.visible_for(&state.filter, state.sort);
    let scroll = scroll.min(visible.len().saturating_sub(1));
    let mut suffix = if app
        .pull_requests
        .observation_stale(app.config.pr_refresh_seconds)
    {
        " · stale cached PRs".to_string()
    } else {
        " · cached PRs".to_string()
    };
    if app
        .pull_requests
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.truncated)
    {
        suffix.push_str(" · partial data");
    }
    let body = current_data(frame, app, area, visible.len(), &suffix);
    let Some(snapshot) = &app.pull_requests.snapshot else {
        let message = if app.pull_requests.error.is_some() {
            "Current PR data unavailable"
        } else {
            "Loading current pull requests…"
        };
        frame.render_widget(
            Paragraph::new(message).style(app.config.theme.style(Surface::Hint)),
            body,
        );
        return 0;
    };
    if visible.is_empty() {
        let message = if snapshot.truncated {
            "No matches in loaded PRs · partial data"
        } else {
            "No current pull requests match this view"
        };
        frame.render_widget(
            Paragraph::new(message).style(app.config.theme.style(Surface::Hint)),
            body,
        );
        return 0;
    }
    let header = Rect { height: 1, ..body };
    let widths = crate::pr_view::column_widths(app, state, body.width);
    let cells = crate::list_presentation::cells(header, &widths);
    let headers = state
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{} {}", i + 1, c.header(app.registry())))
        .collect::<Vec<_>>();
    crate::list_presentation::header(
        frame,
        header,
        &cells,
        &headers,
        app.config.theme.style(Surface::Header),
    );
    for (offset, index) in visible
        .iter()
        .skip(scroll)
        .take(usize::from(body.height.saturating_sub(1)))
        .enumerate()
    {
        let row_area = Rect {
            y: body.y + 1 + offset as u16,
            height: 1,
            ..body
        };
        crate::pr_view::draw_row(frame, app, state, &snapshot.rows[*index], row_area, false);
    }
    scroll
}

fn current_data(frame: &mut Frame, app: &App, area: Rect, count: usize, suffix: &str) -> Rect {
    let matches = if count == 1 { "match" } else { "matches" };
    frame.render_widget(
        Paragraph::new(Line::from(format!(
            "Current data · {count} {matches}{suffix}"
        )))
        .style(app.config.theme.style(Surface::Hint)),
        Rect { height: 1, ..area },
    );
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
}

/// Nested outlines retain their exact heading values without crowding task rows
/// out of a miniature. The full table keeps its original separate heading rows.
fn compact_headings(rows: Vec<crate::group::Row>) -> Vec<crate::group::Row> {
    use crate::group::Row;
    let mut compact = Vec::with_capacity(rows.len());
    for row in rows {
        match (compact.last_mut(), row) {
            (Some(Row::Heading { text, .. }), Row::Heading { text: next, .. }) => {
                text.push_str(" › ");
                text.push_str(&next);
            }
            (_, row) => compact.push(row),
        }
    }
    compact
}
