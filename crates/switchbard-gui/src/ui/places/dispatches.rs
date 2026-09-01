//! The Dispatches view (TASK-98): the built-in "Tasks / Dispatches" view —
//! task-scoped dispatch delivery state. One row per dispatch-labeled task,
//! faceted Active/Queued/Finished/Failed, with a live detail card for the
//! selected run.
//!
//! Split out of the old `ui::dispatch` module (TASK-96 decision record: IA
//! V2's two dispatch axes). `ui::dispatch` keeps [`ui::dispatch::
//! DispatchSummary`] — the ambient counts three other surfaces read on
//! every frame regardless of which place is active — this module owns the
//! actual row list, which only needs to exist while this view is on screen.
//!
//! ## Where the data comes from
//!
//! Same rule as before the split: state is derived, never stored. A task's
//! dispatch label *is* its pipeline state
//! (`ui::backlog::dispatch_ui::dispatch_state`), and a run's paths/timestamps
//! come from `switchbard_core::dispatch_inspect`, cached by `workers::
//! refresh_dispatch_runs` so this render path never touches the filesystem
//! for the row list itself.
//!
//! ## Actions are the existing verbs, not new ones
//!
//! Kill signals the run's process group through `HiveApp::spawn_kill_dispatch`
//! — the exact control the old `ui::dispatch::render` had, moved verbatim.
//! Retry re-flags a failed task through `HiveApp::spawn_backlog_dispatch_
//! toggle` — the same label-toggle path the Backlog detail rail's Dispatch
//! button already drives. Watch/Log both open the run's log file via
//! `HiveApp::open_dispatch_path`. No second kill/retry/log implementation.
//!
//! ## The one new filesystem read: the selected run's log tail
//!
//! The detail card for the selected run shows a bounded tail of its log —
//! the one piece of "watch it happen" evidence this view didn't have before.
//! [`read_log_tail`] reads at most [`LOG_TAIL_MAX_BYTES`] from the end of
//! one file, on the render path, but only for the single selected run (never
//! per-row) and only while this view is the active place — bounded exactly
//! the way the mission brief asks, not cached, because a session watching a
//! live run wants it to actually update every frame.

use crate::app::HiveApp;
use crate::runtime::{BacklogTaskKey, DispatchesFacet};
use crate::ui::backlog::dispatch_ui::{self, DispatchState};
use crate::ui::components::{status_pill, table_shell, StatusKind};
use crate::ui::dispatch::format_elapsed;
use crate::ui::theme::{self, ActionIcon};
use eframe::egui;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Duration;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun};
use switchbard_core::{BacklogTask, DispatchOptions};

/// Table row height (mock §2b: `Task | Status | Now doing | Elapsed | []`).
/// Taller than Ops's flat 26px rows because the "Now doing" cell can carry a
/// second, small "unsupervised — ..." line (`render_unsupervised_notice`)
/// under the main now-doing text — every `TableBuilder` row in one `body.
/// rows()` call must share one height, so this is sized for that two-line
/// case rather than the common single-line one.
const ROW_HEIGHT: f32 = 40.0;

/// Bounded log tail for the selected-run detail card — enough to show a few
/// lines of recent output without risking a multi-megabyte read on a chatty
/// run's log.
const LOG_TAIL_MAX_BYTES: u64 = 4_000;

/// One dispatched task, joined to whatever is knowable about its run.
struct DispatchRow {
    /// Repo root, not just the display name: the Kill/Retry controls key
    /// their confirm/task-write state by [`BacklogTaskKey`], and a task id
    /// is only unique within one repo.
    repo_root: PathBuf,
    repo_name: String,
    task: BacklogTask,
    state: DispatchState,
    run: Option<DispatchRun>,
}

impl DispatchRow {
    fn key(&self) -> BacklogTaskKey {
        (self.repo_root.clone(), self.task.id.clone())
    }

