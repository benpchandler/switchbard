//! The IA V2 places sidebar (TASK-96) — the app's primary navigation
//! surface, replacing the old top-bar "view:" tab row. Renders, top to
//! bottom: the brand mark, the multi-select repo-scope selector, the
//! FAVORITES group (only when non-empty — nothing here auto-populates), the
//! five places (Digest / Tasks / Command / Goals / Ops) each with a glyph
//! and a count badge, the Tasks place's two built-in subviews (All tasks /
//! Dispatches) when Tasks is active, and a footer ambient dispatch lamp that
//! reuses `ui::dispatch::DispatchSummary` — silent when idle, deep-linking
//! to Tasks / Dispatches otherwise.
//!
//! At a narrow window width (below [`NARROW_WIDTH_THRESHOLD`]) the whole
//! panel collapses to a fixed-width icon rail: five place glyphs with
//! tooltips for names, no scope selector, no favorites, no footer — mock
//! §4/§7d. This is a pure function of `ctx.screen_rect().width()`, checked
//! fresh every frame, so resizing the window live toggles between the two
//! layouts with no persisted "collapsed" flag of its own (unlike the legacy
//! `ui::sidebar` panel's `Config::ui.sidebar_collapsed`, which is a manual
//! user toggle for a different panel entirely).
//!
//! ## Transition routing (TASK-96)
//!
//! This module owns *navigation state* only (`HiveApp::place`/`tasks_view`/
//! `repo_scope`) — it never renders a place's body. `HiveApp::render_ui`
//! reads `self.place`/`self.tasks_view` after this module runs and routes to
//! the existing surface: `Place::Digest` to `ui::backlog::digest::
//! render_digest_place` (the Digest lens's own content, not the whole
//! Backlog view — Digest is its own place now, so it must not also carry the
//! Tasks place's lens-tab chrome); `Place::Tasks` to the whole
//! `ui::backlog::render` (`TasksView::All`, its internal lens tabs staying
//! reachable exactly as before) or `ui::dispatch::render`
//! (`TasksView::Dispatches`); `Place::Command` to `ui::agents::render`;
//! `Place::Goals` to `ui::backlog::digest::render_goals_place` (the Digest
//! lens's "This week's goals" section alone — an explicit interim body per
//! the decision record, not a real Goals index); `Place::Ops` to
//! `ui::workspace::render`. Nothing here licenses deleting any of those
//! surfaces or their internal lens code — five more fireteams build the real
//! place bodies for Command/Goals-index/etc. on top of this shell.

use crate::app::HiveApp;
use crate::runtime::{Place, TasksView};
use crate::ui::dispatch::{self, DispatchSummary};
use crate::ui::theme::{self, Glyph};
use eframe::egui;
use std::path::Path;
use switchbard_core::config::{FavoriteKind, FavoriteRef};
use switchbard_core::{BacklogRepo, BacklogTaskSource, Repo};

/// Below this window width the panel collapses to the icon rail (mock §4:
/// "narrow width: sidebar collapses to the icon rail... below 720px the
/// facet bar wraps").
const NARROW_WIDTH_THRESHOLD: f32 = 720.0;
const EXPANDED_WIDTH: f32 = 184.0;
const RAIL_WIDTH: f32 = 44.0;

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    // `viewport_rect` (not `content_rect`, which subtracts safe-area insets
    // meant for phone notches/dynamic islands — irrelevant to a desktop
    // window's collapse threshold) is egui 0.36's replacement for the
    // removed `Context::screen_rect()`.
    let narrow = ui.input(|i| i.viewport_rect().width()) < NARROW_WIDTH_THRESHOLD;
    if narrow {
        render_rail(app, ui);
    } else {
        render_expanded(app, ui);
    }
}

/// Every place's count badge, computed once per frame from data every
/// surface already snapshots (no I/O of its own). `Place::Digest` and
/// `Place::Command` deliberately carry no count — the mock leaves Digest's
/// badge off entirely, and the binding directive says "Command: leave count
/// off for now".
struct NavCounts {
    tasks: usize,
    goals: usize,
    ops: usize,
}

