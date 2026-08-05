//! The Digest lens (task-21): "what should I do today" — the Backlog tab's
//! default landing screen. Four sections, each capped and cross-repo:
//! overdue, newly unblocked, in progress, and recently done. Each section is
//! an entry point back into triage — clicking a task jumps to the List lens
//! with it selected; each section header can jump to the List lens filtered
//! to match.
//!
//! Unlike the List/Board lenses, Digest reads directly from `scoped_projects`
//! rather than the toolbar-filtered `tasks` list — the same choice `stats.rs`
//! makes. A "Recently Done" section would otherwise render empty whenever
//! the user has the Done filter off elsewhere, which defeats the point of a
//! landing page.

use super::{format, scoped_projects, ProjectRow, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogLens;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{is_newly_unblocked, triage_entry_from_task, BacklogTask, TriageDue};

/// Cap per section — a digest is a glance, not another full list.
const SECTION_LIMIT: usize = 6;

struct DigestRow<'a> {
    project: &'a ProjectRow,
    task: &'a BacklogTask,
    /// Section-specific context line (e.g. which dependency just cleared).
    subtitle: Option<String>,
}

pub(super) fn render_digest(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot) {
    let scoped = scoped_projects(app, snap);
    let today_day = chrono::Utc::now().timestamp().div_euclid(86_400);

    let mut overdue = Vec::new();
    let mut newly_unblocked = Vec::new();
    let mut in_progress = Vec::new();
    let mut recently_done: Vec<(&ProjectRow, &BacklogTask, i64)> = Vec::new();

    for project in &scoped {
        for task in &project.project.tasks {
            if task.source == switchbard_core::BacklogTaskSource::Archived {
                continue;
            }
            if task.is_done() {
                if let Some(day) = task
                    .updated_date
                    .as_deref()
                    .and_then(switchbard_core::parse_backlog_day)
                {
                    recently_done.push((project, task, day));
                }
                continue;
            }
            let entry = triage_entry_from_task(
                project.key.clone(),
                &project.repo_name,
                task,
                &project.project,
            );
            if entry.due == TriageDue::Overdue {
                overdue.push(DigestRow {
                    project,
                    task,
                    subtitle: None,
                });
            }
            if is_newly_unblocked(task, &project.project, today_day) {
                newly_unblocked.push(DigestRow {
                    project,
                    task,
                    subtitle: Some("a dependency was just completed".to_string()),
                });
            }
            if task.status.eq_ignore_ascii_case("in progress") {
                in_progress.push(DigestRow {
                    project,
                    task,
                    subtitle: None,
                });
            }
        }
    }
    recently_done.sort_by_key(|(_, _, day)| std::cmp::Reverse(*day));
    let recently_done: Vec<DigestRow<'_>> = recently_done
        .into_iter()
        .map(|(project, task, _)| DigestRow {
            project,
            task,
            subtitle: task.updated_date.clone(),
        })
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("backlog_digest")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_section(
                app,
                ui,
                "Overdue",
                overdue,
                "No overdue tasks (Backlog.md has no due-date field yet — this section is future-ready).",
                None,
            );
            render_section(
                app,
                ui,
                "Newly unblocked",
                newly_unblocked,
                "Nothing has recently come unblocked.",
                None,
            );
            render_section(
                app,
                ui,
                "In progress",
                in_progress,
                "Nothing in progress right now.",
                Some("In Progress"),
            );
            render_section(
                app,
                ui,
                "Recently done",
                recently_done,
                "Nothing completed recently.",
                Some("Done"),
            );
        });
}

fn render_section(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    title: &str,
    rows: Vec<DigestRow<'_>>,
    empty_text: &str,
    view_all_status_filter: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().heading());
        ui.label(egui::RichText::new(format!("{}", rows.len())).color(theme::muted_text()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("View all").clicked() {
                app.backlog_view.lens = BacklogLens::List;
                if let Some(status) = view_all_status_filter {
                    app.backlog_view.status_filter = status.to_string();
                }
            }
        });
    });
    ui.separator();
    if rows.is_empty() {
        ui.label(egui::RichText::new(empty_text).color(theme::muted_text()));
    } else {
        for row in rows.into_iter().take(SECTION_LIMIT) {
            render_strip(app, ui, &row);
            ui.add_space(4.0);
        }
    }
    ui.add_space(14.0);
}

fn render_strip(app: &mut HiveApp, ui: &mut egui::Ui, row: &DigestRow<'_>) {
    let key = (row.project.key.clone(), row.task.id.clone());
    let frame = egui::Frame::default()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(10, 6));
    let resp = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let _ = theme::painted_dot(ui, theme::repo_rail_color(&row.project.repo_name));
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&row.project.repo_name)
                                .small()
                                .color(theme::muted_text()),
                        );
                        ui.label(
                            egui::RichText::new(&row.task.id)
                                .monospace()
                                .small()
                                .color(theme::muted_text()),
                        );
                        ui.label(
                            egui::RichText::new(format::priority_title(&row.task.priority))
                                .small()
                                .color(format::priority_color(&row.task.priority)),
                        );
                    });
                    ui.label(egui::RichText::new(&row.task.title).strong());
                    if let Some(subtitle) = &row.subtitle {
                        ui.label(
                            egui::RichText::new(subtitle)
                                .small()
                                .color(theme::muted_text()),
                        );
                    }
                });
            });
        })
        .response;
    if resp
        .interact(egui::Sense::click())
        .on_hover_text("Show details in the rail")
        .clicked()
    {
        // Widen to "All projects" scope — a Digest card can surface a task
        // from any tracked project regardless of the current single-project
        // scope, so selecting it needs the rail to actually find it.
        app.backlog_view.selected_project = None;
        app.backlog_view.selected_task = Some(key);
        app.backlog_view.editor.loaded_key = None;
    }
}