    /// Section ordering: things needing attention first. Orphans lead
    /// because they are work already finished that nothing will ever pick
    /// up again; in-flight next because those rows change while you watch;
    /// failures outrank finished work because they are the ones asking for
    /// a decision.
    ///
    /// The `dispatching` label alone does NOT mean in flight — see
    /// `DispatchRun::is_abandoned`'s doc.
    fn section(&self, now: u64) -> Section {
        match self.state {
            DispatchState::InFlight => {
                let abandoned = self
                    .run
                    .as_ref()
                    .is_some_and(|run| run.is_abandoned(now, true));
                if abandoned {
                    Section::Orphaned
                } else {
                    Section::InFlight
                }
            }
            DispatchState::Queued => Section::Queued,
            DispatchState::Failed { .. } => Section::Failed,
            DispatchState::Dispatched { .. } => Section::AwaitingReview,
            DispatchState::NotFlagged => Section::Queued,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Section {
    Orphaned,
    InFlight,
    Queued,
    Failed,
    AwaitingReview,
}

/// Which [`DispatchesFacet`] a row belongs to — mirrors
/// `ui::dispatch::DispatchSummary`'s three-way split (queued / in-flight /
/// needs-attention) plus a fourth bucket for finished (awaiting-review) runs
/// the summary deliberately excludes from all three, so the facet counts and
/// the ambient chip's counts can never silently diverge on what counts as
/// "needs attention".
fn facet_for(row: &DispatchRow, now: u64, stale_after: Duration) -> DispatchesFacet {
    match row.section(now) {
        Section::Queued => DispatchesFacet::Queued,
        Section::AwaitingReview => DispatchesFacet::Finished,
        Section::Failed | Section::Orphaned => DispatchesFacet::Failed,
        Section::InFlight => {
            let stalled = row
                .run
                .as_ref()
                .is_some_and(|run| run.looks_stalled(now, stale_after));
            if stalled {
                DispatchesFacet::Failed
            } else {
                DispatchesFacet::Active
            }
        }
    }
}

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let rows = collect_rows(app);
    let now = now_unix();
    let stale_after = DispatchOptions::default().stale_after;

    egui::CentralPanel::default().show(ui, |ui| {
        if rows.is_empty() {
            render_empty(ui);
            return;
        }

        let counts = facet_counts(&rows, now, stale_after);
        render_facet_bar(app, ui, counts);
        ui.add_space(6.0);

        let facet = app.dispatches_view.facet;
        let visible: Vec<&DispatchRow> = rows
            .iter()
            .filter(|row| facet_for(row, now, stale_after) == facet)
            .collect();

        // A selection pointing at a row the current facet just filtered out
        // stays remembered (switching facets and back restores it) rather
        // than snapping to nothing — but the detail card below only renders
        // for a row actually on screen, so it never shows stale detail for
        // an invisible row.
        if visible.is_empty() {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(format!("No {} runs", facet.label().to_lowercase()))
                    .color(theme::muted_text()),
            );
        } else {
            render_table(ui, app, &visible, now, stale_after);
        }
        render_kill_confirm_banners(app, ui, &visible, now);

        if let Some(selected) = app.dispatches_view.selected.clone() {
            if let Some(row) = rows.iter().find(|r| r.key() == selected) {
                ui.add_space(10.0);
                render_detail_card(ui, row, now);
            }
        }
    });
}

/// Mock §2b's aligned table: `Task | Status | Now doing | Elapsed | []`,
/// following `ui::places::ops::row`'s `TableBuilder` precedent (clip
/// columns, a bounded [`ROW_HEIGHT`], a stroke-ring on the selected row).
/// Row click-to-select wraps only the Task cell's content in its own
/// click-sensing scope (the table's own per-row `response()` is built from
/// each cell's default `Sense::hover`, so a click has to be sensed inside
/// one specific cell rather than read off the row union) — clicking the
/// task id still selects the row exactly as clicking anywhere on the old
/// stacked row did.
fn render_table(
    ui: &mut egui::Ui,
    app: &mut HiveApp,
    rows: &[&DispatchRow],
    now: u64,
    stale_after: Duration,
) {
    egui::ScrollArea::vertical()
        .id_salt("dispatches_table_scroll")
        .max_height(ui.available_height() * 0.6)
        .show(ui, |ui| {
            table_shell(ui, "dispatches_table")
                .column(egui_extras::Column::initial(170.0).at_least(120.0))
                .column(egui_extras::Column::initial(120.0).at_least(100.0))
                .column(egui_extras::Column::remainder().at_least(200.0).clip(true))
                .column(
                    egui_extras::Column::initial(130.0)
                        .at_least(100.0)
                        .clip(true),
                )
                .column(egui_extras::Column::initial(90.0).at_least(70.0))
                .header(22.0, |mut header| {
                    for label in ["Task", "Status", "Now doing", "Elapsed", ""] {
                        header.col(|ui| {
                            ui.label(egui::RichText::new(label).strong().small());
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_HEIGHT, rows.len(), |mut table_row| {
                        let row = rows[table_row.index()];
                        render_dispatch_table_row(&mut table_row, app, row, now, stale_after);
                    });
                });
        });
}

fn render_dispatch_table_row(
    table_row: &mut egui_extras::TableRow<'_, '_>,
    app: &mut HiveApp,
    row: &DispatchRow,
    now: u64,
    stale_after: Duration,
) {
    let key = row.key();
    let selected = app.dispatches_view.selected.as_ref() == Some(&key);
    let orphaned = row.section(now) == Section::Orphaned;
    let in_flight = matches!(row.state, DispatchState::InFlight) && !orphaned;

    // Task cell — id + title, the id's own click-sensing scope selects the
    // row (see this function's call site doc).
    let mut task_clicked = false;
    table_row.col(|ui| {
        ui.style_mut().interaction.selectable_labels = false;
        let resp = ui
            .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&row.task.id).strong().monospace());
                    ui.label(
                        egui::RichText::new(&row.task.title)
                            .small()
                            .color(theme::muted_text()),
                    );
                });
            })
            .response;
        task_clicked = resp.clicked();
    });
    if task_clicked {
        app.dispatches_view.selected = Some(key.clone());
    }

    // Status cell.
    table_row.col(|ui| {
        dispatch_ui::render_dispatch_pill(ui, &row.state);
    });

    // Now doing cell — the live evidence line, plus a second small line:
    // the unsupervised/unverified notice when one applies (mock §2b's
    // `.cap` sub-line under the "now doing" text), else the run's branch.
    table_row.col(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(now_doing_line(&row.state, row.run.as_ref(), &row.task, now))
                    .small()
                    .color(theme::muted_text()),
            );
            if let Some(run) = &row.run {
                if in_flight && !run.liveness.is_supervised() {
                    render_unsupervised_notice(ui, run);
                } else {
                    ui.label(
                        egui::RichText::new(&run.branch)
                            .small()
                            .monospace()
                            .color(theme::muted_text()),
                    );
                }
            }
        });
    });

    // Elapsed cell.
    table_row.col(|ui| {
        render_elapsed_cell(ui, row, now, stale_after);
    });

    // Actions cell.
    table_row.col(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            render_row_actions(app, ui, row, in_flight);
        });
    });

    if selected {
        let resp = table_row.response();
        resp.ctx.debug_painter().rect_stroke(
            resp.rect,
            2.0,
            theme::selected_row_stroke(),
            egui::StrokeKind::Inside,
        );
    }
}

