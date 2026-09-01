//! The Goals **place** (IA V2, TASK-101) — the real Goals index (mock §5's
//! sibling table below the goal page: one row per goal, pace chip,
//! actual/target, inline check-in, edit-target, "+ New goal") and the goal
//! page itself (crumb, this-week card, history card, Inputs card over
//! TASK-92's attach/detach). Replaces the interim body TASK-96 routed
//! `Place::Goals` to (`ui::backlog::digest::render_goals_place`, now
//! deleted — its "This week's goals" *section* lives on unchanged as
//! `ui::backlog::digest::render_goals_section`, still serving the Digest
//! place/lens).
//!
//! Own module, outside the `ui::backlog` tree (Rust privacy: `Snapshot`/
//! `RepoRow`/`Pending` there are `pub(in crate::ui::backlog)`, so this place
//! cannot name them and does not try to). What genuinely gets reused rather
//! than forked: the goal-write core surface (`switchbard_core::{create_goal,
//! check_in_goal, edit_goal_target, attach_goal_inputs, detach_goal_inputs,
//! roll_goals}`), the session-only check-in draft map
//! (`HiveApp::backlog_view.goal_checkin_drafts` — the exact same map the
//! Digest goal cards read/write, so a draft typed on one surface is still
//! there on the other), the "New Goal" modal (`ui::backlog::goal_create`,
//! refactored to a `pub(crate)` signature that takes plain repo/project
//! lists instead of `ui::backlog`'s private types precisely so this module
//! could call it), and the pace-chip color convention (`StatusKind`).
//!
//! Selection (`HiveApp::goals_view.selected_goal`) is session-only, like
//! every other place's — see `GoalsPlaceState`'s own doc.

use crate::app::HiveApp;
use crate::runtime::{GoalAttachInputState, GoalEditTargetState};
use crate::ui::backlog::goal_create;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme::{self, Glyph};
use eframe::egui;
use std::path::{Path, PathBuf};
use switchbard_core::{
    compute_goal_statuses, config::FavoriteKind, week_monday_of, BacklogRepo, BacklogTask,
    BacklogTaskSource, GoalDef, GoalMeasure, GoalPace, GoalStatus,
};

/// One tracked repo's goal-relevant slice — this module's own stand-in for
/// `ui::backlog::Snapshot`/`RepoRow`, which this module cannot name (see
/// this file's header doc). Unscoped: `render_goals_place` builds the full
/// tracked-repo list once, and each renderer applies `app.repo_scope`
/// itself where scope should apply (the index) and skips it where a
/// specific goal is already selected (the goal page, favorites nav, and
/// pickers scoped to one goal's own repo — matching how the Digest strip
/// widens scope for a selected task).
struct GoalRepoRow {
    key: PathBuf,
    repo_name: String,
    repo: BacklogRepo,
}

fn all_goal_repos(app: &HiveApp) -> Vec<GoalRepoRow> {
    let repos = app.repos_snapshot();
    let worktrees = app.worktrees_snapshot();
    let mut rows: Vec<GoalRepoRow> = app
        .backlog_repos_snapshot()
        .into_iter()
        .map(|(key, repo)| {
            let repo_name = worktrees
                .iter()
                .find(|wt| wt.path == key)
                .map(|wt| wt.repo_name.clone())
                .or_else(|| repos.iter().find(|r| r.path == key).map(|r| r.name.clone()))
                .unwrap_or_else(|| {
                    key.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("repo")
                        .to_string()
                });
            GoalRepoRow {
                key,
                repo_name,
                repo,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        a.repo_name
            .cmp(&b.repo_name)
            .then_with(|| a.key.cmp(&b.key))
    });
    rows
}

pub(crate) fn render_goals_place(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let repos = all_goal_repos(app);
    reconcile_selected_goal(app, &repos);

    let frame =
        egui::Frame::central_panel(&ctx.style_of(ctx.theme())).inner_margin(egui::Margin::same(12));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("goals_place")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let selected = app.goals_view.selected_goal.clone();
                match selected {
                    Some((repo_key, goal_name)) => {
                        render_goal_page(app, ui, &repos, &repo_key, &goal_name)
                    }
                    None => render_index(app, ui, &repos),
                }
            });
    });

    render_new_goal_modal(app, &ctx, &repos);
    render_edit_target_modal(app, &ctx);
    render_attach_input_modal(app, &ctx, &repos);
}

