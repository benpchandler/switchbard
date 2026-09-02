//! The Digest place (TASK-99) — the app's landing surface. Mock §1 ("goal
//! cards lead, then attention") plus §7a's zero-goal empty state; frozen
//! reference `~/.lavish/switchbard-ia-places.html`, decision record in
//! `docs/product-trajectory.md` ("Information architecture V2 — places and
//! objects"). Three sections, in the mock's fixed order:
//!
//! 1. **Goal cards** (`ui::backlog::digest::render_goal_cards_for_digest_place`,
//!    reused rather than forked — see that function's doc) — the week chip
//!    lives in this module's own header, one line up.
//! 2. **In flight** — tasks in the current repo scope that are either
//!    currently dispatching or carry an `In Progress` status. Compact rows;
//!    clicking one deep-links to the Tasks place with the task selected.
//! 3. **Needs a human** — an attention feed computed fresh every frame from
//!    the objects that actually own each fact, never stored on a task. Every
//!    row deep-links to its owning place and its inline action calls the
//!    exact same `HiveApp` verb that place's own UI calls — see each row
//!    builder's doc for which one and why.
//!
//! ## Named gaps (do not fabricate; mission brief §3)
//!
//! Two feed-row types the mock draws have no cheap live evidence behind them
//! in this codebase today, so they are **omitted** rather than invented:
//!
//! - **PR rows** — no PR probe exists anywhere in `switchbard-core`. Nothing
//!   here polls GitHub or reads a cached delivery-state fact, because no such
//!   fact exists yet (see the Task Queue orchestration trajectory entry for
//!   where that's headed).
//! - **Server rows** ("service exited unexpectedly") — the scanner is a
//!   snapshot, not a history: nothing records that a listener that is absent
//!   *now* was present a moment ago, so "exited 4m ago" cannot be told apart
//!   from "was never running here." Inventing an uptime log to answer this
//!   is exactly what the mission brief forbids.
//!
//! Both gaps are recorded on TASK-99's own task notes, not just here.

use crate::app::HiveApp;
use crate::runtime::{self, BacklogTaskKey, Place, TasksView};
use crate::ui::backlog::dispatch_ui::{self, DispatchCategory, DispatchState};
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::path::PathBuf;
use switchbard_core::dispatch_inspect::now_unix;
use switchbard_core::{AttributedListener, BacklogTask, DispatchOptions, Repo, WorktreeRef};

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let read_state = app.tasks_read_state_snapshot();
    let frame = egui::Frame::central_panel(&ui.ctx().style_of(ui.ctx().theme()))
        .inner_margin(egui::Margin::same(12));
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        // No clone just to check emptiness — `backlog_repos_snapshot()`
        // deep-clones every task's full body (description, plan, notes),
        // which the perf smoke (`digest_perf_smoke.rs`) caught costing real
        // per-frame milliseconds even before doing anything with the result.
        if app.backlog_repos.lock().unwrap().is_empty() {
            crate::ui::tasks_read_state::render_empty(app, ui, "Digest", &read_state);
            return;
        }
        // One pass over the backlog cache for both task-derived sections
        // below (see `collect_task_rows`'s own doc for why this matters).
        let (in_flight, run_rows) = collect_task_rows(app);
        egui::ScrollArea::vertical()
            .id_salt("digest_place")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_header(ui);
                crate::ui::tasks_read_state::render_retained_rows_notice(ui, &read_state);
                ui.add_space(8.0);
                crate::ui::backlog::digest::render_goal_cards_for_digest_place(app, ui);
                ui.add_space(14.0);
                render_in_flight(app, ui, &in_flight);
                ui.add_space(14.0);
                render_attention_feed(app, ui, run_rows);
            });
    });
}