/// Every visible in-flight row's kill-confirm banner (mock's Kill/Confirm/
/// Cancel flow) — a full-width strip below the table rather than inline in
/// a fixed-height table row, since the banner's text only needs to exist
/// while a kill is armed. `render_kill_confirm_banner` itself gates on
/// whether `app.dispatch_kill_confirm` actually names this row, so calling
/// it for every in-flight row here is the same "ask each row, only the
/// armed one answers" shape `render_row_actions`'s Kill icon already uses.
fn render_kill_confirm_banners(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    rows: &[&DispatchRow],
    now: u64,
) {
    for row in rows {
        let orphaned = row.section(now) == Section::Orphaned;
        let in_flight = matches!(row.state, DispatchState::InFlight) && !orphaned;
        if in_flight {
            if let Some(run) = &row.run {
                crate::ui::dispatch::render_kill_confirm_banner(
                    app,
                    ui,
                    &row.repo_root,
                    &row.task.id,
                    run,
                );
            }
        }
    }
}

struct FacetCounts {
    active: usize,
    queued: usize,
    finished: usize,
    failed: usize,
}

impl FacetCounts {
    fn for_facet(&self, facet: DispatchesFacet) -> usize {
        match facet {
            DispatchesFacet::Active => self.active,
            DispatchesFacet::Queued => self.queued,
            DispatchesFacet::Finished => self.finished,
            DispatchesFacet::Failed => self.failed,
        }
    }
}

