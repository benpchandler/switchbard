//! Pinned testing directions and a small, persistent feedback editor.
use crate::{
    app::{App, IslandAction, IslandHit},
    config::Surface,
    experiments::{catalog, ExperimentReview},
};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};

pub(super) fn height(app: &App, available: u16) -> u16 {
    if !active_id(app).is_some_and(|id| catalog().iter().any(|spec| spec.id == id)) {
        return 0;
    }
    available.min(if app.island.editing { 8 } else { 3 })
}

pub(super) fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    app.island.hits.clear();
    let Some(spec) = active_id(app).and_then(|id| catalog().iter().find(|spec| spec.id == id))
    else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    let background = app.config.theme.style(Surface::Selected);
    frame.render_widget(Paragraph::new("").style(background), area);
    let title = format!(" E{:03} {}", spec.number, spec.title);
    if app.island.editing {
        text(
            frame,
            row(area, 0),
            title,
            background.add_modifier(Modifier::BOLD),
        );
        draw_editor(frame, app, area, spec.description, background);
        return;
    }
    let state = app.experiments.state(spec.id);
    let (review, review_color) = match state.review {
        ExperimentReview::Unreviewed => ("Keep", "green"),
        ExperimentReview::Kept => ("Kept", "green"),
        ExperimentReview::RemovalRequested => ("Remove", "red"),
    };
    let feedback = if !app.experiment_feedback.draft(spec.id).is_empty() {
        "Draft"
    } else if app.experiment_feedback.submission_count(spec.id) > 0 {
        "Saved"
    } else {
        "Feedback"
    };
    let controls = [
        (
            if state.enabled { "On" } else { "Off" },
            3,
            if state.enabled { "cyan" } else { "gray" },
            IslandAction::Toggle,
        ),
        (review, 6, review_color, IslandAction::Review),
        (
            feedback,
            8,
            if feedback == "Saved" { "green" } else { "cyan" },
            IslandAction::Feedback,
        ),
        ("Next", 4, "gray", IslandAction::Next),
        ("x", 1, "gray", IslandAction::Hide),
    ];
    // Preserve the experiment number and a title fragment, even in a narrow pane.
    let budget = area.width.saturating_sub(16);
    let count = controls
        .iter()
        .scan(0_u16, |used, (_, width, _, _)| {
            *used += *width as u16 + 3;
            Some(*used)
        })
        .take_while(|used| *used <= budget)
        .count();
    let controls_width = controls[..count]
        .iter()
        .map(|(_, width, _, _)| *width as u16 + 3)
        .sum::<u16>();
    text(
        frame,
        Rect {
            width: area.width.saturating_sub(controls_width),
            ..row(area, 0)
        },
        title,
        background.add_modifier(Modifier::BOLD),
    );
    let mut x = area.right().saturating_sub(controls_width);
    for (label, width, color, action) in controls.iter().take(count) {
        let button = Rect::new(x, area.y, *width as u16 + 2, 1);
        draw_button(
            frame, app, button, label, *width, color, *action, background,
        );
        x += button.width + 1;
    }
    if area.height > 1 {
        let directions = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1).min(2),
            ..area
        };
        draw_directions(frame, directions, spec.description, background);
    }
    if area.height > 2 {
        if let Some(error) = &app.island.error {
            text(
                frame,
                row(area, 2),
                format!(" {error}"),
                background.fg(semantic_color(app, "red")),
            );
        } else if Paragraph::new(format!(" Try: {}", spec.description))
            .wrap(Wrap { trim: false })
            .line_count(area.width)
            == 1
        {
            let status = if !app.experiment_feedback.draft(spec.id).is_empty() {
                "Feedback draft saved · Feedback resumes it".to_string()
            } else {
                app.island.message.clone().unwrap_or_else(|| {
                    "Feedback saves what happened · Next changes instructions".into()
                })
            };
            text(frame, row(area, 2), format!(" {status}"), background);
        }
    }
}

fn active_id(app: &App) -> Option<&str> {
    if app.island.editing {
        app.island.feedback_id.as_deref()
    } else {
        app.experiment_feedback.active()
    }
}