/// Mock §1's `htitle` row: "Digest" + the week chip ("Week of `<date>` · day
/// `N` of 7"). `days_elapsed` mirrors `compute_goal_statuses`'s own formula
/// (`(today - monday) + 1`, always in `1..=7` for a `monday` derived from
/// `today` itself — no clamp is reachable here the way it is there, since
/// there is no persisted `week` string that could point at some other week).
/// Computed independently rather than threaded through from a `GoalStatus`
/// because the header renders even with zero goals, when no `GoalStatus`
/// exists to read it from.
fn render_header(ui: &mut egui::Ui) {
    let today = chrono::Local::now().date_naive();
    let monday = switchbard_core::week_monday_of(today);
    let days_elapsed = (today - monday).num_days() + 1;
    ui.horizontal(|ui| {
        ui.heading("Digest");
        ui.label(
            egui::RichText::new(format!(
                "Week of {} · day {days_elapsed} of 7",
                monday.format("%b %-d")
            ))
            .color(theme::muted_text()),
        );
    });
}

// ── in flight ───────────────────────────────────────────────────────────

/// One "In flight" row: a task currently dispatching, or one whose status is
/// `In Progress` — the mock's TASK-83 (dispatching) and TASK-76 (in
/// progress) pairing. A dispatching task always renders with the dispatch
/// pill (`dispatch_ui::render_dispatch_pill`, the exact pill the Tasks
/// place's rows use) regardless of its workflow status; a task is never
/// listed twice for satisfying both conditions.
struct InFlightRow {
    repo_root: PathBuf,
    repo_name: String,
    task_id: String,
    task_title: String,
    dispatching: bool,
}

fn render_in_flight(app: &mut HiveApp, ui: &mut egui::Ui, rows: &[InFlightRow]) {
    ui.label(egui::RichText::new("In flight").strong().heading());
    ui.separator();
    if rows.is_empty() {
        ui.label(egui::RichText::new("Nothing in flight right now.").color(theme::muted_text()));
        return;
    }
    for row in rows {
        render_in_flight_row(app, ui, row);
        ui.add_space(2.0);
    }
}

fn render_in_flight_row(app: &mut HiveApp, ui: &mut egui::Ui, row: &InFlightRow) {
    let frame = egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(10, 6));
    let resp = frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&row.task_id)
                        .monospace()
                        .small()
                        .color(theme::muted_text()),
                );
                ui.label(egui::RichText::new(&row.task_title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let _ = theme::painted_dot(ui, theme::repo_rail_color(&row.repo_name));
                    ui.label(
                        egui::RichText::new(&row.repo_name)
                            .small()
                            .color(theme::muted_text()),
                    );
                    if row.dispatching {
                        dispatch_ui::render_dispatch_pill(ui, &DispatchState::InFlight);
                    } else {
                        status_pill(ui, StatusKind::Info, "in progress", None);
                    }
                });
            });
        })
        .response;
    if resp
        .interact(egui::Sense::click())
        .on_hover_text("Open in Tasks")
        .clicked()
    {
        deep_link_to_task(app, row.repo_root.clone(), row.task_id.clone());
    }
}

fn deep_link_to_task(app: &mut HiveApp, repo_root: PathBuf, task_id: String) {
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    // Widen to "All repos" scope, matching the legacy Digest lens's own
    // deep-link — a Digest row can surface a task from any tracked repo
    // regardless of the current scope, so selecting it needs the rail to
    // actually find it.
    app.backlog_view.selected_repo = None;
    app.backlog_view.selected_task = Some((repo_root, task_id));
    app.backlog_view.editor.loaded_key = None;
}

// ── attention feed ─────────────────────────────────────────────────────

enum RunKind {
    Failed {
        reason: Option<String>,
    },
    /// Claimed but the agent is verifiably gone — no recovery verb exists
    /// anywhere in the app for this (not even in the Dispatches view itself,
    /// which renders the same blurb and no button); Log is the only action.
    Orphaned,
    /// Still in flight but past the advisory staleness threshold — the same
    /// bucket `DispatchSummary`/the Dispatches view's own sectioning call
    /// "needs attention", still killable via the identical confirm-armed
    /// control that view uses.
    Stalled {
        pgid: i32,
        started_at_unix: u64,
    },
}

struct RunFeedRow {
    key: BacklogTaskKey,
    repo_name: String,
    task_title: String,
    elapsed_desc: Option<String>,
    log_path: Option<PathBuf>,
    kind: RunKind,
}

struct PortFeedRow {
    pgid: i32,
    pid: u32,
    port: u16,
    command_name: String,
}