/// Drop a selection that no longer names a real goal (repo untracked, or
/// the goal deleted from under it) — falls back to the index, never a blank
/// page. Cheap: `repos` is already collected once per frame by the caller.
fn reconcile_selected_goal(app: &mut HiveApp, repos: &[GoalRepoRow]) {
    if let Some((root, name)) = &app.goals_view.selected_goal {
        let exists = repos
            .iter()
            .any(|r| &r.key == root && r.repo.goals.iter().any(|g| &g.name == name));
        if !exists {
            app.goals_view.selected_goal = None;
        }
    }
}

fn goal_pace_pill(ui: &mut egui::Ui, pace: GoalPace) {
    let (kind, label) = match pace {
        GoalPace::OnTrack => (StatusKind::Good, "on track"),
        GoalPace::Behind => (StatusKind::Warn, "behind"),
        GoalPace::Met => (StatusKind::Good, "met"),
        GoalPace::Missed => (StatusKind::Danger, "missed"),
    };
    status_pill(ui, kind, label, None);
}

fn scoped<'a>(app: &HiveApp, repos: &'a [GoalRepoRow]) -> Vec<&'a GoalRepoRow> {
    repos
        .iter()
        .filter(|row| crate::runtime::path_in_scope(&row.key, &app.repo_scope))
        .collect()
}

// ---------------------------------------------------------------------
// Index — mock §5's Goals table: one row per goal this week, pace chip,
// actual/target, inline check-in, edit target, "+ New goal".
// ---------------------------------------------------------------------

fn render_index(app: &mut HiveApp, ui: &mut egui::Ui, repos: &[GoalRepoRow]) {
    let today = chrono::Local::now().date_naive();
    let week = week_monday_of(today).format("%Y-%m-%d").to_string();
    let scoped_rows = scoped(app, repos);

    let mut rows: Vec<(PathBuf, String, GoalStatus)> = Vec::new();
    for row in &scoped_rows {
        for status in compute_goal_statuses(&[&row.repo], &week, today) {
            rows.push((row.key.clone(), row.repo_name.clone(), status));
        }
    }
    let show_repo_dot = scoped_rows.len() > 1;

    egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Goals").strong().heading());
                ui.label(
                    egui::RichText::new(format!("week of {week}"))
                        .small()
                        .color(theme::muted_text()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::icon_button_label(theme::painted_plus_button(ui), "New goal")
                        .clicked()
                    {
                        goal_create::open_new_goal(app, scoped_rows.first().map(|r| r.key.clone()));
                    }
                });
            });
            ui.separator();
            if rows.is_empty() {
                render_empty_state(app, ui, &scoped_rows, &week);
                return;
            }
            egui::Grid::new("goals_index_grid")
                .striped(true)
                .num_columns(5)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    for label in ["Goal", "Pace", "", "Check in", ""] {
                        ui.label(
                            egui::RichText::new(label)
                                .small()
                                .color(theme::muted_text()),
                        );
                    }
                    ui.end_row();
                    for (repo_key, repo_name, status) in &rows {
                        render_index_row(app, ui, repo_key, repo_name, status, show_repo_dot);
                    }
                });
        });
}