fn draw_editor(frame: &mut Frame, app: &mut App, area: Rect, description: &str, style: Style) {
    if area.height < 5 {
        if area.height > 1 {
            text(
                frame,
                row(area, 1),
                " Feedback open · enlarge terminal".into(),
                style,
            );
        }
        if area.height > 2 {
            let message = app
                .island
                .error
                .clone()
                .unwrap_or_else(|| "Esc saves draft and goes back".into());
            let recovery_style = if app.island.error.is_some() {
                style.fg(semantic_color(app, "red"))
            } else {
                style
            };
            text(frame, row(area, 2), format!(" {message}"), recovery_style);
        }
        return;
    }
    // Keep input and recovery controls usable even when the terminal is short.
    let directions_height = if area.height >= 7 { 2 } else { 0 };
    if directions_height > 0 {
        draw_directions(
            frame,
            Rect {
                y: area.y + 1,
                height: directions_height,
                ..area
            },
            description,
            style,
        );
    }
    let status_y = 1 + directions_height;
    let status = app.island.error.clone().unwrap_or_else(|| {
        if app.island.dirty {
            "Draft not saved yet".into()
        } else if app.island.draft.is_empty() {
            "Feedback: what happened / what should change?".into()
        } else {
            "Draft saved locally · submit when ready".into()
        }
    });
    let status_style = if app.island.error.is_some() {
        style.fg(semantic_color(app, "red"))
    } else {
        style
    };
    text(
        frame,
        row(area, status_y),
        format!(" {status}"),
        status_style,
    );
    let input = Rect {
        x: area.x + 1,
        y: area.y + status_y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(status_y + 2),
    };
    draw_input(frame, app, input);
    let footer = row(area, area.height - 1);
    let mut x = footer.x + 1;
    for (label, width, action) in [
        ("Submit", 6, IslandAction::Submit),
        ("Back", 4, IslandAction::CloseFeedback),
    ] {
        if x + width as u16 + 2 <= footer.right() {
            let button = Rect::new(x, footer.y, width as u16 + 2, 1);
            draw_button(frame, app, button, label, width, "cyan", action, style);
            x += button.width + 1;
        }
    }
    if x < footer.right() {
        text(
            frame,
            Rect::new(x, footer.y, footer.right() - x, 1),
            "Ctrl-S submit · Esc save draft + back".into(),
            style,
        );
    }
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut lines = vec![String::new()];
    let mut column = 0;
    for ch in app.island.draft.chars() {
        if ch == '\n' {
            lines.push(String::new());
            column = 0;
            continue;
        }
        let cells = Span::raw(ch.to_string()).width() as u16;
        if cells > area.width {
            continue;
        }
        if column + cells > area.width {
            lines.push(String::new());
            column = 0;
        }
        if let Some(line) = lines.last_mut() {
            line.push(ch);
        }
        column += cells;
        if column == area.width {
            lines.push(String::new());
            column = 0;
        }
    }
    let offset = lines.len().saturating_sub(area.height as usize);
    let cursor_y = lines.len().saturating_sub(1 + offset) as u16;
    let visible = lines
        .into_iter()
        .skip(offset)
        .map(Line::raw)
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(visible).style(app.config.theme.canvas_style()),
        area,
    );
    frame.set_cursor_position((area.x + column.min(area.width - 1), area.y + cursor_y));
}

fn draw_directions(frame: &mut Frame, area: Rect, description: &str, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let paragraph = Paragraph::new(format!(" Try: {description}"))
        .style(style)
        .wrap(Wrap { trim: false });
    let overflow = paragraph.line_count(area.width) > area.height as usize;
    frame.render_widget(paragraph, area);
    if overflow && area.height > 1 {
        text(
            frame,
            row(area, area.height - 1),
            " … widen pane for full instructions".into(),
            style,
        );
    }
}

fn draw_button(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    width: usize,
    color: &str,
    action: IslandAction,
    style: Style,
) {
    let pill = crate::progress::word_pill(
        label,
        width,
        semantic_color(app, color),
        style,
        app.config.progress_style,
    );
    frame.render_widget(Paragraph::new(pill), area);
    app.island.hits.push(IslandHit { area, action });
}

fn semantic_color(app: &App, name: &str) -> Color {
    app.config
        .theme
        .emphasis_style(name, &app.config.palette)
        .and_then(|style| style.bg.or(style.fg))
        .unwrap_or(Color::Gray)
}

fn row(area: Rect, index: u16) -> Rect {
    Rect {
        y: area.y + index,
        height: u16::from(index < area.height),
        ..area
    }
}

fn text(frame: &mut Frame, area: Rect, value: String, style: Style) {
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(value).style(style), area);
}
