//! The Statistics lens (TASK-16): cross-repo totals, completion %, status and
//! priority distributions, and a completion-trend burndown — overall and per
//! milestone. Everything is derived from `switchbard_core::backlog_stats`,
//! which in turn only reads metadata already present on each cached
//! `BacklogTask` (no new store, no schema change — task-16's constraint).
//!
//! Header language ("gauges") follows TASK-14 direction B's instrument
//! panel: a row of labeled numeric readouts, same idea as the mockup's
//! `<div class="dial">` gauges.

use super::Snapshot;
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{
    compute_burndown, compute_burndown_by_milestone, compute_cross_repo_stats, BurndownSeries,
    CANONICAL_STATUS_ORDER,
};

pub(super) fn render_statistics(app: &mut HiveApp, ui: &mut egui::Ui, snap: &Snapshot) {
    let scoped = super::scoped_projects(app, snap);
    let projects: Vec<(String, &switchbard_core::BacklogProject)> = scoped
        .iter()
        .map(|row| (row.repo_name.clone(), &row.project))
        .collect();
    let stats = compute_cross_repo_stats(&projects);

    egui::ScrollArea::vertical()
        .id_salt("backlog_statistics")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_gauges(ui, &stats);
            ui.add_space(12.0);
            render_distribution(ui, "Status", &canonical_status_order(&stats.by_status));
            ui.add_space(8.0);
            render_distribution(
                ui,
                "Priority",
                &stats
                    .by_priority
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect::<Vec<_>>(),
            );
            ui.add_space(12.0);
            render_repo_table(ui, &stats);
            ui.add_space(16.0);
            render_burndown_section(app, ui, &scoped);
        });
}

fn render_gauges(ui: &mut egui::Ui, stats: &switchbard_core::CrossRepoStats) {
    ui.horizontal(|ui| {
        gauge(
            ui,
            "OPEN",
            &(stats.total_tasks - stats.done_tasks).to_string(),
        );
        gauge(ui, "DONE", &stats.done_tasks.to_string());
        gauge(ui, "TOTAL", &stats.total_tasks.to_string());
        gauge(ui, "REPOS", &stats.per_repo.len().to_string());
        gauge(ui, "COMPLETION", &format!("{:.0}%", stats.completion_pct()));
    });
}

fn gauge(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.set_min_width(90.0);
        ui.label(egui::RichText::new(value).monospace().heading().strong());
        ui.label(
            egui::RichText::new(label)
                .small()
                .color(theme::muted_text()),
        );
    });
    ui.separator();
}

/// Owner UX pass (2026-08-05): the Status distribution renders in the same
/// shared canonical order every other status surface uses (Board columns,
/// List's filter, the detail-pane editor) instead of `BTreeMap`'s
/// alphabetical iteration order, which put "Done" before "In Progress"
/// before "To Do" — technically correct, but inconsistent with the rest of
/// the app and not the order a kanban flow reads in.
fn canonical_status_order(
    by_status: &std::collections::BTreeMap<String, usize>,
) -> Vec<(String, usize)> {
    let mut ordered: Vec<(String, usize)> =
        by_status.iter().map(|(k, v)| (k.clone(), *v)).collect();
    ordered.sort_by_key(|(status, _)| {
        CANONICAL_STATUS_ORDER
            .iter()
            .position(|c| c.eq_ignore_ascii_case(status))
            .unwrap_or(CANONICAL_STATUS_ORDER.len())
    });
    ordered
}

fn render_distribution(ui: &mut egui::Ui, title: &str, by_key: &[(String, usize)]) {
    ui.label(egui::RichText::new(title).strong());
    let max = by_key
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(0)
        .max(1);
    for (key, count) in by_key {
        ui.horizontal(|ui| {
            ui.add_sized([90.0, 16.0], egui::Label::new(key));
            let frac = *count as f32 / max as f32;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(160.0 * frac.max(0.02), 12.0),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 2.0, theme::sky());
            ui.label(
                egui::RichText::new(count.to_string())
                    .small()
                    .color(theme::muted_text()),
            );
        });
    }
}