fn render_index_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    repo_key: &Path,
    repo_name: &str,
    status: &GoalStatus,
    show_repo_dot: bool,
) {
    ui.horizontal(|ui| {
        if show_repo_dot {
            let _ = theme::painted_dot(ui, theme::repo_rail_color(repo_name));
        }
        let resp = ui.add(
            egui::Label::new(egui::RichText::new(&status.name).strong())
                .sense(egui::Sense::click()),
        );
        if resp.clicked() {
            app.goals_view.selected_goal = Some((repo_key.to_path_buf(), status.name.clone()));
        }
    });
    goal_pace_pill(ui, status.pace);
    ui.label(
        egui::RichText::new(format!(
            "{} / {} {}",
            status.actual, status.target, status.unit
        ))
        .color(theme::weak_text()),
    );
    match status.measure {
        GoalMeasure::Tasks => {
            ui.label(
                egui::RichText::new("automatic")
                    .small()
                    .color(theme::muted_text()),
            );
        }
        GoalMeasure::Manual => {
            let key = (repo_key.to_path_buf(), status.name.clone());
            ui.horizontal(|ui| {
                let draft = app
                    .backlog_view
                    .goal_checkin_drafts
                    .entry(key.clone())
                    .or_insert(status.actual);
                ui.add(egui::DragValue::new(draft).range(0..=i64::MAX));
                if theme::icon_button_label(theme::painted_check_button(ui), "Check in").clicked() {
                    let value = *app
                        .backlog_view
                        .goal_checkin_drafts
                        .get(&key)
                        .expect("just inserted");
                    app.spawn_goal_checkin(
                        repo_key.to_path_buf(),
                        status.name.clone(),
                        status.week.clone(),
                        value,
                        ui.ctx(),
                    );
                }
            });
        }
    }
    if theme::icon_button_label(theme::painted_pencil_button(ui), "Edit target").clicked() {
        app.goals_view.edit_target = GoalEditTargetState {
            open: true,
            repo_root: Some(repo_key.to_path_buf()),
            goal_name: status.name.clone(),
            week: status.week.clone(),
            target: status.target,
        };
    }
    ui.end_row();
}

/// Mock §7a: the live zero-goals state (`goals.yml` may not exist yet).
fn render_empty_state(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    scoped_rows: &[&GoalRepoRow],
    week: &str,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(egui::RichText::new("No goals this week").strong());
        ui.label(
            egui::RichText::new(format!("Week of {week} has no targets set yet."))
                .color(theme::muted_text()),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("+ New goal").clicked() {
                goal_create::open_new_goal(app, scoped_rows.first().map(|r| r.key.clone()));
            }
            if ui
                .button("Roll last week")
                .on_hover_text("Carry every existing goal's most recent target into this week")
                .clicked()
            {
                for row in scoped_rows {
                    app.spawn_goal_roll(row.key.clone(), week.to_string(), ui.ctx());
                }
            }
        });
        ui.add_space(8.0);
    });
}

fn render_new_goal_modal(app: &mut HiveApp, ctx: &egui::Context, repos: &[GoalRepoRow]) {
    let scoped_rows = scoped(app, repos);
    let repo_options: Vec<goal_create::GoalModalRepoOption> = scoped_rows
        .iter()
        .map(|row| goal_create::GoalModalRepoOption {
            key: row.key.clone(),
            label: row.repo_name.clone(),
            project_names: row.repo.project_names(),
        })
        .collect();
    let fixed_target = repo_options.len() <= 1;
    if let Some((project_root, goal)) =
        goal_create::render_goal_modal(app, ctx, &repo_options, fixed_target)
    {
        app.spawn_goal_create(project_root, goal, ctx);
    }
}