impl NavCounts {
    fn compute(app: &HiveApp) -> Self {
        let repos = app.repos_snapshot();
        let backlog_repos = app.backlog_repos_snapshot();
        let today = chrono::Local::now().date_naive();
        let week = switchbard_core::week_monday_of(today)
            .format("%Y-%m-%d")
            .to_string();

        let mut tasks = 0usize;
        let mut goals = 0usize;
        for (root, repo) in backlog_repos.iter() {
            if !app.repo_scope.is_empty() && !app.repo_scope.contains(root) {
                continue;
            }
            tasks += active_task_count(repo);
            goals += switchbard_core::compute_goal_statuses(&[repo], &week, today).len();
        }

        let worktrees = app.worktrees_snapshot();
        let ops = worktrees
            .iter()
            .filter(|w| {
                repos
                    .iter()
                    .find(|r| r.name == w.repo_name)
                    .is_some_and(|r| crate::runtime::repo_in_scope(r, &app.repo_scope))
            })
            .count();

        Self { tasks, goals, ops }
    }
}

/// Active = not done, not archived — "the primary work list" count, matching
/// what the Tasks place's default (List, Triage-sorted, Done/Archived
/// hidden) view shows out of the box.
fn active_task_count(repo: &BacklogRepo) -> usize {
    repo.tasks
        .iter()
        .filter(|task| task.source != BacklogTaskSource::Archived && !task.is_done())
        .count()
}

fn render_expanded(app: &mut HiveApp, ui: &mut egui::Ui) {
    let dispatch_summary = dispatch::summarize_dispatch(app);
    let counts = NavCounts::compute(app);
    let frame = egui::Frame::side_top_panel(&ui.ctx().style_of(ui.ctx().theme()))
        .fill(theme::nav_bg())
        .stroke(theme::surface_stroke())
        .inner_margin(egui::Margin::symmetric(8, 10));
    egui::Panel::left("nav")
        .resizable(false)
        .exact_size(EXPANDED_WIDTH)
        .frame(frame)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("SWITCHBARD")
                    .small()
                    .strong()
                    .color(theme::muted_text()),
            );
            ui.add_space(8.0);
            render_scope_selector(app, ui);
            ui.add_space(8.0);
            render_favorites(app, ui);

            place_row(app, ui, Place::Digest, Glyph::Digest, None);
            place_row(app, ui, Place::Tasks, Glyph::Tasks, Some(counts.tasks));
            if app.place == Place::Tasks {
                render_tasks_subviews(app, ui, dispatch_summary);
            }
            place_row(app, ui, Place::Command, Glyph::Command, None);
            place_row(app, ui, Place::Goals, Glyph::Goals, Some(counts.goals));
            place_row(app, ui, Place::Ops, Glyph::Ops, Some(counts.ops));

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                render_footer_lamp(app, ui, dispatch_summary);
            });
        });
}

/// The "N repos" / "All repos" popup — checkboxes, one per tracked repo, any
/// subset. Reads and writes `app.repo_scope` directly; `Config::ui.repo_scope`
/// is kept in lockstep by `HiveApp::save_ui_to_config`'s change-detection
/// (see `app.rs`), not by this function.
fn render_scope_selector(app: &mut HiveApp, ui: &mut egui::Ui) {
    let repos = app.repos_snapshot();
    let restricted_count = app.repo_scope.len();
    let label = if repos.is_empty() || app.repo_scope.is_empty() {
        "All repos".to_string()
    } else {
        format!(
            "{restricted_count} repo{}",
            if restricted_count == 1 { "" } else { "s" }
        )
    };
    ui.menu_button(label, |ui| {
        if repos.is_empty() {
            ui.label(egui::RichText::new("No repos tracked").color(theme::muted_text()));
            return;
        }
        for repo in &repos {
            let mut checked = app.repo_scope.is_empty() || app.repo_scope.contains(&repo.path);
            if ui.checkbox(&mut checked, &repo.name).changed() {
                set_repo_checked(app, &repos, &repo.path, checked);
            }
        }
        ui.separator();
        if ui
            .add_enabled(!app.repo_scope.is_empty(), egui::Button::new("All repos"))
            .clicked()
        {
            app.repo_scope.clear();
        }
    });
}