struct WorktreeFeedRow {
    repo_path: PathBuf,
    repo_name: String,
    worktree_path: PathBuf,
    branch: Option<String>,
}

fn repo_name_index(repos: &[Repo]) -> std::collections::HashMap<PathBuf, String> {
    repos
        .iter()
        .map(|r| (r.path.clone(), r.name.clone()))
        .collect()
}

/// The "In flight" section and the run-shaped third of "Needs a human"
/// (failed / orphaned / stalled dispatch runs — the same three buckets
/// `DispatchSummary`, `ui::dispatch`, folds into `needs_attention`), built
/// from **one** pass over the backlog cache.
///
/// Perf (render-path rule, CLAUDE.md): `HiveApp::backlog_repos_snapshot()`
/// deep-clones every task's full body (description, plan, notes) — cheap
/// for one caller, real per-frame cost for two, which is what this file had
/// until `digest_perf_smoke.rs` caught it. Locking `backlog_repos` directly
/// (like `ui::dispatch::summarize_dispatch` already does, for the same
/// reason) and cloning only the handful of `String`s each row actually
/// needs avoids the clone entirely — the same shape `dispatch_runs` and
/// `repos` get below.
///
/// Lock discipline: one mutex at a time, never nested — the same ordering
/// `ui::dispatch::summarize_dispatch` and `workers::refresh_dispatch_runs`
/// hold to. The `backlog_repos` pass below collects owned `PendingRun`s
/// (everything a `RunFeedRow` needs except what only `dispatch_runs` itself
/// can answer) and drops that lock before `dispatch_runs` is ever taken, so
/// the two locks are never held simultaneously.
fn collect_task_rows(app: &HiveApp) -> (Vec<InFlightRow>, Vec<RunFeedRow>) {
    let repo_names = repo_name_index(&app.repos_snapshot());
    let now = now_unix();
    let stale_after = DispatchOptions::default().stale_after;

    let mut in_flight = Vec::new();
    let mut pending_runs = Vec::new();
    {
        let backlog_repos = app.backlog_repos.lock().unwrap();
        for (root, repo) in backlog_repos.iter() {
            if !runtime::path_in_scope(root, &app.repo_scope) {
                continue;
            }
            let repo_name = repo_names
                .get(root)
                .cloned()
                .unwrap_or_else(|| root.display().to_string());
            for task in &repo.tasks {
                let category = dispatch_ui::dispatch_category(task);
                let dispatching = category == DispatchCategory::InFlight;
                let active =
                    task.source != switchbard_core::BacklogTaskSource::Archived && !task.is_done();
                if active && (dispatching || task.status.eq_ignore_ascii_case("in progress")) {
                    in_flight.push(InFlightRow {
                        repo_root: root.clone(),
                        repo_name: repo_name.clone(),
                        task_id: task.id.clone(),
                        task_title: task.title.clone(),
                        dispatching,
                    });
                }

                if !matches!(
                    category,
                    DispatchCategory::Failed | DispatchCategory::InFlight
                ) {
                    continue;
                }
                pending_runs.push(PendingRun {
                    key: (root.clone(), task.id.clone()),
                    repo_name: repo_name.clone(),
                    task_title: task.title.clone(),
                    category,
                    failure_reason: (category == DispatchCategory::Failed)
                        .then(|| failure_reason(task))
                        .flatten(),
                });
            }
        }
    } // `backlog_repos` lock dropped here — never held alongside `dispatch_runs`.

    let mut run_rows = Vec::with_capacity(pending_runs.len());
    let runs = app.dispatch_runs.lock().unwrap();
    for pending in pending_runs {
        let run = runs.get(&pending.key);
        let kind = match pending.category {
            DispatchCategory::Failed => RunKind::Failed {
                reason: pending.failure_reason,
            },
            DispatchCategory::InFlight => {
                let abandoned = run.is_some_and(|r| r.is_abandoned(now, true));
                let stalled = run.is_some_and(|r| r.looks_stalled(now, stale_after));
                if abandoned {
                    RunKind::Orphaned
                } else if stalled {
                    match run.and_then(|r| r.liveness.killable_pgid()) {
                        Some(pgid) => match run.and_then(|r| r.started_at_unix) {
                            Some(started_at_unix) => RunKind::Stalled {
                                pgid,
                                started_at_unix,
                            },
                            None => continue,
                        },
                        // No verified-live pgid to kill: same as the
                        // Dispatches view, which renders no Kill button
                        // in this case either (`render_kill_control`'s
                        // own early return).
                        None => RunKind::Orphaned,
                    }
                } else {
                    continue; // healthy — already in "In flight" above
                }
            }
            _ => unreachable!(),
        };
        run_rows.push(RunFeedRow {
            key: pending.key,
            repo_name: pending.repo_name,
            task_title: pending.task_title,
            elapsed_desc: run.and_then(|r| r.elapsed(now)).map(format_elapsed),
            log_path: run.and_then(|r| r.log_path.clone()),
            kind,
        });
    }
    in_flight.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    (in_flight, run_rows)
}