fn facet_counts(rows: &[DispatchRow], now: u64, stale_after: Duration) -> FacetCounts {
    let mut counts = FacetCounts {
        active: 0,
        queued: 0,
        finished: 0,
        failed: 0,
    };
    for row in rows {
        match facet_for(row, now, stale_after) {
            DispatchesFacet::Active => counts.active += 1,
            DispatchesFacet::Queued => counts.queued += 1,
            DispatchesFacet::Finished => counts.finished += 1,
            DispatchesFacet::Failed => counts.failed += 1,
        }
    }
    counts
}

/// The mock's `.facets` pill row (§2b: "Active · 1 / Queued · 0 / Finished ·
/// 2 / Failed · 1") — same selectable-pill-with-count convention
/// `workspace::staleness::render_filter_bar` already established for the Ops
/// place's staleness chips.
fn render_facet_bar(app: &mut HiveApp, ui: &mut egui::Ui, counts: FacetCounts) {
    ui.horizontal_wrapped(|ui| {
        for facet in DispatchesFacet::ALL {
            let label = format!("{} · {}", facet.label(), counts.for_facet(facet));
            if ui
                .selectable_label(app.dispatches_view.facet == facet, label)
                .clicked()
            {
                app.dispatches_view.facet = facet;
            }
        }
    });
}

/// Join every dispatch-labeled task to its cached run, filtered by the
/// shared top-bar filter (`ui::top_bar::render_filter_controls` already
/// renders the search box for `TasksView::Dispatches` — see that module's
/// doc for why Command needs its own but Dispatches doesn't). Sorted
/// newest-run-first so a long queue keeps the thing that just happened at
/// the top.
fn collect_rows(app: &HiveApp) -> Vec<DispatchRow> {
    let backlog_repos = app.backlog_repos_snapshot();
    let runs = app.dispatch_runs_snapshot();
    let repos = app.repos_snapshot();
    let filter = app.filter().to_lowercase();

    let mut rows: Vec<DispatchRow> = Vec::new();
    for (root, repo) in &backlog_repos {
        if !crate::runtime::path_in_scope(root, &app.repo_scope) {
            continue;
        }
        let repo_name = repos
            .iter()
            .find(|repo| &repo.path == root)
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| root.display().to_string());

        for task in &repo.tasks {
            let state = dispatch_ui::dispatch_state(task);
            if matches!(state, DispatchState::NotFlagged) {
                continue;
            }
            let run = runs
                .get(&(root.clone(), task.id.clone()) as &BacklogTaskKey)
                .cloned();
            let row = DispatchRow {
                repo_root: root.clone(),
                repo_name: repo_name.clone(),
                task: task.clone(),
                state,
                run,
            };
            if row_matches(&row, &filter) {
                rows.push(row);
            }
        }
    }

    rows.sort_by(|a, b| {
        let a_started = a.run.as_ref().and_then(|r| r.started_at_unix).unwrap_or(0);
        let b_started = b.run.as_ref().and_then(|r| r.started_at_unix).unwrap_or(0);
        b_started.cmp(&a_started).then(a.task.id.cmp(&b.task.id))
    });
    rows
}

fn row_matches(row: &DispatchRow, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    let branch = row
        .run
        .as_ref()
        .map(|run| run.branch.as_str())
        .unwrap_or_default();
    [
        row.task.id.as_str(),
        row.task.title.as_str(),
        row.repo_name.as_str(),
        branch,
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(filter_lc))
}