/// Flip one repo's membership in the scope, materializing the implicit
/// "every repo checked" state (an empty scope) into an explicit full set
/// first — unchecking one repo out of an unrestricted scope must narrow to
/// "every repo but this one", not no-op. Re-collapses back to the canonical
/// empty-set "All repos" representation when the edit leaves every tracked
/// repo checked again, so the selector's label and the persisted shape never
/// drift into "explicitly every repo, coincidentally" — a state that would
/// silently stop tracking newly-added repos.
fn set_repo_checked(app: &mut HiveApp, repos: &[Repo], path: &Path, checked: bool) {
    if app.repo_scope.is_empty() {
        app.repo_scope = repos.iter().map(|r| r.path.clone()).collect();
    }
    if checked {
        app.repo_scope.insert(path.to_path_buf());
    } else {
        app.repo_scope.remove(path);
    }
    if !repos.is_empty()
        && app.repo_scope.len() >= repos.len()
        && repos.iter().all(|r| app.repo_scope.contains(&r.path))
    {
        app.repo_scope.clear();
    }
}

fn render_favorites(app: &mut HiveApp, ui: &mut egui::Ui) {
    if app.config.ui.favorites.is_empty() {
        return;
    }
    ui.label(
        egui::RichText::new("FAVORITES")
            .small()
            .color(theme::muted_text()),
    );
    // Cloned so `navigate_to_favorite` can mutate `app` inside the loop
    // without fighting a live borrow of `app.config.ui.favorites`.
    let favorites = app.config.ui.favorites.clone();
    for fav in &favorites {
        render_favorite_row(app, ui, fav);
    }
    ui.add_space(6.0);
}

fn render_favorite_row(app: &mut HiveApp, ui: &mut egui::Ui, fav: &FavoriteRef) {
    let glyph = match fav.kind {
        FavoriteKind::Project => Glyph::Project,
        FavoriteKind::Task => Glyph::Tasks,
        FavoriteKind::Goal => Glyph::Goals,
        FavoriteKind::View => Glyph::View,
    };
    ui.horizontal(|ui| {
        theme::painted_glyph(ui, glyph, theme::muted_text());
        let resp = ui.add(
            egui::Label::new(&fav.key)
                .truncate()
                .sense(egui::Sense::click()),
        );
        if resp.clicked() {
            app.navigate_to_favorite(fav);
        }
    });
}

/// One place row: glyph + name + optional trailing count badge, the whole
/// row clickable, highlighted (card fill + selection-stroke border) when
/// `place` is the active one. Selecting `Place::Tasks` always lands on
/// `TasksView::All` — see `HiveApp::tasks_view`'s own doc for why only this
/// click (never anything else) resets it.
fn place_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    place: Place,
    glyph: Glyph,
    count: Option<usize>,
) {
    let active = app.place == place;
    let frame = egui::Frame::default()
        .fill(if active {
            theme::card_bg()
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(if active {
            theme::selected_row_stroke()
        } else {
            egui::Stroke::NONE
        })
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(8, 5));
    let inner = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            let glyph_color = if active {
                theme::sky()
            } else {
                theme::muted_text()
            };
            theme::painted_glyph(ui, glyph, glyph_color);
            ui.label(place.label());
            if let Some(n) = count {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(n.to_string())
                            .small()
                            .color(theme::muted_text()),
                    );
                });
            }
        });
    });
    let id = ui.id().with(("nav_place_row", place.label()));
    let resp = ui.interact(inner.response.rect, id, egui::Sense::click());
    if resp.clicked() {
        app.place = place;
        if place == Place::Tasks {
            app.tasks_view = TasksView::All;
        }
    }
}

/// The Tasks place's two built-in views (decision record: "under Tasks live
/// only the built-in views — All tasks, Dispatches"), indented under the
/// Tasks row, shown only while Tasks is the active place.
fn render_tasks_subviews(app: &mut HiveApp, ui: &mut egui::Ui, dispatch_summary: DispatchSummary) {
    ui.indent("nav_tasks_subviews", |ui| {
        subview_row(app, ui, TasksView::All, "All tasks", None);
        let running = dispatch_summary.badge_count();
        let label = if running > 0 {
            format!("Dispatches ({running})")
        } else {
            "Dispatches".to_string()
        };
        let lamp = (running > 0).then(|| {
            if dispatch_summary.needs_attention() {
                theme::danger()
            } else {
                theme::dispatch_accent()
            }
        });
        subview_row(app, ui, TasksView::Dispatches, &label, lamp);
    });
}