/// A flagged task's owned data plus the failure reason (computable from the
/// task alone) — everything [`RunFeedRow`] needs except what only a locked
/// `dispatch_runs` lookup can answer (`elapsed_desc`, `log_path`, and the
/// abandoned/stalled branch of `kind`). Collected while `backlog_repos` is
/// locked, then consumed after that lock is dropped and `dispatch_runs` is
/// locked in its place — see `collect_task_rows`'s own doc.
struct PendingRun {
    key: BacklogTaskKey,
    repo_name: String,
    task_title: String,
    category: DispatchCategory,
    failure_reason: Option<String>,
}

fn failure_reason(task: &BacklogTask) -> Option<String> {
    match dispatch_ui::dispatch_state(task) {
        DispatchState::Failed { reason } => reason,
        _ => None,
    }
}

fn format_elapsed(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    match (secs / 3600, (secs % 3600) / 60) {
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// External port squatters — listeners the scanner attributed to no tracked
/// worktree at all. Not filtered by repo scope: an unattributed listener by
/// definition has no owning repo for a scope to narrow against, the same
/// reason `ui::workspace`'s own "Unattributed listeners" card ignores scope
/// today.
fn collect_port_rows(app: &HiveApp) -> Vec<PortFeedRow> {
    let listeners: Vec<AttributedListener> = app.state.lock().unwrap().listeners.clone();
    listeners
        .into_iter()
        .filter(|al| al.worktree_path.is_none())
        .map(|al| PortFeedRow {
            pgid: al.listener.pgid,
            pid: al.listener.pid,
            port: al.listener.port,
            command_name: al.listener.command_name,
        })
        .collect()
}

/// Worktrees `removal_safety` verdicts `Safe` right now — the exact
/// predicate (`runtime::is_retired_worktree`) the top-bar "N retired
/// worktrees" nudge and the Workspace bulk "Select all merged+clean" action
/// already share, so this feed can never disagree with either about which
/// worktrees qualify.
fn collect_worktree_rows(app: &HiveApp) -> Vec<WorktreeFeedRow> {
    let repos = app.repos_snapshot();
    let worktrees: Vec<WorktreeRef> = app.worktrees_snapshot();
    let meta = app.meta.lock().unwrap();
    let mut rows = Vec::new();
    for w in &worktrees {
        let Some(repo) = repos.iter().find(|r| r.name == w.repo_name) else {
            continue;
        };
        if !runtime::repo_in_scope(repo, &app.repo_scope) {
            continue;
        }
        let attached = app.attached_processes(&w.path);
        if runtime::is_retired_worktree(w, &repos, meta.get(&w.path), attached) {
            rows.push(WorktreeFeedRow {
                repo_path: repo.path.clone(),
                repo_name: repo.name.clone(),
                worktree_path: w.path.clone(),
                branch: w.branch.clone(),
            });
        }
    }
    rows
}

fn render_attention_feed(app: &mut HiveApp, ui: &mut egui::Ui, run_rows: Vec<RunFeedRow>) {
    let port_rows = collect_port_rows(app);
    let worktree_rows = collect_worktree_rows(app);
    let total = run_rows.len() + port_rows.len() + worktree_rows.len();

    ui.label(egui::RichText::new("Needs a human").strong().heading());
    ui.separator();
    if total == 0 {
        ui.label(
            egui::RichText::new("Nothing needs a human right now.").color(theme::muted_text()),
        );
        return;
    }
    for row in run_rows {
        render_run_row(app, ui, &row);
        ui.add_space(2.0);
    }
    for row in port_rows {
        render_port_row(app, ui, &row);
        ui.add_space(2.0);
    }
    for row in worktree_rows {
        render_worktree_row(app, ui, &row);
        ui.add_space(2.0);
    }
}

fn feed_row_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(egui::Stroke::NONE)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(10, 6))
}

