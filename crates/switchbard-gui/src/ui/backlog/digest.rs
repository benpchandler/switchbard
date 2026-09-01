//! The Digest **lens** (task-21): "what should I do today" — one of the
//! Tasks place's own internal lens tabs (`BacklogLens::Digest`, reachable
//! from `ui::backlog::toolbar::render_lens_tabs` while `Place::Tasks` /
//! `TasksView::All` is active). Four sections, each capped and cross-repo:
//! overdue, newly unblocked, in progress, and recently done.
//!
//! **Not to be confused with the Digest *place*** (`ui::places::digest`,
//! TASK-99) — the sidebar's own top-level landing surface, a different mock
//! section entirely (goal cards / in-flight / attention feed) reached
//! without ever opening Tasks. TASK-96's routing map originally pointed
//! `Place::Digest` at this lens's body as an interim measure; TASK-99
//! replaced that routing with `ui::places::digest::render`, but this lens
//! and its tab stay reachable exactly as before — nav.rs's own doc is
//! explicit that nothing licenses deleting Backlog lens code out from under
//! Tasks, and this file's four sections are still that lens's real content.
//!
//! Unlike the List/Board lenses, Digest reads directly from `scoped_repos`
//! rather than the toolbar-filtered `tasks` list — the same choice `stats.rs`
//! makes. A "Recently Done" section would otherwise render empty whenever
//! the user has the Done filter off elsewhere, which defeats the point of a
//! landing page.
//!
//! ## Goal-card machinery, shared with the Digest place
//!
//! [`render_goal_card`] (and its pace pill / meter / check-in draft
//! plumbing) is reused rather than forked by TASK-99's
//! [`render_goal_cards_for_digest_place`] — see that function's own doc for
//! why the header/empty-state framing still differs from
//! [`render_goals_section`] (this lens's own "This week's goals" heading)
//! while every behavior-bearing piece is one implementation. The Goals
//! **place** (TASK-101, `ui::places::goals`) is a real, independent index
//! now and does not call into either — it builds its own rows.

use super::{format, scoped_repos, Pending, RepoRow, Snapshot};
use crate::app::HiveApp;
use crate::runtime::BacklogLens;
use crate::ui::components::StatusKind;
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{
    is_newly_unblocked, triage_entry_from_task, BacklogTask, GoalMeasure, TriageDue,
};

/// Cap per section — a digest is a glance, not another full list.
const SECTION_LIMIT: usize = 6;

struct DigestRow<'a> {
    repo: &'a RepoRow,
    task: &'a BacklogTask,
    /// Section-specific context line (e.g. which dependency just cleared).
    subtitle: Option<String>,
}

