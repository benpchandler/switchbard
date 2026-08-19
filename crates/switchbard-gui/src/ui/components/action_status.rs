//! Single-line-clamped label for the top bar's free-form "last action"
//! status messages (`HiveApp::config_status`/`kill_status`/`server_status`/
//! `backlog_status`).
//!
//! Defense in depth (TASK-28, 2026-08-05): a `backlog task create` call's
//! raw multi-line stdout used to land straight in `backlog_status` and get
//! painted with a plain `ui.label(msg)` — no wrapping/line limit at all —
//! stretching the whole top bar into a many-line void. The real fix is at
//! the source (`HiveApp::spawn_backlog_create` now builds a compact
//! message instead of forwarding raw CLI stdout), but every status label in
//! the top bar goes through this instead of a bare `ui.label` too, so a
//! *different* future mutation function making the same mistake — or any
//! status message that's simply too long for the bar at the current window
//! width — can't repeat the layout failure. The full message is always
//! available on hover, so clamping never actually loses information.

use eframe::egui;

/// `color` matches whatever styling the call site used before this existed
/// (`top_bar.rs`'s status messages are plain/default-colored; `sidebar.rs`'s
/// config-save message is muted) — this only changes *how much* renders,
/// never the color a message was already using.
pub fn action_status_label(
    ui: &mut egui::Ui,
    msg: &str,
    color: Option<egui::Color32>,
) -> egui::Response {
    // Take only the first line before anything else touches the widget —
    // `egui::Label::truncate()` handles horizontal overflow on one line,
    // but doesn't collapse embedded `\n`s into that line on its own, and
    // it's the embedded newlines (not just raw length) that caused the
    // original bug.
    let mut lines = msg.lines();
    let first_line = lines.next().unwrap_or("");
    let has_more_lines = lines.next().is_some();
    let display = if has_more_lines {
        format!("{first_line} …")
    } else {
        first_line.to_string()
    };
    let mut text = egui::RichText::new(display);
    if let Some(color) = color {
        text = text.color(color);
    }
    ui.add(egui::Label::new(text).truncate()).on_hover_text(msg)
}

#[cfg(test)]
mod tests {
    // Widget rendering itself is covered by `backlog_controls.rs`'s
    // `action_status_label_clamps_a_multiline_message_to_one_line` (needs a
    // real `egui::Ui` to measure/paint against, which this crate's `#[cfg(
    // test)]` unit tests don't construct — see that file's kittest harness
    // instead). This pins just the line-selection logic in isolation.
    fn first_line_and_ellipsis_marker(msg: &str) -> (String, bool) {
        let mut lines = msg.lines();
        let first_line = lines.next().unwrap_or("").to_string();
        (first_line, lines.next().is_some())
    }

    #[test]
    fn single_line_message_is_unchanged() {
        let (first, has_more) = first_line_and_ellipsis_marker("saved TASK-1");
        assert_eq!(first, "saved TASK-1");
        assert!(!has_more);
    }

    #[test]
    fn multiline_message_keeps_only_the_first_line() {
        let (first, has_more) =
            first_line_and_ellipsis_marker("File: /path/to/task-1.md\n\nTask TASK-1 - Title\n====");
        assert_eq!(first, "File: /path/to/task-1.md");
        assert!(has_more);
    }

    #[test]
    fn empty_message_is_empty() {
        let (first, has_more) = first_line_and_ellipsis_marker("");
        assert_eq!(first, "");
        assert!(!has_more);
    }
}
