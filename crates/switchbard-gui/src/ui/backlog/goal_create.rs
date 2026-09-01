//! The "New Goal" modal (TASK-75) — the GUI entry point for defining a
//! weekly goal without the CLI. Same shell as the task composer
//! (`create.rs`): in single-repo scope the target repo is fixed; in the
//! All-repos scope it carries its own repo picker. Creates for the
//! *current* week only — historical or future targets stay a CLI
//! (`goal create --week` / `goal roll`) affair.
//!
//! `pub(crate)` (TASK-101): the Goals **place** (`ui::places::goals`) opens
//! and renders this exact modal too — the "one New Goal form, not two"
//! requirement — so its signature takes plain, crate-visible data
//! (`GoalModalRepoOption`, project-name strings) rather than `ui::backlog`'s
//! private `Snapshot`/`Pending` types, which a sibling module tree outside
//! `ui::backlog` cannot name. Each caller decides how to act on the
//! `Some((repo_root, goal))` a click on Create returns: `ui::backlog`
//! queues it on its own `Pending`, `ui::places::goals` spawns the write
//! directly (see each call site).

use crate::app::HiveApp;
use crate::ui::theme;
use eframe::egui;
use std::path::PathBuf;
use switchbard_core::{GoalMeasure, NewGoal};

/// One repo the modal's target-repo picker can offer — the `(key, label)`
/// pair every caller already has from its own repo listing (`Snapshot`'s
/// `RepoRow`s in `ui::backlog`, the Goals place's own scoped-repo list).
pub(crate) struct GoalModalRepoOption {
    pub key: PathBuf,
    pub label: String,
}

/// Open the goal composer. Every entry point (the Digest goals-section
/// header, its zero-goals affordance, and the Goals place's own "+ New
/// goal") shares this so repo targeting cannot drift.
pub(crate) fn open_new_goal(app: &mut HiveApp, target_repo: Option<PathBuf>) {
    app.backlog_view.new_goal.target_repo = target_repo;
    app.backlog_view.new_goal.open = true;
}

/// Renders the modal if open. Returns `Some((repo_root, goal))` the exact
/// frame the user clicks Create — never writes anything itself, so callers
/// stay in charge of how the write is dispatched (queued vs. spawned
/// immediately).
pub(crate) fn render_goal_modal(
    app: &mut HiveApp,
    ctx: &egui::Context,
    repo_options: &[GoalModalRepoOption],
    known_project_names: &[String],
    fixed_target: bool,
) -> Option<(PathBuf, NewGoal)> {
    if !app.backlog_view.new_goal.open {
        return None;
    }
    // With no explicit choice yet, seed the picker with the first offered
    // repo; with nothing tracked there is nothing to create into.
    if app.backlog_view.new_goal.target_repo.is_none() {
        app.backlog_view.new_goal.target_repo = repo_options.first().map(|opt| opt.key.clone());
    }
    let Some(target_key) = app.backlog_view.new_goal.target_repo.clone() else {
        app.backlog_view.new_goal.open = false;
        return None;
    };
    let Some(target) = repo_options.iter().find(|opt| opt.key == target_key) else {
        app.backlog_view.new_goal.open = false;
        return None;
    };
    let target_label = target.label.clone();

    let mut open = true;
    let mut close = false;
    let mut created = None;
    egui::Window::new("New Goal")
        .id(egui::Id::new("backlog_new_goal_modal"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if fixed_target {
                ui.label(egui::RichText::new(&target_label).color(theme::muted_text()));
            } else {
                ui.label(egui::RichText::new("Repo").color(theme::muted_text()));
                egui::ComboBox::from_id_salt("backlog_new_goal_repo")
                    .selected_text(&target_label)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for opt in repo_options {
                            let selected =
                                app.backlog_view.new_goal.target_repo.as_deref() == Some(&opt.key);
                            if ui.selectable_label(selected, &opt.label).clicked() {
                                app.backlog_view.new_goal.target_repo = Some(opt.key.clone());
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
                    egui::ComboBox::from_id_salt("backlog_new_goal_scope")
                        .selected_text("pick")
                        .width(72.0)
                        .show_ui(ui, |ui| {
                            for name in known_project_names {
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
                    created = Some((
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
    created
}