fn render_edit_target_modal(app: &mut HiveApp, ctx: &egui::Context) {
    if !app.goals_view.edit_target.open {
        return;
    }
    let goal_name = app.goals_view.edit_target.goal_name.clone();
    let week = app.goals_view.edit_target.week.clone();
    let mut open = true;
    let mut close = false;
    egui::Window::new("Edit target")
        .id(egui::Id::new("goals_edit_target_modal"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(&goal_name).strong());
            ui.label(
                egui::RichText::new(format!("week of {week}"))
                    .small()
                    .color(theme::muted_text()),
            );
            ui.horizontal(|ui| {
                ui.label("target");
                ui.add(
                    egui::DragValue::new(&mut app.goals_view.edit_target.target)
                        .range(0..=i64::MAX),
                );
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    if let Some(root) = app.goals_view.edit_target.repo_root.clone() {
                        app.spawn_goal_edit_target(
                            root,
                            goal_name.clone(),
                            week.clone(),
                            app.goals_view.edit_target.target,
                            ctx,
                        );
                    }
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close || !open {
        app.goals_view.edit_target.open = false;
    }
}

fn render_attach_input_modal(app: &mut HiveApp, ctx: &egui::Context, repos: &[GoalRepoRow]) {
    if !app.goals_view.attach_input.open {
        return;
    }
    let Some(repo_root) = app.goals_view.attach_input.repo_root.clone() else {
        app.goals_view.attach_input.open = false;
        return;
    };
    let Some(row) = repos.iter().find(|r| r.key == repo_root) else {
        app.goals_view.attach_input.open = false;
        return;
    };
    let goal_name = app.goals_view.attach_input.goal_name.clone();
    let Some(goal_def) = row.repo.goals.iter().find(|g| g.name == goal_name) else {
        app.goals_view.attach_input.open = false;
        return;
    };

    let mut project_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for task in &row.repo.tasks {
        if let Some(p) = &task.project {
            project_names.insert(p.clone());
        }
    }
    for def in &row.repo.project_defs {
        project_names.insert(def.name.clone());
    }
    let query_lc = app.goals_view.attach_input.query.to_lowercase();
    let projects: Vec<String> = project_names
        .into_iter()
        .filter(|p| !goal_def.inputs.projects.iter().any(|a| a == p))
        .filter(|p| query_lc.is_empty() || p.to_lowercase().contains(&query_lc))
        .collect();
    let tasks: Vec<&BacklogTask> = row
        .repo
        .tasks
        .iter()
        .filter(|t| t.source != BacklogTaskSource::Archived)
        .filter(|t| {
            !goal_def
                .inputs
                .tasks
                .iter()
                .any(|a| a.eq_ignore_ascii_case(&t.id))
        })
        .filter(|t| {
            query_lc.is_empty()
                || t.id.to_lowercase().contains(&query_lc)
                || t.title.to_lowercase().contains(&query_lc)
        })
        .take(30)
        .collect();

    let mut open = true;
    let mut close = false;
    let mut attach_task: Option<String> = None;
    let mut attach_project: Option<String> = None;
    egui::Window::new("Attach task or project")
        .id(egui::Id::new("goals_attach_input_modal"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.goals_view.attach_input.query)
                    .hint_text("Filter by name or id")
                    .desired_width(340.0),
            );
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    if projects.is_empty() && tasks.is_empty() {
                        ui.label(
                            egui::RichText::new("Nothing left to attach")
                                .color(theme::muted_text()),
                        );
                        return;
                    }
                    if !projects.is_empty() {
                        ui.label(
                            egui::RichText::new("Projects")
                                .small()
                                .color(theme::muted_text()),
                        );
                        for project in &projects {
                            ui.horizontal(|ui| {
                                theme::painted_glyph(ui, Glyph::Project, theme::muted_text());
                                ui.label(project);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("Attach").clicked() {
                                            attach_project = Some(project.clone());
                                        }
                                    },
                                );
                            });
                        }
                    }
                    if !tasks.is_empty() {
                        ui.label(
                            egui::RichText::new("Tasks")
                                .small()
                                .color(theme::muted_text()),
                        );
                        for task in &tasks {
                            ui.horizontal(|ui| {
                                theme::painted_glyph(ui, Glyph::Tasks, theme::muted_text());
                                ui.label(egui::RichText::new(&task.id).monospace().small());
                                ui.label(egui::RichText::new(&task.title).small());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("Attach").clicked() {
                                            attach_task = Some(task.id.clone());
                                        }
                                    },
                                );
                            });
                        }
                    }
                });
            ui.add_space(6.0);
            if ui.button("Close").clicked() {
                close = true;
            }
        });

    if let Some(task_id) = attach_task {
        app.spawn_goal_attach_input(
            repo_root.clone(),
            goal_name.clone(),
            vec![task_id],
            vec![],
            ctx,
        );
    }
    if let Some(project) = attach_project {
        app.spawn_goal_attach_input(
            repo_root.clone(),
            goal_name.clone(),
            vec![],
            vec![project],
            ctx,
        );
    }
    if close || !open {
        app.goals_view.attach_input.open = false;
    }
}

