//! The Portfolio lens (task-19): read-only per-repo health — open/in-
//! progress/done counts, oldest open task, last activity, and blocked count.
//! A `switchbard_core::compute_cross_repo_stats` row (`RepoStats`) already
//! carries everything this table needs; this module is presentation only.

use super::scoped_repos;
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{compute_cross_repo_stats, RepoStats};

pub(super) fn render_portfolio(app: &mut HiveApp, ui: &mut egui::Ui, snap: &super::Snapshot) {
    let scoped = scoped_repos(app, snap);
    let repos: Vec<(String, &switchbard_core::BacklogRepo)> = scoped
        .iter()
        .map(|row| (row.repo_name.clone(), &row.repo))
        .collect();
    let stats = compute_cross_repo_stats(&repos);

    egui::ScrollArea::both()
        .id_salt("backlog_portfolio")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if stats.per_repo.is_empty() {
                ui.label(egui::RichText::new("No tracked repos").color(theme::muted_text()));
                return;
            }
            egui::Grid::new("backlog_portfolio_table")
                .striped(true)
                .num_columns(7)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    header_row(ui);
                    for repo in &stats.per_repo {
                        render_row(ui, repo);
                    }
                });
        });
}

fn header_row(ui: &mut egui::Ui) {
    for label in [
        "Repo",
        "Open",
        "In Progress",
        "Done",
        "Blocked",
        "Oldest open",
        "Last activity",
    ] {
        ui.label(
            egui::RichText::new(label)
                .small()
                .color(theme::muted_text()),
        );
    }
    ui.end_row();
}

fn render_row(ui: &mut egui::Ui, repo: &RepoStats) {
    ui.label(egui::RichText::new(&repo.repo).strong());
    ui.label(repo.open().to_string());
    ui.label(
        repo.by_status
            .get("In Progress")
            .copied()
            .unwrap_or(0)
            .to_string(),
    );
    ui.label(format!("{} ({:.0}%)", repo.done, repo.completion_pct()));
    if repo.blocked > 0 {
        ui.colored_label(theme::warn_orange(), repo.blocked.to_string());
    } else {
        ui.label("0");
    }
    ui.label(
        repo.oldest_open_created_date
            .as_deref()
            .unwrap_or("—")
            .to_string(),
    );
    ui.label(
        repo.last_activity_updated_date
            .as_deref()
            .unwrap_or("—")
            .to_string(),
    );
    ui.end_row();
}
