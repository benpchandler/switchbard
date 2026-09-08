//! The Inbox destination, awaiting the action collection workflow.
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::config::{Action, Surface};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    frame.render_widget(
        Paragraph::new("Your review requests and follow-ups will appear here.")
            .style(theme.style(Surface::Hint))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.style(Surface::Border))
                    .title(" Inbox "),
            ),
        area,
    );
}

pub fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.config.theme;
    let mut lines: Vec<Line<'static>> = Action::all()
        .filter(|action| app.page.allows(action))
        .map(|action| {
            Line::from(vec![
                Span::styled(
                    app.config.bindings_for(&action).join(" / "),
                    theme.style(Surface::Keys),
                ),
                Span::raw(format!("  {}", action.name())),
            ])
        })
        .collect();
    lines.push(Line::raw(":page cycles Tasks / Pull Requests / Inbox"));
    lines.push(Line::raw(":bug <description> / :idea <description>"));
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.style(Surface::Border))
                .title(" Inbox keys "),
        ),
        area,
    );
}
