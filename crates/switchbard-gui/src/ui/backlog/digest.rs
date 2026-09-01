//! The Digest lens (task-21): "what should I do today" — the Backlog tab's
//! default landing screen. Four sections, each capped and cross-repo:
//! overdue, newly unblocked, in progress, and recently done. Each section is
//! an entry point back into triage — clicking a task jumps to the List lens
//! with it selected; each section header can jump to the List lens filtered
//! to match.
//!
//! Unlike the List/Board lenses, Digest reads directly from `scoped_repos`
//! rather than the toolbar-filtered `tasks` list — the same choice `stats.rs`
//! makes. A "Recently Done" section would otherwise render empty whenever
//! the user has the Done filter off elsewhere, which defeats the point of a
//! landing page.

use super::{format, scoped_repos, Pending, RepoRow, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogLens;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{is_newly_unblocked, triage_entry_from_task, BacklogTask, TriageDue};

/// Cap per section — a digest is a glance, not another full list.
const SECTION_LIMIT: usize = 6;

struct DigestRow<'a> {
    repo: &'a RepoRow,
    task: &'a BacklogTask,
    /// Section-specific context line (e.g. which dependency just cleared).
    subtitle: Option<String>,
}

/// IA V2 (TASK-96) transition routing: the Digest **place**'s body —
/// everything `render_digest` below draws, wrapped in exactly the snapshot
/// collection / pending-mutation plumbing `ui::backlog::render` would
/// otherwise supply. Deliberately does **not** call the whole
/// `ui::backlog::render`: that would also draw the lens-tabs/toolbar chrome
/// meant for the *Tasks* place, and TASK-96's routing map calls for "the
/// existing Backlog Digest lens body" here, not "the whole Backlog view"
/// (contrast `Place::Tasks`'s routing, which does reuse the whole thing).
///
/// Still renders the persistent detail rail (`rail::render_detail_rail`) so
/// clicking a task in any Digest section shows its detail exactly as it
/// would from the Tasks place — Digest place and Tasks place share one
/// `backlog_view.selected_task`, deliberately: there is only one "the task
/// you're looking at" per the owner UX pass's rail doc.
pub(crate) fn render_digest_place(app: &mut HiveApp, ui: &mut egui::Ui) {
    let snap = Snapshot::collect(app);
    let mut pending = super::Pending::default();

    if !snap.repos.is_empty() {
        super::rail::render_detail_rail(app, ui, &snap, &mut pending);
    }

    let frame = egui::Frame::central_panel(&ui.ctx().style_of(ui.ctx().theme()))
        .inner_margin(egui::Margin::same(12));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        if snap.repos.is_empty() {
            render_empty_digest(ui);
            return;
        }
        render_digest(app, ui, &snap, &mut pending);
    });

    let ctx = &ui.ctx().clone();
    render_goal_modal_for_digest(app, ctx, &snap, &mut pending);
    super::apply_pending(app, ui, pending);
}

/// Builds this module's own `GoalModalRepoOption`/known-project-name inputs
/// from the Digest's `Snapshot` and calls the now-shared (TASK-101)
/// `goal_create::render_goal_modal`, queuing a create onto `pending` exactly
/// as before the modal's signature stopped taking `Snapshot`/`Pending`
/// directly — see that function's own doc for why (the Goals place needs the
/// identical modal and cannot name either private type).
fn render_goal_modal_for_digest(
    app: &mut HiveApp,
    ctx: &egui::Context,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let repo_options: Vec<super::goal_create::GoalModalRepoOption> = snap
        .repos
        .iter()
        .map(|row| super::goal_create::GoalModalRepoOption {
            key: row.key.clone(),
            label: row.label(),
        })
        .collect();
    let known_project_names = super::detail::known_project_names(snap);
    let fixed_target = app.backlog_view.selected_repo.is_some();
    if let Some((project_root, goal)) = super::goal_create::render_goal_modal(
        app,
        ctx,
        &repo_options,
        &known_project_names,
        fixed_target,
    ) {
        pending.goal_create = Some((project_root, goal));
    }
}

fn render_empty_digest(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading("Digest");
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "No tracked worktrees have a backlog/config.yml or backlog/tasks directory.",
            )
            .color(theme::muted_text()),
        );
    });
}