fn render_run_row(app: &mut HiveApp, ui: &mut egui::Ui, row: &RunFeedRow) {
    feed_row_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            let (kind_label, kind_color) = match &row.kind {
                RunKind::Failed { .. } => ("run · failed", theme::danger()),
                RunKind::Orphaned => ("run · orphaned", theme::amber()),
                RunKind::Stalled { .. } => ("run · stalled", theme::amber()),
            };
            ui.label(
                egui::RichText::new(kind_label)
                    .small()
                    .strong()
                    .color(kind_color),
            );
            let text = feed_text(row);
            let deep_link = ui
                .add(egui::Label::new(text).sense(egui::Sense::click()))
                .on_hover_text("Open in Tasks / Dispatches");
            if deep_link.clicked() {
                app.place = Place::Tasks;
                app.tasks_view = TasksView::Dispatches;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                render_run_actions(app, ui, row);
            });
        });
    });
}

fn feed_text(row: &RunFeedRow) -> String {
    let (_, task_id) = &row.key;
    let detail = match &row.kind {
        RunKind::Failed { reason } => {
            let reason = reason.as_deref().unwrap_or("no reason recorded");
            match &row.elapsed_desc {
                Some(elapsed) => format!("Dispatch failed: {task_id} · {reason} after {elapsed}"),
                None => format!("Dispatch failed: {task_id} · {reason}"),
            }
        }
        RunKind::Orphaned => match &row.elapsed_desc {
            Some(elapsed) => {
                format!("{task_id} claimed but its agent is gone (abandoned after {elapsed})")
            }
            None => format!("{task_id} claimed but its agent is gone"),
        },
        RunKind::Stalled { .. } => match &row.elapsed_desc {
            Some(elapsed) => format!("{task_id} still running {elapsed} - check on it"),
            None => format!("{task_id} still running - check on it"),
        },
    };
    format!("{detail} · {} · {}", row.repo_name, row.task_title)
}

fn render_run_actions(app: &mut HiveApp, ui: &mut egui::Ui, row: &RunFeedRow) {
    // `theme::action_icon_button` — the same painted Watch/Kill/Retry/Log/
    // Respond icon set `ui::places::dispatches`/`ui::places::command` use
    // (TASK-98), consolidated here (TASK-98 rebase onto TASK-99) rather than
    // duplicated: it already sets the AccessKit name and hover tooltip to
    // the exact verb name, and paints instead of relying on literal `↻`/`≡`
    // glyphs, which render as tofu on a stock font install (the same
    // failure this module's own header doc documents for `●▸▾↑↓✕•○`).
    if let RunKind::Failed { .. } = &row.kind {
        if theme::action_icon_button(ui, theme::ActionIcon::Retry, "Retry", true).clicked() {
            let ctx = ui.ctx().clone();
            app.spawn_backlog_dispatch_toggle(row.key.0.clone(), row.key.1.clone(), true, &ctx);
        }
    }
    if let RunKind::Stalled {
        pgid,
        started_at_unix,
    } = &row.kind
    {
        render_stalled_kill(app, ui, row, *pgid, *started_at_unix);
    }
    if let Some(log_path) = &row.log_path {
        if theme::action_icon_button(ui, theme::ActionIcon::Log, "Log", true).clicked() {
            crate::ui::agent_context::open(log_path);
        }
    }
}

