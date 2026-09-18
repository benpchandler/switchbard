//! Durable bug questions, owner replies, and reviewed publication.
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::config::{Action, Surface};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    use ratatui::layout::{Constraint, Direction, Layout};
    if app.inbox.publish_confirmation.is_some() {
        draw_confirmation(frame, app, area);
        return;
    }
    let theme = &app.config.theme;
    let inbox = &app.inbox;
    if inbox.rows.is_empty() {
        let message = inbox
            .error
            .as_deref()
            .or(inbox.read_error.as_deref())
            .unwrap_or("Your review requests and follow-ups will appear here.");
        frame.render_widget(
            Paragraph::new(message)
                .wrap(Wrap { trim: false })
                .style(theme.style(Surface::Hint))
                .block(Block::default().borders(Borders::ALL).title(" Inbox ")),
            area,
        );
        return;
    }
    let height = if area.height < 10 {
        0
    } else {
        (inbox.rows.len() as u16 + 2).min(7).min(area.height / 3)
    };
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(height), Constraint::Min(0)])
        .split(area);
    let start = inbox
        .selected
        .saturating_sub(usize::from(height.saturating_sub(3)));
    let rows: Vec<Line<'static>> = inbox
        .rows
        .iter()
        .enumerate()
        .skip(start)
        .take(usize::from(height))
        .map(|(index, run)| {
            Line::styled(
                format!(
                    "{} {} · {} · {}",
                    if index == inbox.selected { ">" } else { " " },
                    run.task_id,
                    crate::inbox_state::state_label(run.state),
                    run.title
                ),
                theme.style(if index == inbox.selected {
                    Surface::Selected
                } else {
                    Surface::Text
                }),
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(rows).block(Block::default().borders(Borders::ALL).title(" Inbox ")),
        parts[0],
    );
    let Some(run) = inbox.row() else {
        return;
    };
    if let Some(diff) = &inbox.diff {
        frame.render_widget(
            Paragraph::new(diff.as_str())
                .wrap(Wrap { trim: false })
                .scroll((inbox.scroll, 0))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Reviewed changes · PgUp/PgDn · :diff close "),
                ),
            parts[1],
        );
        return;
    }
    let mut lines = vec![
        Line::styled(
            crate::inbox_state::state_label(run.state),
            theme.style(Surface::Accent),
        ),
        Line::raw(run.title.clone()),
        Line::raw(format!("Repo: {}", run.repo_root.display())),
        Line::raw(""),
    ];
    if let Some(message) = run.messages.last() {
        lines.extend(message.text.lines().map(|s| Line::raw(s.to_owned())));
    }
    if let Some(head) = &run.review_head {
        lines.push(Line::raw(format!("Review commit: {head}")));
    }
    for evidence in &run.evidence {
        lines.push(Line::raw(format!("Check: {evidence}")));
    }
    for risk in &run.risks {
        lines.push(Line::raw(format!("Remaining: {risk}")));
    }
    if let Some(error) = &run.error {
        lines.push(Line::raw(error.to_string()));
    }
    if let Some(url) = &run.pr_url {
        lines.push(Line::styled(url.clone(), theme.style(Surface::Link)));
    }
    if let Some(error) = inbox.error.as_ref().or(inbox.read_error.as_ref()) {
        lines.push(Line::raw(error.clone()));
    }
    lines.push(Line::raw(""));
    if inbox.editing {
        lines.push(Line::styled(
            "Your reply (Ctrl-S send · Esc save · Ctrl-R keep both drafts)",
            theme.style(Surface::Keys),
        ));
        lines.extend(inbox.draft.lines().map(|s| Line::raw(s.to_owned())));
        lines.push(Line::styled("▏", theme.style(Surface::Accent)));
    } else {
        let hint = match run.state {
            switchbard_core::bug_run::RunState::AwaitingAnswer => "Enter reply",
            switchbard_core::bug_run::RunState::AwaitingReview => {
                "Enter feedback · :diff changes · :publish open PR"
            }
            switchbard_core::bug_run::RunState::Failed => ":retry retry this run",
            switchbard_core::bug_run::RunState::Unknown
                if run.retry_action == Some(switchbard_core::bug_run::RunAction::Publish) =>
            {
                "Recovery needed; :reconcile checks GitHub and safely resumes publication"
            }
            switchbard_core::bug_run::RunState::Unknown => {
                "Recovery needed; :retry resumes only when prior processes are proven gone"
            }
            switchbard_core::bug_run::RunState::PrOpen => "Enter open PR",
            _ => "Codex is working; you can leave this page",
        };
        lines.push(Line::styled(hint, theme.style(Surface::Keys)));
        if !run.draft.is_empty() {
            lines.push(Line::raw(format!("Saved draft: {}", run.draft)));
        }
    }
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let scroll = if inbox.editing {
        paragraph
            .line_count(parts[1].width.saturating_sub(2))
            .saturating_sub(usize::from(parts[1].height.saturating_sub(2)))
            .min(u16::MAX as usize) as u16
    } else {
        inbox.scroll
    };
    frame.render_widget(
        paragraph.scroll((scroll, 0)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", run.task_id)),
        ),
        parts[1],
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
    lines.push(Line::raw(
        ":page cycles Tasks / Pull Requests / Agents / Inbox",
    ));
    let mut bindings: Vec<_> = app
        .config
        .inbox_keys
        .iter()
        .map(|(key, action)| (key.label(), action))
        .collect();
    bindings.sort();
    for (key, action) in bindings {
        lines.push(Line::raw(format!("{key}  {action}")));
    }
    lines.insert(0, Line::raw(":bug <description> / :idea <description>"));
    lines.push(Line::raw(":dispatch <bug ID> retries filing-to-dispatch"));
    lines.push(Line::raw("Enter reply · Ctrl-S send · Esc keep draft"));
    lines.push(Line::raw(
        ":diff reviewed changes · :publish open PR · :retry failed run",
    ));
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

fn draw_confirmation(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(run) = &app.inbox.publish_confirmation else {
        return;
    };
    let text = format!("Open a PR for {}?\n\nGitHub: {}\nLocal repository: {}\nBranch: {}\nReviewed commit: {}\n\nThe validation gate runs before this reviewed commit is published.\n\nEnter open PR · Esc cancel", run.task_id, run.review_repository.as_deref().unwrap_or("unavailable; request fresh review"), run.repo_root.display(), run.branch.as_deref().unwrap_or("unknown"), run.review_head.as_deref().unwrap_or("unknown"));
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    app.inbox.confirmation_visible = paragraph.line_count(area.width.saturating_sub(2))
        <= usize::from(area.height.saturating_sub(2))
        && area.width >= 30;
    let title = if app.inbox.confirmation_visible {
        " Open PR "
    } else {
        " Enlarge terminal to confirm · Esc cancel "
    };
    frame.render_widget(
        paragraph.block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}
