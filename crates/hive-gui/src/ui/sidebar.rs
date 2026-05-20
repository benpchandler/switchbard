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

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let repos = app.repos_snapshot();
    let worktrees = app.worktrees_snapshot();
    let picker_busy = matches!(*app.picker.lock().unwrap(), PickerState::InFlight);
    let config_msg = app.config_status.snapshot();

    // User intents queued during the immediate-mode render; applied after the
    // SidePanel closure returns so we don't double-borrow `app`.
    let mut want_pick = false;
    let mut move_request: Option<(usize, isize)> = None;

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

            let repo_count_total = repos.len();
            let s = app.state.lock().unwrap();
            for (i, repo) in repos.iter().enumerate() {
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
                    if repo_count > 0 {
                        theme::painted_dot_pulse(ui, theme::GREEN, repo_count);
                    } else {
                        theme::painted_dot(ui, egui::Color32::GRAY);
                    }
                    if theme::caret_button(ui, expanded).clicked() {
                        if expanded {
                            app.expanded_repos.remove(&repo.name);
                        } else {
                            app.expanded_repos.insert(repo.name.clone());
                        }
                    }
                    let label = format!("{} ({} wt)", repo.name, repo_worktrees.len());
                    let resp = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
                    if resp.clicked() {
                        if expanded {
                            app.expanded_repos.remove(&repo.name);
                        } else {
                            app.expanded_repos.insert(repo.name.clone());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Right-to-left layout: items added first end up on the right.
                        // Visual order: [count] [▲] [▼] [Remove]
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new("Remove").small())
                                    .frame(false),
                            )
                            .on_hover_text(format!(
                                "Remove '{}' from Hive (confirms before removing; \
                                 does not delete the repo on disk)",
                                repo.name
                            ))
                            .clicked()
                        {
                            app.confirm_remove_repo = Some((repo.path.clone(), repo.name.clone()));
                        }
                        let can_down = i + 1 < repo_count_total;
                        if theme::triangle_button(ui, false, can_down)
                            .on_hover_text("Move down")
                            .clicked()
                        {
                            move_request = Some((i, 1));
                        }
                        let can_up = i > 0;
                        if theme::triangle_button(ui, true, can_up)
                            .on_hover_text("Move up")
                            .clicked()
                        {
                            move_request = Some((i, -1));
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
                            if n > 0 {
                                theme::painted_dot_small_pulse(ui, theme::GREEN, n);
                            } else {
                                theme::painted_dot_small(ui, egui::Color32::DARK_GRAY);
                            }
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
    if let Some((i, delta)) = move_request {
        app.move_repo(i, delta);
    }
    render_remove_confirmation(app, ctx);
}

/// Modal that pops over the whole window when the user clicks the ✕ next to
/// a repo. Confirm removes the repo (does not touch the repo on disk).
fn render_remove_confirmation(app: &mut HiveApp, ctx: &egui::Context) {
    let Some((path, name)) = app.confirm_remove_repo.clone() else {
        return;
    };
    let mut open = true;
    let mut do_confirm = false;
    let mut do_cancel = false;
    egui::Window::new("Remove repo?")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(format!("Stop tracking '{name}' in Hive?")).strong());
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("Path: {}", path.display())).weak());
            ui.add_space(6.0);
            ui.label(
                "This only removes it from Hive — the repository and its \
                 worktrees stay on disk untouched.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Remove").color(egui::Color32::WHITE),
                        )
                        .fill(theme::DANGER),
                    )
                    .clicked()
                {
                    do_confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    do_cancel = true;
                }
            });
        });
    if do_confirm {
        app.remove_repo(path);
        app.confirm_remove_repo = None;
    } else if do_cancel || !open {
        app.confirm_remove_repo = None;
    }
}
