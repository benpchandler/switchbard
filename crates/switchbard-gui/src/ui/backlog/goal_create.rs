//! The "New Goal" modal (TASK-75) — the GUI entry point for defining a
//! weekly goal without the CLI. Same shell as the task composer
//! (`create.rs`): in single-repo scope the target repo is fixed; in the
//! All-repos scope it carries its own repo picker. Creates for the
//! *current* week only — historical or future targets stay a CLI
//! (`goal create --week` / `goal roll`) affair.

use super::{detail, Pending, Snapshot};
use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use std::path::PathBuf;
use switchbard_core::{GoalMeasure, NewGoal};

/// Open the goal composer. Both entry points (the goals-section header and
/// the zero-goals affordance) share this so repo targeting cannot drift.
pub(super) fn open_new_goal(app: &mut HiveApp, target_repo: Option<PathBuf>) {
    app.backlog_view.new_goal.target_repo = target_repo;
    app.backlog_view.new_goal.open = true;
}

pub(super) fn render_goal_modal(
    app: &mut HiveApp,
    ctx: &egui::Context,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    if !app.backlog_view.new_goal.open {
        return;
    }
    // In the All-repos scope with no explicit choice yet, seed the picker
    // with the first tracked repo; with nothing tracked there is nothing to
    // create into.
    if app.backlog_view.new_goal.target_repo.is_none() {
        app.backlog_view.new_goal.target_repo = snap.repos.first().map(|row| row.key.clone());
    }
    let Some(target_key) = app.backlog_view.new_goal.target_repo.clone() else {
        app.backlog_view.new_goal.open = false;
        return;
    };
    let Some(repo) = snap.repo(&target_key) else {
        app.backlog_view.new_goal.open = false;
        return;
    };
    let fixed_target = app.backlog_view.selected_repo.is_some();

    let mut open = true;
    let mut close = false;
    egui::Window::new("New Goal")
        .id(egui::Id::new("backlog_new_goal_modal"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if fixed_target {
                ui.label(
                    egui::RichText::new(format!("{} / {}", repo.repo_name, repo.worktree_label))
                        .color(theme::muted_text()),
                );
            } else {
                ui.label(egui::RichText::new("Repo").color(theme::muted_text()));
                egui::ComboBox::from_id_salt("backlog_new_goal_repo")
                    .selected_text(repo.label())
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for row in &snap.repos {
                            let selected =
                                app.backlog_view.new_goal.target_repo.as_deref() == Some(&row.key);
                            if ui.selectable_label(selected, row.label()).clicked() {
                                app.backlog_view.new_goal.target_repo = Some(row.key.clone());
                            }
                        }
                    });
            }

            ui.label("name");
            ui.add(
                egui::TextEdit::singleline(&mut app.backlog_view.new_goal.name)
                    .hint_text("Onboard users")
                    .desired_width(320.0),
            );
            ui.horizontal(|ui| {
                ui.label("target");
                ui.add(
                    egui::DragValue::new(&mut app.backlog_view.new_goal.target).range(1..=i64::MAX),
                );
                ui.label("unit");
                ui.add(
                    egui::TextEdit::singleline(&mut app.backlog_view.new_goal.unit)
                        .hint_text("users")
                        .desired_width(120.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("measure");
                ui.selectable_value(
                    &mut app.backlog_view.new_goal.measure_tasks,
                    false,
                    "manual check-ins",
                );
                ui.selectable_value(
                    &mut app.backlog_view.new_goal.measure_tasks,
                    true,
                    "tasks done in the week",
                );
            });
            if app.backlog_view.new_goal.measure_tasks {
                ui.horizontal(|ui| {
                    ui.label("scope");
                    let known = detail::known_project_names(snap);
                    egui::ComboBox::from_id_salt("backlog_new_goal_scope")
                        .selected_text("pick")
                        .width(72.0)
                        .show_ui(ui, |ui| {
                            for name in &known {
                                ui.selectable_value(
                                    &mut app.backlog_view.new_goal.scope,
                                    name.clone(),
                                    name,
                                );
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut app.backlog_view.new_goal.scope)
                            .hint_text("project name or label")
                            .desired_width(220.0),
                    );
                });
            }
            ui.label(
                egui::RichText::new("Created for the current week; use `goal roll` weekly.")
                    .small()
                    .color(theme::muted_text()),
            );

            ui.add_space(6.0);
            let state = &app.backlog_view.new_goal;
            let ready = !state.name.trim().is_empty()
                && !state.unit.trim().is_empty()
                && (!state.measure_tasks || !state.scope.trim().is_empty());
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(ready, egui::Button::new("Create"))
                    .on_disabled_hover_text(
                        "Needs a name and unit — and a scope when measuring from tasks",
                    )
                    .clicked()
                {
                    let state = &app.backlog_view.new_goal;
                    let week = switchbard_core::week_monday_of(chrono::Local::now().date_naive())
                        .format("%Y-%m-%d")
                        .to_string();
                    pending.goal_create = Some((
                        target_key.clone(),
                        NewGoal {
                            name: state.name.trim().to_string(),
                            unit: state.unit.trim().to_string(),
                            measure: if state.measure_tasks {
                                GoalMeasure::Tasks
                            } else {
                                GoalMeasure::Manual
                            },
                            scope: (state.measure_tasks && !state.scope.trim().is_empty())
                                .then(|| state.scope.trim().to_string()),
                            week,
                            target: state.target,
                        },
                    ));
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close || !open {
        app.backlog_view.new_goal.open = false;
        if close {
            app.backlog_view.new_goal.name.clear();
        }
    }
}