/// "Now doing": the best available *live* evidence, cheapest first — never a
/// new filesystem read. Orchestrator phase (already parsed by the dispatch
/// worker from the events sidecar) beats AC progress (already cached on the
/// task) beats an honest last-activity line built from fields the row
/// already carries.
pub(crate) fn now_doing_line(
    state: &DispatchState,
    run: Option<&DispatchRun>,
    task: &BacklogTask,
    now: u64,
) -> String {
    if let Some(run) = run {
        // Orphaned first, ahead of any sidecar phase: a stale events file
        // from an agent that is already gone would otherwise still claim to
        // be "running cargo test", the exact false reassurance this row's
        // whole job is to avoid.
        if matches!(state, DispatchState::InFlight) && run.is_abandoned(now, true) {
            return "Claimed, but nothing is running - the agent is gone".to_string();
        }
        if let Some(phase) = &run.progress.phase {
            return phase.clone();
        }
        if let Some(outcome) = &run.progress.outcome {
            return format!("orchestrator: {outcome}");
        }
        if !task.acceptance_criteria.is_empty() {
            return format!(
                "{} of {} ACs checked",
                task.acceptance_done_count(),
                task.acceptance_criteria.len()
            );
        }
    }
    match state {
        DispatchState::Queued => "queued - waiting for the dispatch worker's next poll".to_string(),
        DispatchState::InFlight => match run {
            Some(run) if run.log_has_output() => {
                let age = run
                    .log_modified_unix
                    .map(|t| now.saturating_sub(t))
                    .unwrap_or(0);
                format!(
                    "log updated {} ago",
                    format_elapsed(Duration::from_secs(age))
                )
            }
            Some(_) => "no activity recorded yet - this is normal early on".to_string(),
            None => "claimed, waiting on the agent to start".to_string(),
        },
        DispatchState::Dispatched { .. } => "done · PR opened".to_string(),
        DispatchState::Failed { reason } => reason
            .clone()
            .unwrap_or_else(|| "agent exited without a PR".to_string()),
        DispatchState::NotFlagged => String::new(),
    }
}

/// The elapsed-time cell — TASK-46: no code path here promises a hard-kill
/// deadline any more, so the wording is purely descriptive. State changes
/// the *prefix* (running / abandoned after / ran), never just the number,
/// because "5m" alone can't tell a healthy run from a dead one.
fn render_elapsed_cell(ui: &mut egui::Ui, row: &DispatchRow, now: u64, stale_after: Duration) {
    let Some(run) = &row.run else { return };
    let Some(elapsed) = run.elapsed(now) else {
        return;
    };
    let orphaned = row.section(now) == Section::Orphaned;
    let in_flight = matches!(row.state, DispatchState::InFlight) && !orphaned;
    let stalled = in_flight && run.looks_stalled(now, stale_after);
    let label = if in_flight {
        if stalled {
            format!("running {} - check on it", format_elapsed(elapsed))
        } else {
            format!("running {}", format_elapsed(elapsed))
        }
    } else if orphaned {
        format!("abandoned after {}", format_elapsed(elapsed))
    } else {
        format!("ran {}", format_elapsed(elapsed))
    };
    let color = if stalled {
        theme::danger()
    } else {
        theme::muted_text()
    };
    ui.label(egui::RichText::new(label).small().color(color));
}

/// What to say about a run whose supervisor is gone — moved verbatim from
/// the pre-split `ui::dispatch::render_unsupervised_notice` (same two
/// situations, same copy): a verifiably live-but-unsupervised agent needs a
/// human to finish the release step by hand; an unverifiable sidecar means
/// the app genuinely cannot say whether an agent is out there at all.
fn render_unsupervised_notice(ui: &mut egui::Ui, run: &DispatchRun) {
    let text = match run.liveness.doubt() {
        Some(doubt) => format!("unverified - {}", doubt.explain()),
        None if run.liveness.killable_pgid().is_some() => {
            "unsupervised - the app that started this run is gone, so nothing will release \
             the task when it ends; kill it or resolve the task by hand"
                .to_string()
        }
        None => "unsupervised - no record of a live process for this run".to_string(),
    };
    ui.label(egui::RichText::new(text).color(theme::amber()).italics());
}