/// TASK-99: the Digest place's goal-cards section (mock §1's `goalrow` /
/// §7a's empty state). Fully self-contained — builds and applies its own
/// `Pending` — so `ui::places::digest` never needs to see `RepoRow`/
/// `Pending`/`Snapshot`, all private to this module tree; it just calls this
/// one function with `(app, ui)`.
///
/// Deliberately does **not** reuse `render_goals_section`: that function's
/// "This week's goals" heading + inline "+ Goal" button belong to the Goals
/// **place**'s own body now (TASK-101, `ui::places::goals`, a fully
/// independent module this task must not change out from under it). The two
/// share every behavior-bearing piece instead: the same
/// `compute_goal_statuses` call, the same [`render_goal_card`] per status,
/// the same `goal_create::render_goal_modal` plumbing (TASK-101's
/// `pub(crate)` refactor — plain `GoalModalRepoOption` data instead of this
/// module's private `Snapshot`/`Pending` — is exactly the shape this
/// function already needed).
pub(crate) fn render_goal_cards_for_digest_place(app: &mut HiveApp, ui: &mut egui::Ui) {
    let snap = Snapshot::collect(app);
    // Sidebar scope ONLY — deliberately not `scoped_repos`, which also
    // applies `backlog_view.selected_repo`, the Tasks place's own single-repo
    // picker. That picker is invisible on Digest, and a places surface
    // aggregates over the sidebar's multi-select scope by IA ruling
    // (TASK-76 round 1); the place's other sections (`ui::places::digest::
    // collect_task_rows`) already filter by exactly this rule. Owner-reported
    // bug: with the Tasks picker parked on a repo with no goals, Digest
    // rendered the zero-goal state while another scoped repo had three
    // current-week goals.
    let scoped: Vec<&RepoRow> = snap
        .repos
        .iter()
        .filter(|row| crate::runtime::path_in_scope(&row.key, &app.repo_scope))
        .collect();
    let mut pending = Pending::default();
    let today = chrono::Local::now().date_naive();
    let week = switchbard_core::week_monday_of(today)
        .format("%Y-%m-%d")
        .to_string();
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
        let scoped_roots: Vec<std::path::PathBuf> =
            scoped.iter().map(|repo| repo.key.clone()).collect();
        render_zero_goal_state(app, ui, &week, &scoped_roots);
    } else {
        // TASK-76 parity pass: mock §1's `goalrow` is a 3-across grid of
        // compact, read-only glance tiles — a different shape from
        // `render_goal_card`'s full-width, inline-editable row (still used
        // verbatim by the Goals **place** index and the old Backlog Digest
        // **lens**, `render_goals_section`, neither of which this task
        // touches). An earlier attempt at the side-by-side layout reused
        // that row's frame directly inside a `horizontal_wrapped` and left a
        // second card invisibly overlapping the first, because that frame's
        // favorite-star `Layout::right_to_left` claims the rest of its
        // *row's* width to place itself — see the retired comment this
        // replaces. `render_compact_goal_grid` below owns an explicit width
        // per card instead of sharing one.
        let cards: Vec<(&RepoRow, &switchbard_core::GoalStatus)> = per_repo
            .iter()
            .flat_map(|(repo, statuses)| statuses.iter().map(move |status| (*repo, status)))
            .collect();
        render_compact_goal_grid(ui, &cards);
    }

    let ctx = &ui.ctx().clone();
    render_goal_modal_for_digest_place(app, ctx, &snap, &mut pending);
    super::apply_pending(app, ui, pending);
}

/// Builds this function's own `GoalModalRepoOption`/known-project-name
/// inputs from the Digest place's `Snapshot` and calls the shared
/// (TASK-101) `goal_create::render_goal_modal`, queuing a create onto
/// `pending` — the same adaptation `ui::backlog::render`'s own top level
/// does for the Tasks place, each caller building the plain-data view its
/// own `Snapshot` type supplies since `GoalModalRepoOption` cannot name
/// `RepoRow` itself.
fn render_goal_modal_for_digest_place(
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
    // Never a fixed target here: `selected_repo` is the Tasks place's own
    // picker, invisible from Digest — locking the modal's repo dropdown to
    // it would pin a choice the user cannot see (same family as the
    // goal-card scoping fix above).
    if let Some((project_root, goal)) =
        super::goal_create::render_goal_modal(app, ctx, &repo_options, &known_project_names, false)
    {
        pending.goal_create = Some((project_root, goal));
    }
}