fn subview_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    view: TasksView,
    label: &str,
    lamp: Option<egui::Color32>,
) {
    let active = app.place == Place::Tasks && app.tasks_view == view;
    ui.horizontal(|ui| {
        if let Some(color) = lamp {
            theme::painted_dot_small(ui, color);
        }
        let text_color = if active {
            theme::weak_text()
        } else {
            theme::muted_text()
        };
        let resp = ui.add(
            egui::Label::new(egui::RichText::new(label).small().color(text_color))
                .sense(egui::Sense::click()),
        );
        if resp.clicked() {
            app.place = Place::Tasks;
            app.tasks_view = view;
        }
    });
}

/// The ambient dispatch lamp — silent when idle (same "no ticking counters
/// with nothing to say" rule `top_bar`'s own dispatch chip follows), a
/// clickable deep link to Tasks / Dispatches otherwise. Reuses the exact
/// `DispatchSummary` the top bar's chip renders, computed once per frame and
/// passed in by the caller — never recomputed here, so the two ambient
/// indicators can never disagree about whether anything is running.
fn render_footer_lamp(app: &mut HiveApp, ui: &mut egui::Ui, summary: DispatchSummary) {
    if summary.is_idle() {
        return;
    }
    ui.add_space(4.0);
    ui.separator();
    let color = if summary.needs_attention() {
        theme::danger()
    } else {
        theme::dispatch_accent()
    };
    let resp = ui.add(
        egui::Button::new(
            egui::RichText::new(summary.chip_text())
                .small()
                .color(color),
        )
        .frame(false),
    );
    if resp
        .on_hover_text("Headless agent runs in flight — open Tasks / Dispatches")
        .clicked()
    {
        app.place = Place::Tasks;
        app.tasks_view = TasksView::Dispatches;
    }
}

/// The collapsed icon rail (mock §4/§7d): brand initials + five place
/// glyphs, tooltips carrying the names text would otherwise show. No scope
/// selector, no favorites, no footer lamp — narrow width is a "just get me
/// to a place" affordance, not a second copy of the full nav.
fn render_rail(app: &mut HiveApp, ui: &mut egui::Ui) {
    let frame = egui::Frame::side_top_panel(&ui.ctx().style_of(ui.ctx().theme()))
        .fill(theme::nav_bg())
        .stroke(theme::surface_stroke())
        .inner_margin(egui::Margin::symmetric(4, 10));
    egui::Panel::left("nav")
        .resizable(false)
        .exact_size(RAIL_WIDTH)
        .frame(frame)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("SB")
                        .small()
                        .strong()
                        .color(theme::muted_text()),
                );
                ui.add_space(6.0);
                rail_glyph(app, ui, Place::Digest, Glyph::Digest);
                rail_glyph(app, ui, Place::Tasks, Glyph::Tasks);
                rail_glyph(app, ui, Place::Command, Glyph::Command);
                rail_glyph(app, ui, Place::Goals, Glyph::Goals);
                rail_glyph(app, ui, Place::Ops, Glyph::Ops);
            });
        });
}

fn rail_glyph(app: &mut HiveApp, ui: &mut egui::Ui, place: Place, glyph: Glyph) {
    let active = app.place == place;
    let color = if active {
        theme::sky()
    } else {
        theme::muted_text()
    };
    ui.add_space(4.0);
    let frame = egui::Frame::default()
        .fill(if active {
            theme::card_bg()
        } else {
            egui::Color32::TRANSPARENT
        })
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(6));
    let inner = frame.show(ui, |ui| theme::painted_glyph(ui, glyph, color));
    let id = ui.id().with(("nav_rail_glyph", place.label()));
    let resp = ui
        .interact(inner.response.rect, id, egui::Sense::click())
        .on_hover_text(place.label());
    if resp.clicked() {
        app.place = place;
        if place == Place::Tasks {
            app.tasks_view = TasksView::All;
        }
    }
}