/// Renders inside a `right_to_left` layout (see `render_dispatch_table_row`'s
/// Actions cell), so every branch below calls its icons in the *reverse* of
/// their intended left-to-right reading order — the first call paints
/// furthest right, per this app's established convention (see `ui::
/// workspace`'s landing-stage row for the same rule spelled out at its own
/// call site).
fn render_row_actions(app: &mut HiveApp, ui: &mut egui::Ui, row: &DispatchRow, in_flight: bool) {
    let has_log = row.run.as_ref().and_then(|r| r.log_path.as_ref()).is_some();

    if in_flight {
        // Reads left-to-right as "Watch, Kill" — or "Watch, Dismiss" when
        // there is no live process to signal: a run the app cannot verify as
        // alive (no killable pgid) can sit here forever once its log has
        // been cleaned out of $TMPDIR and no sidecar was written, because
        // `is_abandoned` has no evidence left to adjudicate with
        // (owner-reported, TASK-307). Kill and Dismiss are mutually
        // exclusive on purpose: while a pgid is verifiably alive, the only
        // honest offer is to kill it, and once nothing is verifiably
        // running, the only honest offer is to discard the record.
        if let Some(run) = &row.run {
            if run.liveness.killable_pgid().is_some() {
                crate::ui::dispatch::render_kill_icon(app, ui, &row.repo_root, &row.task.id, run);
            } else if theme::action_icon_button(ui, ActionIcon::Dismiss, "Dismiss", true).clicked()
            {
                app.spawn_dispatch_dismiss(row.repo_root.clone(), row.task.id.clone(), ui.ctx());
            }
        }
        if theme::action_icon_button(ui, ActionIcon::Watch, "Watch", has_log).clicked() {
            if let Some(path) = row.run.as_ref().and_then(|r| r.log_path.clone()) {
                app.open_dispatch_path(&path);
            }
        }
    } else if matches!(row.state, DispatchState::InFlight) {
        // Still labelled `dispatching` but not `in_flight` — the Orphaned
        // section: positive proof the run died with nothing releasing the
        // claim. Reads left-to-right as "Log, Dismiss".
        if theme::action_icon_button(ui, ActionIcon::Dismiss, "Dismiss", true).clicked() {
            app.spawn_dispatch_dismiss(row.repo_root.clone(), row.task.id.clone(), ui.ctx());
        }
        if theme::action_icon_button(ui, ActionIcon::Log, "Log", has_log).clicked() {
            if let Some(path) = row.run.as_ref().and_then(|r| r.log_path.clone()) {
                app.open_dispatch_path(&path);
            }
        }
    } else if matches!(row.state, DispatchState::Failed { .. }) {
        // Reads left-to-right as "Retry, Log, Dismiss".
        if theme::action_icon_button(ui, ActionIcon::Dismiss, "Dismiss", true).clicked() {
            app.spawn_dispatch_dismiss(row.repo_root.clone(), row.task.id.clone(), ui.ctx());
        }
        if theme::action_icon_button(ui, ActionIcon::Log, "Log", has_log).clicked() {
            if let Some(path) = row.run.as_ref().and_then(|r| r.log_path.clone()) {
                app.open_dispatch_path(&path);
            }
        }
        if theme::action_icon_button(ui, ActionIcon::Retry, "Retry", true).clicked() {
            app.spawn_backlog_dispatch_toggle(
                row.repo_root.clone(),
                row.task.id.clone(),
                true,
                ui.ctx(),
            );
        }
    } else if has_log && theme::action_icon_button(ui, ActionIcon::Log, "Log", true).clicked() {
        if let Some(path) = row.run.as_ref().and_then(|r| r.log_path.clone()) {
            app.open_dispatch_path(&path);
        }
    }
}

fn render_empty(ui: &mut egui::Ui) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("Nothing dispatched yet").strong());
        ui.label(
            egui::RichText::new(
                "Flag a task for dispatch from the Backlog view and it shows up here.",
            )
            .color(theme::muted_text()),
        );
    });
}