// ---------------------------------------------------------------------
// Goal page — mock §5: crumb, this-week card, history card, Inputs card.
// ---------------------------------------------------------------------

fn render_goal_page(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    repos: &[GoalRepoRow],
    repo_key: &Path,
    goal_name: &str,
) {
    let Some(row) = repos.iter().find(|r| r.key.as_path() == repo_key) else {
        return;
    };
    let Some(goal_def) = row.repo.goals.iter().find(|g| g.name == goal_name) else {
        return;
    };

    let today = chrono::Local::now().date_naive();
    let week = week_monday_of(today).format("%Y-%m-%d").to_string();
    let status = compute_goal_statuses(&[&row.repo], &week, today)
        .into_iter()
        .find(|s| s.name == goal_name);

    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::Label::new(egui::RichText::new("Goals").color(theme::muted_text()))
                .sense(egui::Sense::click()),
        );
        if resp.clicked() {
            app.goals_view.selected_goal = None;
        }
        ui.label(egui::RichText::new("/").color(theme::muted_text()));
        ui.label(egui::RichText::new(goal_name).strong());
    });
    ui.horizontal(|ui| {
        ui.heading(goal_name);
        if let Some(status) = &status {
            goal_pace_pill(ui, status.pace);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let favorited = app.is_favorited(FavoriteKind::Goal, repo_key, goal_name);
            let hover = if favorited {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            };
            if theme::icon_button_label(theme::favorite_star_button(ui, favorited), hover).clicked()
            {
                app.toggle_favorite(FavoriteKind::Goal, repo_key, goal_name);
            }
        });
    });
    let hsub = match (goal_def.measure, &goal_def.scope) {
        (GoalMeasure::Tasks, Some(scope)) => format!(
            "Counts {scope} tasks done this week - progress is automatic, no manual check-ins"
        ),
        (GoalMeasure::Tasks, None) => {
            "Counts attached inputs done this week - progress is automatic, no manual check-ins"
                .to_string()
        }
        (GoalMeasure::Manual, _) => "Reported by weekly check-ins".to_string(),
    };
    ui.label(egui::RichText::new(hsub).small().color(theme::muted_text()));
    ui.add_space(8.0);

    ui.columns(2, |cols| {
        render_this_week_card(app, &mut cols[0], row, goal_def, status.as_ref(), &week);
        render_history_card(&mut cols[1], goal_def, &row.repo, today);
    });
    ui.add_space(10.0);
    render_inputs_card(app, ui, row, goal_def, &week);
}

fn render_this_week_card(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &GoalRepoRow,
    goal_def: &GoalDef,
    status: Option<&GoalStatus>,
    week: &str,
) {
    let monday = chrono::NaiveDate::parse_from_str(week, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::Local::now().date_naive());
    let sunday = monday
        .checked_add_days(chrono::Days::new(6))
        .unwrap_or(monday);
    egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "This week · {} - {}",
                    monday.format("%b %-d"),
                    sunday.format("%b %-d")
                ))
                .small()
                .color(theme::muted_text()),
            );
            match status {
                Some(status) => {
                    let pct = (status.week_fraction() * 100.0).round() as i64;
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {} {} · {pct}% of week elapsed",
                            status.actual, status.target, status.unit
                        ))
                        .heading(),
                    );
                    ui.add(egui::ProgressBar::new(status.progress_fraction()).desired_width(220.0));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Roll into next week").clicked() {
                            let next = monday
                                .checked_add_days(chrono::Days::new(7))
                                .unwrap_or(monday)
                                .format("%Y-%m-%d")
                                .to_string();
                            app.spawn_goal_roll(row.key.clone(), next, ui.ctx());
                        }
                        if theme::icon_button_label(theme::painted_pencil_button(ui), "Edit target")
                            .clicked()
                        {
                            app.goals_view.edit_target = GoalEditTargetState {
                                open: true,
                                repo_root: Some(row.key.clone()),
                                goal_name: goal_def.name.clone(),
                                week: status.week.clone(),
                                target: status.target,
                            };
                        }
                    });
                    if goal_def.measure == GoalMeasure::Tasks {
                        ui.label(
                            egui::RichText::new("progress counts automatically")
                                .small()
                                .color(theme::muted_text()),
                        );
                    }
                }
                None => {
                    ui.label(
                        egui::RichText::new("No target set for this week yet.")
                            .color(theme::muted_text()),
                    );
                    if ui
                        .button("Roll last week")
                        .on_hover_text("Carry the most recent target into this week")
                        .clicked()
                    {
                        app.spawn_goal_roll(row.key.clone(), week.to_string(), ui.ctx());
                    }
                }
            }
        });
}