pub(super) fn render_digest(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    snap: &Snapshot,
    pending: &mut Pending,
) {
    let scoped = scoped_repos(app, snap);
    let today_day = chrono::Utc::now().timestamp().div_euclid(86_400);

    let mut overdue = Vec::new();
    let mut newly_unblocked = Vec::new();
    let mut in_progress = Vec::new();
    let mut recently_done: Vec<(&RepoRow, &BacklogTask, i64)> = Vec::new();

    for repo in &scoped {
        for task in &repo.repo.tasks {
            if task.source == switchbard_core::BacklogTaskSource::Archived {
                continue;
            }
            if task.is_done() {
                if let Some(day) = task
                    .updated_date
                    .as_deref()
                    .and_then(switchbard_core::parse_backlog_day)
                {
                    recently_done.push((repo, task, day));
                }
                continue;
            }
            let entry = triage_entry_from_task(repo.key.clone(), &repo.repo_name, task, &repo.repo);
            if entry.due == TriageDue::Overdue {
                overdue.push(DigestRow {
                    repo,
                    task,
                    subtitle: None,
                });
            }
            if is_newly_unblocked(task, &repo.repo, today_day) {
                newly_unblocked.push(DigestRow {
                    repo,
                    task,
                    subtitle: Some("a dependency was just completed".to_string()),
                });
            }
            if task.status.eq_ignore_ascii_case("in progress") {
                in_progress.push(DigestRow {
                    repo,
                    task,
                    subtitle: None,
                });
            }
        }
    }
    recently_done.sort_by_key(|(_, _, day)| std::cmp::Reverse(*day));
    let recently_done: Vec<DigestRow<'_>> = recently_done
        .into_iter()
        .map(|(repo, task, _)| DigestRow {
            repo,
            task,
            subtitle: task.updated_date.clone(),
        })
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("backlog_digest")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_goals_section(app, ui, &scoped, pending);
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

/// "This week's goals" — the section leading the Digest when any scoped
/// repo has a goal with a target set for the current week; absent entirely
/// otherwise (a glance surface earns no empty shells). Cards show
/// actual/target, a pace pill, and a progress bar whose "today" tick marks
/// the elapsed-week fraction — fill past the tick reads as ahead, short of
/// it as behind, with no arithmetic. Manual goals carry an inline check-in;
/// task-derived goals state their scope instead. State matrix (design-state,
/// 2026-08-31): zero goals (section absent), on-track/behind/met/missed,
/// 0-target (full bar, met), long names (wrap), check-in failure (surfaces
/// via `backlog_status` like every other backlog write).
fn render_goals_section(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    scoped: &[&RepoRow],
    pending: &mut Pending,
) {
    let today = chrono::Local::now().date_naive();
    let week = switchbard_core::week_monday_of(today)
        .format("%Y-%m-%d")
        .to_string();
    // Per repo (not flattened): a check-in write needs the owning repo root.
    let per_repo: Vec<(&RepoRow, Vec<switchbard_core::GoalStatus>)> = scoped
        .iter()
        .map(|repo| {
            (
                *repo,
                switchbard_core::compute_goal_statuses(&[&repo.repo], &week, today),
            )
        })
        .filter(|(_, statuses)| !statuses.is_empty())
        .collect();
    if per_repo.is_empty() {
        // No section shell — but creating the first goal needs a doorway.
        if ui
            .small_button("+ Goal for this week")
            .on_hover_text("Define a weekly target (backlog/goals.yml)")
            .clicked()
        {
            super::goal_create::open_new_goal(app, app.backlog_view.selected_repo.clone());
        }
        ui.add_space(10.0);
        return;
    }

    let days_elapsed = per_repo[0].1[0].days_elapsed;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("This week's goals").strong().heading());
        ui.label(
            egui::RichText::new(format!("week of {week}  ·  day {days_elapsed} of 7"))
                .color(theme::muted_text()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+ Goal").clicked() {
                super::goal_create::open_new_goal(app, app.backlog_view.selected_repo.clone());
            }
        });
    });
    ui.separator();
    for (repo, statuses) in per_repo {
        for status in statuses {
            render_goal_card(app, ui, repo, &status, pending);
            ui.add_space(4.0);
        }
    }
    ui.add_space(14.0);
}