/// The selected run's detail card (mock §2b): a bounded log tail, AC chips
/// from the task card, and SITREP age (= log mtime age — the same evidence
/// `ui::places::command`'s fleet rows use for the same field, so the two
/// surfaces can never disagree about how stale a run's last activity is).
fn render_detail_card(ui: &mut egui::Ui, row: &DispatchRow, now: u64) {
    egui::Frame::default()
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&row.task.id).strong());
                ui.label(egui::RichText::new(&row.task.title).color(theme::muted_text()));
            });

            match row.run.as_ref().and_then(|r| r.log_path.as_ref()) {
                Some(log_path) => match read_log_tail(log_path, LOG_TAIL_MAX_BYTES) {
                    Some(tail) if !tail.trim().is_empty() => {
                        egui::ScrollArea::vertical()
                            .max_height(140.0)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(tail)
                                        .monospace()
                                        .small()
                                        .color(theme::muted_text()),
                                );
                            });
                    }
                    _ => {
                        ui.label(
                            egui::RichText::new("log is empty - nothing written yet")
                                .italics()
                                .color(theme::muted_text()),
                        );
                    }
                },
                None => {
                    ui.label(
                        egui::RichText::new("no log yet")
                            .italics()
                            .color(theme::muted_text()),
                    );
                }
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if row.task.acceptance_criteria.is_empty() {
                    ui.label(
                        egui::RichText::new("no acceptance criteria").color(theme::muted_text()),
                    );
                } else {
                    for item in &row.task.acceptance_criteria {
                        // Checked/unchecked is color-only, not a Unicode
                        // checkmark glyph — this app's fonts are Barlow/
                        // JetBrains plus egui's bundled fallbacks, which
                        // this module doc's own font-coverage note (`ui::
                        // theme`'s `Glyph` doc) says can't be trusted with
                        // an unverified symbol like ✓; better an honest
                        // color-only chip than a tofu box next to a real
                        // task's AC list. Mock §2b paints these as chips
                        // (`.chip.green`/bare `.chip`), not plain text.
                        let text = format!("AC {}", item.index);
                        let kind = if item.checked {
                            StatusKind::Good
                        } else {
                            StatusKind::Neutral
                        };
                        status_pill(ui, kind, text, None);
                    }
                }
                if let Some(run) = &row.run {
                    let sitrep = match run.log_modified_unix {
                        Some(mtime) => {
                            format!(
                                "SITREP {} ago",
                                format_elapsed(Duration::from_secs(now.saturating_sub(mtime)))
                            )
                        }
                        None => "SITREP: no activity yet".to_string(),
                    };
                    // Mock §2b's `.chip.amber` SITREP chip.
                    status_pill(ui, StatusKind::Warn, sitrep, None);
                }
            });
        });
}

