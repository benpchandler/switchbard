//! Shared presentation for the task read model's non-ready states.

use crate::app::HiveApp;
use crate::runtime::TasksReadState;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::path::Path;

/// Render a zero-row state without confusing an incomplete or failed read
/// with an authoritative empty result.
pub fn render_empty(app: &HiveApp, ui: &mut egui::Ui, heading: &str, state: &TasksReadState) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading(heading);
        ui.add_space(8.0);
        match state {
            TasksReadState::InitialLoading => {
                centered_status_pill(ui, StatusKind::Info, "Loading task data");
                ui.label(
                    egui::RichText::new("Loading tasks from tracked repositories…")
                        .color(theme::muted_text()),
                );
            }
            TasksReadState::Refreshing => {
                centered_status_pill(ui, StatusKind::Info, "Refreshing task data");
                ui.label(
                    egui::RichText::new("Refreshing tasks from tracked repositories…")
                        .color(theme::muted_text()),
                );
            }
            TasksReadState::Stale { .. } => {
                centered_status_pill(ui, StatusKind::Warn, "Task data unavailable");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(failure_message(state.failed_source_count(), false))
                        .color(theme::muted_text()),
                );
                ui.add_space(8.0);
                if ui.button("Retry task refresh").clicked() {
                    app.request_tasks_refresh();
                }
            }
            TasksReadState::Ready => {
                ui.label(
                    egui::RichText::new(
                        "No tracked worktrees have a backlog/config.yml or backlog/tasks directory.",
                    )
                    .color(theme::muted_text()),
                );
            }
        }
    });
}

fn centered_status_pill(ui: &mut egui::Ui, kind: StatusKind, text: &str) {
    let text_width = ui
        .painter()
        .layout_no_wrap(
            text.to_string(),
            egui::TextStyle::Small.resolve(ui.style()),
            kind.color(),
        )
        .size()
        .x;
    let pill_width = text_width + 14.0;
    let leading_space = ((ui.available_width() - pill_width) / 2.0).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(leading_space);
        status_pill(ui, kind, text, None);
    });
}

/// Explain why already-rendered rows may not represent the latest disk
/// state. Ready data needs no extra chrome.
pub fn render_retained_rows_notice(ui: &mut egui::Ui, state: &TasksReadState) {
    match state {
        TasksReadState::Refreshing => {
            ui.horizontal_wrapped(|ui| {
                status_pill(ui, StatusKind::Info, "Refreshing task data", None);
                ui.label(
                    egui::RichText::new("Refreshing tasks. Showing last-known rows.")
                        .color(theme::muted_text()),
                );
            });
        }
        TasksReadState::Stale { .. } => {
            ui.horizontal_wrapped(|ui| {
                status_pill(ui, StatusKind::Warn, "Task data stale", None);
                ui.label(
                    egui::RichText::new(failure_message(state.failed_source_count(), true))
                        .color(theme::muted_text()),
                );
            });
        }
        TasksReadState::InitialLoading | TasksReadState::Ready => {}
    }
}

fn failure_message(failed_repos: usize, retained_rows: bool) -> String {
    let source = if failed_repos == 1 {
        "1 task source".to_string()
    } else {
        format!("{failed_repos} task sources")
    };
    if retained_rows {
        format!(
            "{source} could not be refreshed. Showing last-known rows; edits to that source are disabled until a refresh succeeds."
        )
    } else {
        format!("{source} could not be loaded.")
    }
}

/// The one write gate for the stale read model (TASK-127 AC4). Returns
/// `true`, and explains the refusal on the backlog status line, when
/// `root`'s rows are cached-only because its last read failed. Every task
/// write intent passes through here (`backlog::apply_pending` and the Board
/// drop path) so no surface can edit a file the model can no longer see.
pub fn refuse_stale_source_write(app: &HiveApp, root: &Path, subject: &str) -> bool {
    if !app.tasks_read_state_snapshot().blocks_writes_to(root) {
        return false;
    }
    app.backlog_status.set(format!(
        "{subject}: edits disabled while its task source is stale; retry the refresh first"
    ));
    true
}