/// Kill for a stalled run — the exact same confirm state
/// (`HiveApp::dispatch_kill_confirm`) and the exact same verb
/// (`HiveApp::spawn_kill_dispatch`) the Dispatches view's own
/// `render_kill_control` uses, so a kill armed from Digest and one armed
/// from Dispatches can never disagree about whether this row's confirm step
/// is open.
fn render_stalled_kill(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &RunFeedRow,
    pgid: i32,
    started_at_unix: u64,
) {
    if app.dispatch_kill_confirm.as_ref() == Some(&row.key) {
        ui.colored_label(theme::amber(), format!("Kill pgid {pgid}?"));
        if ui.small_button("Confirm").clicked() {
            let ctx = ui.ctx().clone();
            app.spawn_kill_dispatch(row.key.1.clone(), started_at_unix, &ctx);
            app.dispatch_kill_confirm = None;
        }
        if ui.small_button("Cancel").clicked() {
            app.dispatch_kill_confirm = None;
        }
    } else if theme::action_icon_button(ui, theme::ActionIcon::Kill, "Kill", true).clicked() {
        app.dispatch_kill_confirm = Some(row.key.clone());
    }
}

fn render_port_row(app: &mut HiveApp, ui: &mut egui::Ui, row: &PortFeedRow) {
    feed_row_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("port · squatter")
                    .small()
                    .strong()
                    .color(theme::amber()),
            );
            let deep_link = ui
                .add(
                    egui::Label::new(format!(
                        ":{} squatter not owned by any worktree - {} (pid {})",
                        row.port, row.command_name, row.pid
                    ))
                    .sense(egui::Sense::click()),
                )
                .on_hover_text("Open in Ops");
            if deep_link.clicked() {
                app.place = Place::Ops;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                render_port_kill(app, ui, row);
            });
        });
    });
}

/// Confirm-armed Kill (mission brief §4: "confirm-gated verbs stay
/// confirm-gated") calling the exact same `HiveApp::spawn_kill` the Ops
/// place's own listener row calls — Digest just adds the confirm step Ops's
/// single-listener Kill does not have today, the same way the Dispatches
/// view's Kill already is. `DigestViewState::port_kill_confirm` is keyed by
/// pgid, one at a time, the same shape `dispatch_kill_confirm` uses.
fn render_port_kill(app: &mut HiveApp, ui: &mut egui::Ui, row: &PortFeedRow) {
    if app.digest_view.port_kill_confirm == Some(row.pgid) {
        ui.colored_label(theme::amber(), format!("Kill pid {}?", row.pid));
        if ui.add(theme::danger_button("Confirm")).clicked() {
            let ctx = ui.ctx().clone();
            app.spawn_kill(row.pgid, &ctx);
            app.digest_view.port_kill_confirm = None;
        }
        if ui.small_button("Cancel").clicked() {
            app.digest_view.port_kill_confirm = None;
        }
    } else if theme::action_icon_button(ui, theme::ActionIcon::Kill, "Kill", true).clicked() {
        app.digest_view.port_kill_confirm = Some(row.pgid);
    }
}

fn render_worktree_row(app: &mut HiveApp, ui: &mut egui::Ui, row: &WorktreeFeedRow) {
    feed_row_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("worktree · removable")
                    .small()
                    .strong()
                    .color(theme::green()),
            );
            let branch = row.branch.as_deref().unwrap_or("(detached)");
            let deep_link = ui
                .add(
                    egui::Label::new(format!(
                        "{} · {branch} merged, clean, nothing attached",
                        row.repo_name
                    ))
                    .sense(egui::Sense::click()),
                )
                .on_hover_text("Open in Ops");
            if deep_link.clicked() {
                app.place = Place::Ops;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Same painted trash icon `ui::workspace::mod`'s own Remove
                // worktree row action uses, not `danger_button` with a literal
                // `⌫` (tofu on a stock font install) — one Remove treatment
                // everywhere it appears.
                if theme::icon_button_label(
                    theme::painted_trash_button(ui, theme::weak_text()),
                    "Remove worktree",
                )
                .clicked()
                {
                    app.open_remove_worktree_confirm(
                        row.repo_path.clone(),
                        row.worktree_path.clone(),
                        row.branch.clone(),
                    );
                    app.place = Place::Ops;
                }
            });
        });
    });
}