/// Mock §7a, verbatim: "No goals this week" / "Week of `<monday>` ends
/// today." / **+ New goal** (existing `goal_create::open_new_goal` doorway)
/// / **Roll last week** (`HiveApp::spawn_goal_roll` — TASK-101 landed the
/// first GUI wiring of `roll_goals` for the Goals place's own "Roll into
/// next week"; this reuses that exact method rather than adding a second
/// one). Rolls every currently-scoped repo: the empty state has no single
/// repo to target the way a check-in or a new goal does, since by
/// definition nothing scoped has a goal for this week yet.
fn render_zero_goal_state(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    week: &str,
    scoped_roots: &[std::path::PathBuf],
) {
    let ctx = ui.ctx().clone();
    let frame = egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(14, 12));
    frame.show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No goals this week").strong());
            // `week` itself stays the ISO `%Y-%m-%d` key `roll_goals`/
            // `spawn_goal_roll` need — only the label gets the friendlier
            // "Aug 31" the page header (mock §1's chip) already uses. The
            // day count mirrors the header chip's own formula (`ui::places::
            // digest::render_header`) rather than the mock's literal "ends
            // today." copy, which was only true on the day the mock was
            // drawn — owner-reported as wrong on a Tuesday.
            let friendly = chrono::NaiveDate::parse_from_str(week, "%Y-%m-%d")
                .map(|d| d.format("%b %-d").to_string())
                .unwrap_or_else(|_| week.to_string());
            let today = chrono::Local::now().date_naive();
            let days_elapsed =
                ((today - switchbard_core::week_monday_of(today)).num_days() + 1).clamp(1, 7);
            ui.label(
                egui::RichText::new(format!("Week of {friendly} · day {days_elapsed} of 7."))
                    .color(theme::muted_text()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("+ New goal").clicked() {
                    // No preselected repo: `selected_repo` is the Tasks
                    // place's invisible-from-here picker (see the scoping
                    // note in `render_goal_cards_for_digest_place`).
                    super::goal_create::open_new_goal(app, None);
                }
                if ui
                    .button("Roll last week")
                    .on_hover_text(
                        "Carry every scoped repo's most recent earlier goal target into this week",
                    )
                    .clicked()
                {
                    for root in scoped_roots {
                        app.spawn_goal_roll(root.clone(), week.to_string(), &ctx);
                    }
                }
            });
        });
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

/// "This week's goals" — the section leading the Digest **lens** when any
/// scoped repo has a goal with a target set for the current week; absent
/// entirely otherwise (a glance surface earns no empty shells). Cards show
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

/// The one place a `GoalPace` becomes a chip kind + label — shared by
/// [`goal_pace_pill`] (the full-width row's pill) and the compact grid card
/// below, so the two can never disagree about what "behind" looks like.
fn goal_pace_kind_label(pace: switchbard_core::GoalPace) -> (StatusKind, &'static str) {
    use switchbard_core::GoalPace;
    match pace {
        GoalPace::OnTrack => (StatusKind::Good, "on track"),
        GoalPace::Behind => (StatusKind::Warn, "behind"),
        GoalPace::Met => (StatusKind::Good, "met"),
        GoalPace::Missed => (StatusKind::Danger, "missed"),
    }
}

fn goal_pace_pill(ui: &mut egui::Ui, pace: switchbard_core::GoalPace) {
    let (kind, label) = goal_pace_kind_label(pace);
    crate::ui::components::status_pill(ui, kind, label, None);
}

/// Minimum width one compact card claims before the grid drops a column —
/// mock §1's `goalrow` (`grid-template-columns:repeat(3,1fr)`) at the
/// mock's own `.page{max-width:1180px}` reading width, scaled down: three
/// cards need to stay legible (name + a big number + a chip pair) without
/// wrapping their own content, which this floor protects.
const COMPACT_CARD_MIN_WIDTH: f32 = 168.0;
const COMPACT_CARD_GAP: f32 = 8.0;
const COMPACT_CARD_MAX_COLUMNS: usize = 3;

/// Mock §1's `goalrow`: up to [`COMPACT_CARD_MAX_COLUMNS`] cards per line,
/// wrapping to further rows — collapsing toward a single column only when
/// the pane is genuinely too narrow for two, per the mission brief ("stack
/// only when the pane is genuinely narrow"). Every card gets an explicit
/// `allocate_ui` width instead of sharing the row's width the way
/// `render_goal_card`'s `horizontal_wrapped` attempt did (see this
/// function's call site's doc for that bug) — the fix this task exists for.
fn render_compact_goal_grid(ui: &mut egui::Ui, cards: &[(&RepoRow, &switchbard_core::GoalStatus)]) {
    let available = ui.available_width();
    let columns = (((available + COMPACT_CARD_GAP) / (COMPACT_CARD_MIN_WIDTH + COMPACT_CARD_GAP))
        .floor() as usize)
        .clamp(1, COMPACT_CARD_MAX_COLUMNS.min(cards.len().max(1)));
    let card_width =
        (available - COMPACT_CARD_GAP * (columns.saturating_sub(1)) as f32) / columns as f32;

    for row in cards.chunks(columns) {
        ui.horizontal(|ui| {
            for (repo, status) in row {
                ui.allocate_ui(egui::vec2(card_width, COMPACT_CARD_HEIGHT), |ui| {
                    render_compact_goal_card(ui, repo, status, card_width);
                });
                ui.add_space(COMPACT_CARD_GAP);
            }
        });
        ui.add_space(COMPACT_CARD_GAP);
    }
}