fn render_history_card(
    ui: &mut egui::Ui,
    goal_def: &GoalDef,
    repo: &BacklogRepo,
    today: chrono::NaiveDate,
) {
    egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("History · weekly outcomes")
                    .small()
                    .color(theme::muted_text()),
            );
            // Real recorded weeks only (`goal_def.weeks`, chronological via
            // `BTreeMap`'s ISO-date keys) — never invented history. A
            // first-ever goal renders exactly one bar.
            if goal_def.weeks.is_empty() {
                ui.label(egui::RichText::new("No recorded weeks yet.").color(theme::muted_text()));
                return;
            }
            let bars: Vec<(String, GoalPace, f32)> = goal_def
                .weeks
                .keys()
                .map(|week_key| {
                    let status = compute_goal_statuses(&[repo], week_key, today)
                        .into_iter()
                        .find(|s| s.name == goal_def.name);
                    let (pace, frac) = status
                        .map(|s| (s.pace, s.progress_fraction()))
                        .unwrap_or((GoalPace::Behind, 0.0));
                    let label = chrono::NaiveDate::parse_from_str(week_key, "%Y-%m-%d")
                        .map(|d| d.format("%b %-d").to_string())
                        .unwrap_or_else(|_| week_key.clone());
                    (label, pace, frac)
                })
                .collect();
            render_history_bars(ui, &bars);
        });
}

/// Bars: filled for a terminal verdict (green = met, warn = missed);
/// outline-only for a week still in progress (on-track/behind) — mock §5's
/// own legend ("outline = in progress").
fn render_history_bars(ui: &mut egui::Ui, bars: &[(String, GoalPace, f32)]) {
    let bar_w = 26.0;
    let gap = 14.0;
    let area_h = 56.0;
    let n = bars.len().max(1) as f32;
    let width = gap + n * (bar_w + gap);
    let (rect, _resp) =
        ui.allocate_exact_size(egui::vec2(width, area_h + 20.0), egui::Sense::hover());
    let painter = ui.painter();
    let base_y = rect.top() + area_h;
    painter.hline(
        rect.left()..=rect.right(),
        base_y,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    for (i, (label, pace, frac)) in bars.iter().enumerate() {
        let x = rect.left() + gap + i as f32 * (bar_w + gap);
        let h = (area_h * frac.clamp(0.0, 1.0)).max(4.0);
        let bar_rect =
            egui::Rect::from_min_max(egui::pos2(x, base_y - h), egui::pos2(x + bar_w, base_y));
        match pace {
            GoalPace::Met => {
                painter.rect_filled(bar_rect, 3.0, theme::green());
            }
            GoalPace::Missed => {
                painter.rect_filled(bar_rect, 3.0, theme::warn_orange());
            }
            GoalPace::OnTrack | GoalPace::Behind => {
                painter.rect_stroke(
                    bar_rect,
                    3.0,
                    egui::Stroke::new(1.5, theme::muted_text()),
                    egui::StrokeKind::Outside,
                );
            }
        }
        painter.text(
            egui::pos2(x + bar_w / 2.0, base_y + 4.0),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::proportional(10.0),
            theme::muted_text(),
        );
    }
}

fn render_inputs_card(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &GoalRepoRow,
    goal_def: &GoalDef,
    week: &str,
) {
    egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Inputs · what this goal counts")
                    .small()
                    .color(theme::muted_text()),
            );
            if goal_def.measure != GoalMeasure::Tasks {
                ui.label(
                    egui::RichText::new(
                        "Manual goals don't attach inputs — they take weekly check-ins.",
                    )
                    .color(theme::muted_text()),
                );
                return;
            }
            if goal_def.inputs.is_empty() {
                ui.label(egui::RichText::new("Nothing attached yet.").color(theme::muted_text()));
            } else {
                egui::Grid::new("goals_inputs_grid")
                    .num_columns(4)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        for project in &goal_def.inputs.projects {
                            render_project_input_row(app, ui, row, goal_def, project, week);
                        }
                        for task_id in &goal_def.inputs.tasks {
                            render_task_input_row(app, ui, row, goal_def, task_id);
                        }
                    });
            }
            ui.add_space(6.0);
            if ui.button("+ Attach task or project").clicked() {
                app.goals_view.attach_input = GoalAttachInputState {
                    open: true,
                    repo_root: Some(row.key.clone()),
                    goal_name: goal_def.name.clone(),
                    query: String::new(),
                };
            }
        });
}