/// Read the last `max_bytes` of a file as UTF-8, lossily. Bounded, single
/// read per frame for the one selected row's card — see this module's doc
/// for why that's an accepted render-path cost rather than something to
/// cache.
fn read_log_tail(path: &std::path::Path, max_bytes: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    // Bound to the last handful of lines even within the byte budget — a
    // long single line (a stack trace, a JSON blob) shouldn't fill the card.
    let lines: Vec<&str> = text.lines().collect();
    let tail_lines = &lines[lines.len().saturating_sub(20)..];
    Some(tail_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::dispatch_inspect::{DispatchRunLiveness, RunProgress};
    use switchbard_core::{BacklogChecklistItem, BacklogTaskSource};

    const NOW: u64 = 1_000_000;
    const STALE_AFTER: Duration = Duration::from_secs(30 * 60);

    fn task_with_acs(done: usize, total: usize) -> BacklogTask {
        BacklogTask {
            id: "TASK-1".to_string(),
            title: "Example".to_string(),
            status: "In Progress".to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: (0..total)
                .map(|i| BacklogChecklistItem {
                    index: i + 1,
                    checked: i < done,
                    text: format!("AC {}", i + 1),
                })
                .collect(),
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: std::path::PathBuf::from("/repo/backlog/tasks/task-1.md"),
        }
    }

    fn run(overrides: impl FnOnce(&mut DispatchRun)) -> DispatchRun {
        let mut run = DispatchRun {
            task_id: "TASK-1".to_string(),
            branch: "dispatch/task-1".to_string(),
            worktree_path: std::path::PathBuf::from("/repo/.worktrees/dispatch-task-1"),
            worktree_exists: true,
            log_path: None,
            prompt_path: None,
            started_at_unix: Some(NOW - 300),
            log_bytes: 0,
            log_modified_unix: None,
            liveness: DispatchRunLiveness::NoSidecar,
            progress: RunProgress::default(),
        };
        overrides(&mut run);
        run
    }

    fn row(state: DispatchState, run: Option<DispatchRun>, task: BacklogTask) -> DispatchRow {
        DispatchRow {
            repo_root: std::path::PathBuf::from("/repo"),
            repo_name: "repo".to_string(),
            task,
            state,
            run,
        }
    }

    #[test]
    fn a_stalled_in_flight_run_facets_as_failed_not_active() {
        let stalled_run = run(|r| r.started_at_unix = Some(NOW - STALE_AFTER.as_secs() - 60));
        let r = row(
            DispatchState::InFlight,
            Some(stalled_run),
            task_with_acs(0, 0),
        );

        assert_eq!(facet_for(&r, NOW, STALE_AFTER), DispatchesFacet::Failed);
    }

    #[test]
    fn a_healthy_in_flight_run_facets_as_active() {
        let healthy = run(|r| r.started_at_unix = Some(NOW - 60));
        let r = row(DispatchState::InFlight, Some(healthy), task_with_acs(0, 0));

        assert_eq!(facet_for(&r, NOW, STALE_AFTER), DispatchesFacet::Active);
    }

    #[test]
    fn an_orphaned_run_facets_as_failed() {
        let orphan = run(|r| {
            r.started_at_unix = Some(NOW - 3_000);
            r.log_bytes = 900;
            r.log_modified_unix = Some(NOW - 600);
        });
        assert!(
            orphan.looks_orphaned(NOW, true),
            "fixture must be an orphan"
        );
        let r = row(DispatchState::InFlight, Some(orphan), task_with_acs(0, 0));

        assert_eq!(facet_for(&r, NOW, STALE_AFTER), DispatchesFacet::Failed);
    }

    #[test]
    fn queued_finished_and_failed_map_to_their_own_facets() {
        assert_eq!(
            facet_for(
                &row(DispatchState::Queued, None, task_with_acs(0, 0)),
                NOW,
                STALE_AFTER
            ),
            DispatchesFacet::Queued
        );
        assert_eq!(
            facet_for(
                &row(
                    DispatchState::Dispatched { pr_url: None },
                    Some(run(|_| {})),
                    task_with_acs(0, 0)
                ),
                NOW,
                STALE_AFTER
            ),
            DispatchesFacet::Finished
        );
        assert_eq!(
            facet_for(
                &row(
                    DispatchState::Failed { reason: None },
                    None,
                    task_with_acs(0, 0)
                ),
                NOW,
                STALE_AFTER
            ),
            DispatchesFacet::Failed
        );
    }

    /// AC progress is cheap-to-derive "now doing" evidence when the
    /// orchestrator sidecar has nothing to say — this is the fallback the
    /// mission brief calls out by name.
    #[test]
    fn now_doing_prefers_ac_progress_over_a_generic_running_line() {
        let task = task_with_acs(2, 4);
        let in_flight = run(|r| r.log_bytes = 0);

        let line = now_doing_line(&DispatchState::InFlight, Some(&in_flight), &task, NOW);

        assert_eq!(line, "2 of 4 ACs checked");
    }

    /// The orchestrator's own phase always outranks AC progress — it is more
    /// specific live evidence when it exists.
    #[test]
    fn now_doing_prefers_the_orchestrator_phase_over_ac_progress() {
        let task = task_with_acs(2, 4);
        let in_flight = run(|r| r.progress.phase = Some("running cargo test".to_string()));

        let line = now_doing_line(&DispatchState::InFlight, Some(&in_flight), &task, NOW);

        assert_eq!(line, "running cargo test");
    }

    /// With no sidecar and no acceptance criteria at all, the fallback is an
    /// honest last-activity line built from fields already on the row —
    /// never a fabricated "working on it".
    #[test]
    fn now_doing_falls_back_to_an_honest_last_activity_line() {
        let task = task_with_acs(0, 0);
        let in_flight = run(|r| r.log_bytes = 0);

        let line = now_doing_line(&DispatchState::InFlight, Some(&in_flight), &task, NOW);

        assert_eq!(line, "no activity recorded yet - this is normal early on");
    }
}