const COMPACT_CARD_HEIGHT: f32 = 84.0;

/// One mock §1 glance tile: name, the big `actual / target unit` number, a
/// thin pace-colored meter, and the pace + kind chip pair — read-only. A
/// check-in draft/button belongs on the Goals **place** index
/// (`ui::places::goals`), the mock's own home for it, not here.
fn render_compact_goal_card(
    ui: &mut egui::Ui,
    repo: &RepoRow,
    status: &switchbard_core::GoalStatus,
    width: f32,
) {
    let frame = egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(10, 8));
    frame.show(ui, |ui| {
        ui.set_width(width - 20.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(&status.name).strong());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(status.actual.to_string())
                        .size(18.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("/ {} {}", status.target, status.unit))
                        .small()
                        .color(theme::muted_text()),
                );
            });
            let (kind, _) = goal_pace_kind_label(status.pace);
            theme::painted_meter(
                ui,
                status.progress_fraction(),
                kind.color(),
                egui::vec2(width - 20.0, 5.0),
            );
            ui.horizontal_wrapped(|ui| {
                goal_pace_pill(ui, status.pace);
                // Keyed on `measure`, not the presence of `scope`: an
                // input-attached auto goal (`GoalInputs::projects`/`tasks`
                // rather than the legacy `scope` field) has `measure ==
                // Tasks` but `scope == None`, and still needs to read as
                // automatic, not manual.
                let kind_text = match (status.measure, &status.scope) {
                    (GoalMeasure::Manual, _) => "manual check-ins".to_string(),
                    (GoalMeasure::Tasks, Some(scope)) => format!("auto · counts {scope}"),
                    (GoalMeasure::Tasks, None) => "auto · counts attached tasks".to_string(),
                };
                crate::ui::components::status_pill(ui, StatusKind::Neutral, kind_text, None);
            });
        });
    });
    let _ = repo;
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
                // TASK-97: Digest and Tasks are separate places now (TASK-96
                // split them) — `backlog_view.lens`/`status_filter` alone no
                // longer reach anything on screen from here, since the
                // Tasks place reads its own `tasks_place.filters`, not the
                // legacy single-value facets. Navigate there directly and
                // translate the status into that place's filter-builder
                // predicate model (replacing any existing Status predicate
                // rather than stacking one per click).
                //
                // TASK-97 medic pass (BLOCKER finding): this used to *also*
                // write `backlog_view.status_filter` — a second, invisible
                // narrowing layer `sort::visible_task_rows` still reads that
                // the Tasks place's own filter-chip UI can never show or
                // clear. Removing that write and relying solely on
                // `tasks_place.filters` (the one predicate set with a chip
                // and a "recent:" trail) is the fix; `tasks::
                // neutralize_legacy_filters` is the belt-and-suspenders
                // guard in case some other path still sets it.
                app.place = crate::runtime::Place::Tasks;
                app.tasks_view = crate::runtime::TasksView::All;
                app.backlog_view.lens = BacklogLens::List;
                if let Some(status) = view_all_status_filter {
                    app.tasks_place.filters.retain(|predicate| {
                        predicate.field != crate::ui::places::tasks::fields::TaskField::Status
                    });
                    app.tasks_place.filters.push(
                        crate::ui::places::tasks::state::FilterPredicate {
                            field: crate::ui::places::tasks::fields::TaskField::Status,
                            value: status.to_string(),
                        },
                    );
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
