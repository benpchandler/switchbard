//! "Tracked repos" right-side panel — persistent across all views.
//!
//! Rendered from `app::update` before the central-view dispatch so adding /
//! removing repos works the same way no matter which tab you're on. The
//! per-repo "N listeners" badge stays meaningful everywhere: listeners are a
//! global concept, the count just describes how many of them attributed to
//! that repo on the most recent scan.

use crate::app::HiveApp;
use crate::runtime::PickerState;
use crate::ui::theme;
use eframe::egui;
use hive_core::WorktreeRef;
use std::path::PathBuf;

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let repos = app.repos_snapshot();
    let worktrees = app.worktrees_snapshot();
    let picker_busy = matches!(*app.picker.lock().unwrap(), PickerState::InFlight);
    let config_msg = app.config_status.snapshot();
    let mut remove_request: Option<PathBuf> = None;
    let mut want_pick = false;

    egui::SidePanel::right("repos")
        .resizable(true)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Tracked repos");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if picker_busy { "Picking…" } else { "➕ Add" };
                    if ui
                        .add_enabled(!picker_busy, egui::Button::new(label))
                        .on_hover_text("Choose a folder containing a git repository")
                        .clicked()
                    {
                        want_pick = true;
                    }
                });
            });
            ui.label(
                egui::RichText::new(format!(
                    "{} repo{} · {} worktree{}",
                    repos.len(),
                    if repos.len() == 1 { "" } else { "s" },
                    worktrees.len(),
                    if worktrees.len() == 1 { "" } else { "s" }
                ))
                .weak(),
            );
            if let Some(msg) = &config_msg {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(msg).weak());
            }
            ui.add_space(6.0);

            if repos.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("No repos configured yet").strong());
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "Click ➕ Add above and pick a folder\nthat contains a git repository.",
                        )
                        .weak(),
                    );
                });
                return;
            }

            let s = app.state.lock().unwrap();
            for repo in &repos {
                let repo_count = s
                    .listeners
                    .iter()
                    .filter(|l| l.repo_name.as_deref() == Some(repo.name.as_str()))
                    .count();
                let repo_worktrees: Vec<&WorktreeRef> = worktrees
                    .iter()
                    .filter(|w| w.repo_name == repo.name)
                    .collect();
                let expanded = app.expanded_repos.contains(&repo.name);

                ui.horizontal(|ui| {
                    let color = if repo_count > 0 {
                        theme::GREEN
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, theme::DOT_FILLED);
                    let arrow = if expanded { "▾" } else { "▸" };
                    let label = format!("{arrow} {} ({} wt)", repo.name, repo_worktrees.len());
                    let resp = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
                    if resp.clicked() {
                        if expanded {
                            app.expanded_repos.remove(&repo.name);
                        } else {
                            app.expanded_repos.insert(repo.name.clone());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(egui::RichText::new("✕").small()).frame(false))
                            .on_hover_text(format!(
                                "Remove '{}' from Hive (does not delete the repo)",
                                repo.name
                            ))
                            .clicked()
                        {
                            remove_request = Some(repo.path.clone());
                        }
                        if repo_count > 0 {
                            ui.label(egui::RichText::new(format!("{repo_count}")).strong());
                        } else {
                            ui.label(egui::RichText::new("—").weak());
                        }
                    });
                });

                if app.expanded_repos.contains(&repo.name) {
                    for w in &repo_worktrees {
                        let n = s
                            .listeners
                            .iter()
                            .filter(|l| l.worktree_path.as_ref() == Some(&w.path))
                            .count();
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            let dot_color = if n > 0 {
                                theme::GREEN
                            } else {
                                egui::Color32::DARK_GRAY
                            };
                            ui.colored_label(dot_color, theme::DOT_SMALL);
                            let branch = w.branch.as_deref().unwrap_or("(detached)");
                            ui.label(egui::RichText::new(branch).small());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if n > 0 {
                                        ui.label(
                                            egui::RichText::new(format!("{n}")).small().strong(),
                                        );
                                    }
                                },
                            );
                        });
                    }
                    ui.add_space(4.0);
                }
            }
        });

    if want_pick {
        app.open_repo_picker(ctx);
    }
    if let Some(path) = remove_request {
        app.remove_repo(path);
    }
}