fn goal_pace_pill(ui: &mut egui::Ui, pace: switchbard_core::GoalPace) {
    use crate::ui::components::StatusKind;
    use switchbard_core::GoalPace;
    let (kind, label) = match pace {
        GoalPace::OnTrack => (StatusKind::Good, "on track"),
        GoalPace::Behind => (StatusKind::Warn, "behind"),
        GoalPace::Met => (StatusKind::Good, "met"),
        GoalPace::Missed => (StatusKind::Danger, "missed"),
    };
    crate::ui::components::status_pill(ui, kind, label, None);
}

fn render_goal_card(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    repo: &RepoRow,
    status: &switchbard_core::GoalStatus,
    pending: &mut Pending,
) {
    let frame = egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(10, 6));
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            let _ = theme::painted_dot(ui, theme::repo_rail_color(&repo.repo_name));
            ui.label(egui::RichText::new(&status.name).strong());
            goal_pace_pill(ui, status.pace);
            ui.label(
                egui::RichText::new(format!(
                    "{} / {} {}",
                    status.actual, status.target, status.unit
                ))
                .color(theme::weak_text()),
            );
            if let Some(scope) = &status.scope {
                ui.label(
                    egui::RichText::new(format!("auto · {scope}"))
                        .small()
                        .color(theme::muted_text()),
                );
            } else if let Some(date) = &status.last_checkin_date {
                ui.label(
                    egui::RichText::new(format!("checked in {date}"))
                        .small()
                        .color(theme::muted_text()),
                );
            }
            // TASK-96: the goal-kind favorite star, flush right — explicit
            // affordance, no auto-population.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let favorited = app.is_favorited(
                    switchbard_core::config::FavoriteKind::Goal,
                    &repo.key,
                    &status.name,
                );
                let hover = if favorited {
                    "Remove from Favorites"
                } else {
                    "Add to Favorites"
                };
                if theme::favorite_star_button(ui, favorited)
                    .on_hover_text(hover)
                    .clicked()
                {
                    app.toggle_favorite(
                        switchbard_core::config::FavoriteKind::Goal,
                        &repo.key,
                        &status.name,
                    );
                }
            });
        });
        ui.horizontal(|ui| {
            let bar =
                ui.add(egui::ProgressBar::new(status.progress_fraction()).desired_width(160.0));
            // The "today" tick: where the week clock sits on the same bar.
            // Skip it on terminal verdicts — the race is over.
            if matches!(
                status.pace,
                switchbard_core::GoalPace::OnTrack | switchbard_core::GoalPace::Behind
            ) {
                let x = bar.rect.left() + bar.rect.width() * status.week_fraction();
                ui.painter().vline(
                    x,
                    bar.rect.y_range().expand(2.0),
                    egui::Stroke::new(2.0, theme::muted_text()),
                );
            }
            if status.measure == switchbard_core::GoalMeasure::Manual {
                let key = (repo.key.clone(), status.name.clone());
                let draft = app
                    .backlog_view
                    .goal_checkin_drafts
                    .entry(key.clone())
                    .or_insert(status.actual);
                ui.add(egui::DragValue::new(draft).range(0..=i64::MAX));
                if ui
                    .small_button("Check in")
                    .on_hover_text("Record this week's value (cumulative, not an increment)")
                    .clicked()
                {
                    pending.goal_checkin = Some((
                        repo.key.clone(),
                        status.name.clone(),
                        status.week.clone(),
                        *app.backlog_view
                            .goal_checkin_drafts
                            .get(&key)
                            .expect("just inserted"),
                    ));
                }
            }
        });
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
    let key = (row.repo.key.clone(), row.task.id.clone());
    // `theme::card_bg()`, not `ui.visuals().extreme_bg_color` — the owner UX
    // pass repointed that egui slot to input fields (see theme.rs's doc).
    let frame = egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(10, 6));
    let resp = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let _ = theme::painted_dot(ui, theme::repo_rail_color(&row.repo.repo_name));
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&row.repo.repo_name)
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
        // Widen to "All repos" scope — a Digest card can surface a task
        // from any tracked repo regardless of the current single-repo
        // scope, so selecting it needs the rail to actually find it.
        app.backlog_view.selected_repo = None;
        app.backlog_view.selected_task = Some(key);
        app.backlog_view.editor.loaded_key = None;
    }
}