fn render_repo_table(ui: &mut egui::Ui, stats: &switchbard_core::CrossRepoStats) {
    ui.label(egui::RichText::new("Per-repo breakdown").strong());
    egui::Grid::new("backlog_stats_repo_table")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Repo")
                    .small()
                    .color(theme::muted_text()),
            );
            ui.label(
                egui::RichText::new("Total")
                    .small()
                    .color(theme::muted_text()),
            );
            ui.label(
                egui::RichText::new("Done")
                    .small()
                    .color(theme::muted_text()),
            );
            ui.label(
                egui::RichText::new("Completion")
                    .small()
                    .color(theme::muted_text()),
            );
            ui.end_row();
            for repo in &stats.per_repo {
                ui.label(&repo.repo);
                ui.label(repo.total.to_string());
                ui.label(repo.done.to_string());
                ui.label(format!("{:.0}%", repo.completion_pct()));
                ui.end_row();
            }
        });
}

fn render_burndown_section(_app: &mut HiveApp, ui: &mut egui::Ui, scoped: &[&super::ProjectRow]) {
    ui.label(egui::RichText::new("Burndown (completion trend)").strong());
    let all_tasks: Vec<&switchbard_core::BacklogTask> = scoped
        .iter()
        .flat_map(|row| row.project.tasks.iter())
        .filter(|task| task.source != switchbard_core::BacklogTaskSource::Archived)
        .collect();
    let today_day = chrono::Utc::now().timestamp().div_euclid(86_400);

    let overall = compute_burndown(&all_tasks, today_day);
    let milestone_series = compute_burndown_by_milestone(&all_tasks, today_day);

    render_series(ui, "Overall", &overall);
    for series in &milestone_series {
        ui.add_space(8.0);
        render_series(ui, &series.label, series);
    }
    if milestone_series.is_empty() {
        ui.label(
            egui::RichText::new("No milestone-assigned tasks with a parseable created date yet.")
                .small()
                .color(theme::muted_text()),
        );
    }
}

const CHART_HEIGHT: f32 = 90.0;
const CHART_WIDTH: f32 = 480.0;

fn render_series(ui: &mut egui::Ui, label: &str, series: &BurndownSeries) {
    ui.label(egui::RichText::new(label).small().strong());
    if series.points.len() < 2 {
        ui.label(
            egui::RichText::new("Not enough dated tasks yet to chart a trend.")
                .small()
                .color(theme::muted_text()),
        );
        return;
    }
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(CHART_WIDTH, CHART_HEIGHT), egui::Sense::hover());
    let painter = ui.painter();
    // `theme::card_bg()`, not `ui.visuals().extreme_bg_color` — the owner UX
    // pass repointed that egui slot to input fields (see theme.rs's doc).
    painter.rect_filled(rect, 3.0, theme::card_bg());

    let max_remaining = series
        .points
        .iter()
        .map(|p| p.remaining.max(p.completed_cumulative))
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let n = series.points.len() as f32;
    let point_x = |i: usize| rect.left() + (i as f32 / (n - 1.0)) * rect.width();
    let point_y = |v: usize| rect.bottom() - (v as f32 / max_remaining) * rect.height();

    let remaining_line: Vec<egui::Pos2> = series
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| egui::pos2(point_x(i), point_y(p.remaining)))
        .collect();
    let completed_line: Vec<egui::Pos2> = series
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| egui::pos2(point_x(i), point_y(p.completed_cumulative)))
        .collect();

    painter.add(egui::Shape::line(
        remaining_line,
        egui::Stroke::new(2.0, theme::amber()),
    ));
    painter.add(egui::Shape::line(
        completed_line,
        egui::Stroke::new(2.0, theme::green()),
    ));

    ui.horizontal(|ui| {
        ui.colored_label(theme::amber(), "— remaining");
        ui.colored_label(theme::green(), "— completed");
        let last = series.points.last().expect("checked len >= 2 above");
        ui.label(
            egui::RichText::new(format!(
                "{} remaining · {} completed",
                last.remaining, last.completed_cumulative
            ))
            .small()
            .color(theme::muted_text()),
        );
    });
}