fn render_project_input_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &GoalRepoRow,
    goal_def: &GoalDef,
    project: &str,
    week: &str,
) {
    let week_end = chrono::NaiveDate::parse_from_str(week, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.checked_add_days(chrono::Days::new(6)))
        .map(|d| d.format("%Y-%m-%d").to_string());
    let member_tasks: Vec<&BacklogTask> = row
        .repo
        .tasks
        .iter()
        .filter(|t| t.source != BacklogTaskSource::Archived)
        .filter(|t| t.project.as_deref() == Some(project))
        .collect();
    let done_this_week = member_tasks
        .iter()
        .filter(|t| {
            t.is_done()
                && t.updated_date
                    .as_deref()
                    .is_some_and(|d| d >= week && week_end.as_deref().is_some_and(|end| d <= end))
        })
        .count();

    ui.horizontal(|ui| {
        theme::painted_glyph(ui, Glyph::Project, theme::muted_text());
    });
    ui.label(format!("{project} · project"));
    ui.label(
        egui::RichText::new(format!(
            "{} task{} · {done_this_week} done this week",
            member_tasks.len(),
            if member_tasks.len() == 1 { "" } else { "s" }
        ))
        .color(theme::weak_text()),
    );
    if theme::icon_button_label(
        theme::painted_trash_button(ui, theme::weak_text()),
        "Detach input",
    )
    .clicked()
    {
        app.spawn_goal_detach_input(
            row.key.clone(),
            goal_def.name.clone(),
            vec![],
            vec![project.to_string()],
            ui.ctx(),
        );
    }
    ui.end_row();
}

fn render_task_input_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &GoalRepoRow,
    goal_def: &GoalDef,
    task_id: &str,
) {
    let task = row
        .repo
        .tasks
        .iter()
        .find(|t| t.id.eq_ignore_ascii_case(task_id));
    ui.horizontal(|ui| {
        theme::painted_glyph(ui, Glyph::Tasks, theme::muted_text());
    });
    match task {
        Some(task) => {
            ui.label(format!("{} · {}", task.id, task.title));
            ui.label(egui::RichText::new(&task.status).color(theme::weak_text()));
        }
        None => {
            ui.label(
                egui::RichText::new(format!("{task_id} (not found)")).color(theme::muted_text()),
            );
            ui.label("");
        }
    }
    if theme::icon_button_label(
        theme::painted_trash_button(ui, theme::weak_text()),
        "Detach input",
    )
    .clicked()
    {
        app.spawn_goal_detach_input(
            row.key.clone(),
            goal_def.name.clone(),
            vec![task_id.to_string()],
            vec![],
            ui.ctx(),
        );
    }
    ui.end_row();
}
