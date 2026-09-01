//! Global free-text search overlay (Cmd+K / Ctrl+K), task-15 AC #2.
//!
//! "Global" here means cross-repo scope — every tracked repo's tasks,
//! independent of the current repo/status/priority filters or lens —
//! not cross-view; the shortcut is only live while the Backlog tab is on
//! screen, matched by the Backlog.md webview's own command palette being
//! scoped to its task views.
//!
//! Matches title, id, description, and labels (case-insensitive substring),
//! same fields `sort::task_visible`'s free-text filter checks, so search
//! results and the list filter behave consistently.

use super::Snapshot;
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::BacklogTaskSource;

const MAX_RESULTS: usize = 40;

pub(crate) fn handle_shortcut(app: &mut HiveApp, ctx: &egui::Context) {
    let pressed = ctx.input_mut(|input| {
        input.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::K,
        ))
    });
    if pressed {
        app.backlog_view.search.open = !app.backlog_view.search.open;
        if app.backlog_view.search.open {
            app.backlog_view.search.query.clear();
        }
    }
    if app.backlog_view.search.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.backlog_view.search.open = false;
    }
}

pub(crate) fn render_overlay(app: &mut HiveApp, ctx: &egui::Context, snap: &Snapshot) {
    if !app.backlog_view.search.open {
        return;
    }
    let mut open = true;
    egui::Window::new("Search all repos")
        .id(egui::Id::new("backlog_global_search"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0))
        .fixed_size(egui::vec2(560.0, 420.0))
        .show(ctx, |ui| render_contents(app, ui, snap));
    if !open {
        app.backlog_view.search.open = false;
    }
}

fn render_contents(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot) {
    let response = ui.add(
        egui::TextEdit::singleline(&mut app.backlog_view.search.query)
            .hint_text("Search title, id, description, labels…")
            .desired_width(f32::INFINITY),
    );
    if app.backlog_view.search.query.is_empty() {
        response.request_focus();
    }
    ui.separator();

    let query_lc = app.backlog_view.search.query.to_lowercase();
    if query_lc.is_empty() {
        ui.label(
            egui::RichText::new("Type to search across every tracked repo.")
                .color(theme::muted_text()),
        );
        return;
    }

    let mut matches = Vec::new();
    for repo in &snap.repos {
        for task in &repo.repo.tasks {
            if task.source == BacklogTaskSource::Archived {
                continue;
            }
            if matches_query(task, &query_lc) {
                matches.push((repo, task));
            }
        }
        if matches.len() >= MAX_RESULTS {
            break;
        }
    }

    if matches.is_empty() {
        ui.label(egui::RichText::new("No matches").color(theme::muted_text()));
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            for (repo, task) in matches.iter().take(MAX_RESULTS) {
                let label = format!("{}:{}  {}", repo.repo_name, task.id, task.title);
                if ui.selectable_label(false, label).clicked() {
                    // Owner UX pass (2026-08-05): selecting is enough — the
                    // persistent detail rail shows it regardless of lens,
                    // so this no longer needs to jump to List.
                    app.backlog_view.selected_repo = None;
                    app.backlog_view.selected_task = Some((repo.key.clone(), task.id.clone()));
                    app.backlog_view.editor.loaded_key = None;
                    app.backlog_view.search.open = false;
                }
            }
        });
}

fn matches_query(task: &switchbard_core::BacklogTask, query_lc: &str) -> bool {
    let haystack = [
        task.id.as_str(),
        task.title.as_str(),
        task.description.as_str(),
        &task.labels.join(" "),
    ]
    .join(" ")
    .to_lowercase();
    haystack.contains(query_lc)
}
