//! Ops — the operational place (TASK-100, IA V2 decision record). Merges the
//! former Servers + Workspace surfaces into ONE ROW PER WORKTREE, matching
//! the frozen mock's §6 table: `Worktree / Git / Services / Listening /
//! Agent / actions`. The primary worktree's row carries the repo name bold
//! plus its branch; every other worktree of that repo indents beneath it
//! ("↳ branch"). External listeners attributed to no worktree get their own
//! rows at the bottom, with the confirm-armed Kill verb.
//!
//! This module *replaces* the old swimlane-cards layout
//! (`ui::workspace`, retired by this task — see the git history for its
//! last shape) with a flat, virtualized `egui_extras::TableBuilder` — every
//! worktree's full state fits on one line now, so there is no more
//! progressive-disclosure "expand the noteworthy ones" behavior to carry
//! forward; every row always shows everything.
//!
//! Every verb is the *same* verb the old view dispatched — `spawn_start`,
//! `spawn_stop_run`, `spawn_kill`, `open_in_browser`, `open_remove_worktree_
//! confirm` / bulk remove, create/rename worktree — reached through the same
//! `Pending`-then-`apply_pending` queue so a click never double-borrows
//! `HiveApp`. Only the *rendering* is new.
//!
//! The "Tracked repos" panel does not render here (owner UX pass decision,
//! recorded in this task's brief): repo add/remove is reachable from
//! Settings only, which already offered it (`ui::settings`).

use crate::app::HiveApp;
use crate::runtime::{
    dispatch_run_holds_worktree, ActiveRun, ConfirmRemoveWorktree, LandingEntry, RowState,
    WorktreeMeta, WorktreeSizeEntry,
};
use crate::ui::theme;
use eframe::egui;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use switchbard_core::{
    resolve, AttachedProcesses, AttributedListener, CheckOutcome, DetectedService, RemovalCheck,
    RemovalIntent, RemovalSafety, Repo, ResolvedService, ServiceSource, WorktreeRef,
};

mod agent;
mod bulk_remove;
mod chips;
pub mod create_worktree;
mod git_chip;
pub mod landing;
pub mod rename_worktree;
mod row;
pub mod staleness;
pub mod tooltips;

use staleness::StalenessFilter;

/// Actions queued during the table walk; applied after the table closure
/// exits so we don't double-borrow `app`. Identical shape to the retired
/// `ui::workspace::Pending` — same verbs, same reasoning.
#[derive(Default)]
pub(super) struct Pending {
    start: Option<(PathBuf, DetectedService)>,
    stop: Option<(i32, String)>,
    open: Option<u16>,
    kill: Option<i32>,
    open_create_worktree: Option<Repo>,
    open_rename_worktree: Option<(Repo, WorktreeRef)>,
    /// (repo_name, worktree_path, branch) — `apply_pending` resolves repo_name
    /// to a path via `app.config.repos` and opens the confirm dialog.
    open_remove_worktree: Option<(String, PathBuf, Option<String>)>,
}

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    let snap = Snapshot::collect(app);
    let mut pending = Pending::default();

    egui::CentralPanel::default().show(ui, |ui| {
        render_summary(ui, &snap);
        ui.add_space(4.0);
        staleness::render_filter_bar(ui, app, &snap);
        ui.add_space(6.0);
        row::render_table(ui, app, &snap, &mut pending);
    });

    apply_pending(app, ui, pending);
    render_kill_all_modal(app, ui);
    render_remove_worktree_modal(app, ui);
    bulk_remove::render_modal(app, ctx);
    create_worktree::render_modal(app, ctx);
    rename_worktree::render_modal(app, ctx);
}

fn apply_pending(app: &mut HiveApp, ui: &mut egui::Ui, p: Pending) {
    let ctx = &ui.ctx().clone();
    if let Some((path, svc)) = p.start {
        app.spawn_start(path, svc, ctx);
    }
    if let Some((pgid, name)) = p.stop {
        app.spawn_stop_run(pgid, name, ctx);
    }
    if let Some(port) = p.open {
        app.open_in_browser(port);
    }
    if let Some(pgid) = p.kill {
        app.spawn_kill(pgid, ctx);
    }
    if let Some(repo) = p.open_create_worktree {
        app.open_create_worktree(repo);
    }
    if let Some((repo, worktree)) = p.open_rename_worktree {
        app.open_rename_worktree(repo, worktree);
    }
    if let Some((repo_name, wt_path, branch)) = p.open_remove_worktree {
        if let Some(repo_path) = app
            .config
            .repos
            .iter()
            .find(|r| r.name == repo_name)
            .map(|r| r.path.clone())
        {
            app.open_remove_worktree_confirm(repo_path, wt_path, branch);
        }
    }
}

// ── snapshot ──────────────────────────────────────────────────────────────

#[derive(Default)]
pub(super) struct Snapshot {
    pub(super) repos: Vec<Repo>,
    pub(super) worktrees: Vec<WorktreeRef>,
    pub(super) meta: HashMap<PathBuf, WorktreeMeta>,
    pub(super) sizes: HashMap<PathBuf, WorktreeSizeEntry>,
    pub(super) landing: Arc<Mutex<HashMap<PathBuf, LandingEntry>>>,
    pub(super) services: HashMap<PathBuf, Vec<ResolvedService>>,
    pub(super) listeners_by_wt: HashMap<PathBuf, Vec<AttributedListener>>,
    pub(super) unattributed: Vec<AttributedListener>,
    pub(super) active_runs: HashMap<i32, ActiveRun>,
    pub(super) dispatch_holds_by_wt: HashMap<PathBuf, usize>,
    /// TASK-100 Agent cell: the dispatch run(s) currently holding each
    /// worktree, keyed the same way as `dispatch_holds_by_wt` but carrying
    /// enough to render "claude · active 1h" — see `agent`'s module doc for
    /// the seam this leaves for TASK-98's interactive sessions.
    pub(super) agent_attribution_by_wt: HashMap<PathBuf, agent::DispatchAttribution>,
    pub(super) by_port: HashMap<u16, AttributedListener>,
    pub(super) ports_by_pgid: HashMap<i32, Vec<u16>>,
    pub(super) filter_lc: String,
    pub(super) show_only_managed: bool,
    pub(super) raw_detected_total: usize,
    pub(super) staleness_filter: StalenessFilter,
    /// The flat, pre-filtered row list `row::render_table` virtualizes over —
    /// computed once per frame here (not lazily inside the table closure) so
    /// `TableBody::rows`'s `total_rows` count and its per-index lookup agree
    /// with each other by construction.
    pub(super) rows: Vec<OpsRow>,
}

/// One line of the merged table: a worktree (primary or linked) or an
/// external squatter with no worktree attribution.
pub(super) enum OpsRow {
    Worktree {
        worktree_idx: usize,
        is_primary: bool,
    },
    Squatter {
        listener_idx: usize,
    },
}

impl Snapshot {
    fn collect(app: &HiveApp) -> Self {
        let raw: HashMap<PathBuf, Vec<DetectedService>> = app.services.lock().unwrap().clone();
        let raw_detected_total: usize = raw.values().map(|v| v.len()).sum();
        let services: HashMap<PathBuf, Vec<ResolvedService>> =
            raw.into_iter().map(|(p, d)| (p, resolve(d))).collect();
        let meta = app.meta.lock().unwrap().clone();
        let active_runs = app.active_runs.lock().unwrap().clone();

        let attributed: Vec<AttributedListener> = app.state.lock().unwrap().listeners.clone();
        let mut listeners_by_wt: HashMap<PathBuf, Vec<AttributedListener>> = HashMap::new();
        let mut unattributed: Vec<AttributedListener> = Vec::new();
        let mut by_port: HashMap<u16, AttributedListener> = HashMap::new();
        let mut ports_by_pgid: HashMap<i32, Vec<u16>> = HashMap::new();
        for al in attributed {
            by_port
                .entry(al.listener.port)
                .or_insert_with(|| al.clone());
            ports_by_pgid
                .entry(al.listener.pgid)
                .or_default()
                .push(al.listener.port);
            match &al.worktree_path {
                Some(p) => listeners_by_wt.entry(p.clone()).or_default().push(al),
                None => unattributed.push(al),
            }
        }
        for v in ports_by_pgid.values_mut() {
            v.sort();
            v.dedup();
        }

        let dispatch_runs = app.dispatch_runs.lock().unwrap();
        let mut dispatch_holds_by_wt: HashMap<PathBuf, usize> = HashMap::new();
        let mut agent_attribution_by_wt: HashMap<PathBuf, agent::DispatchAttribution> =
            HashMap::new();
        for run in dispatch_runs.values() {
            if dispatch_run_holds_worktree(&run.liveness) {
                *dispatch_holds_by_wt
                    .entry(run.worktree_path.clone())
                    .or_default() += 1;
                agent::accumulate(&mut agent_attribution_by_wt, run);
            }
        }
        drop(dispatch_runs);

        let repos: Vec<Repo> = app
            .repos_snapshot()
            .into_iter()
            .filter(|repo| crate::runtime::repo_in_scope(repo, &app.repo_scope))
            .collect();
        let worktrees = app.worktrees_snapshot();
        let filter_lc = app.filter().to_lowercase();
        let show_only_managed = app.show_only_managed;
        let staleness_filter = app.staleness_filter;

        let mut snap = Self {
            repos,
            worktrees,
            meta,
            sizes: app.sizes.lock().unwrap().clone(),
            landing: app.landing.clone(),
            services,
            listeners_by_wt,
            unattributed,
            active_runs,
            dispatch_holds_by_wt,
            agent_attribution_by_wt,
            by_port,
            ports_by_pgid,
            filter_lc,
            show_only_managed,
            raw_detected_total,
            staleness_filter,
            rows: Vec::new(),
        };
        snap.rows = snap.compute_rows();
        snap
    }

    /// Repo-then-worktree order (primary first, exactly the order `git
    /// worktree list` returns and `worktrees_snapshot()` preserves), filtered
    /// by the free-text filter + staleness facet; unattributed listeners
    /// (external squatters) always sort last, unfiltered by either — the
    /// mock treats them as a distinct bottom section, not a filterable row
    /// class.
    fn compute_rows(&self) -> Vec<OpsRow> {
        let mut rows = Vec::new();
        for repo in &self.repos {
            for (idx, w) in self.worktrees.iter().enumerate() {
                if w.repo_name != repo.name {
                    continue;
                }
                if !worktree_visible(w, self) {
                    continue;
                }
                rows.push(OpsRow::Worktree {
                    worktree_idx: idx,
                    is_primary: w.path == repo.path,
                });
            }
        }
        if !self.show_only_managed {
            for idx in 0..self.unattributed.len() {
                rows.push(OpsRow::Squatter { listener_idx: idx });
            }
        }
        rows
    }

    fn run_for_resolved(&self, wt_path: &Path, resolved: &ResolvedService) -> Option<&ActiveRun> {
        for ep in &resolved.entry_points {
            if let Some(run) = self.run_for(wt_path, &ep.name) {
                return Some(run);
            }
        }
        None
    }

    fn run_for(&self, wt_path: &Path, service_name: &str) -> Option<&ActiveRun> {
        self.active_runs
            .values()
            .find(|r| r.worktree_path == wt_path && r.service_name == service_name)
    }
}

pub(super) fn is_containerized(resolved: &ResolvedService) -> bool {
    resolved
        .entry_points
        .iter()
        .any(|ep| ep.source == ServiceSource::DockerCompose)
}

/// Does this worktree hold work the trunk doesn't? Content, not ancestry —
/// see the retired swimlane view's identical helper for the rebase-merged
/// reasoning this preserves verbatim. Feeds both the Git cell's trunk chip
/// and the Worktree cell's landing-stage chip (`row::render_worktree_cell`).
pub(super) fn has_unlanded_work(trunk: &Option<switchbard_core::TrunkDivergence>) -> bool {
    trunk.as_ref().is_some_and(|t| t.unlanded > 0)
}

// ── header summary ───────────────────────────────────────────────────────

fn render_summary(ui: &mut egui::Ui, snap: &Snapshot) {
    let services_total: usize = snap.services.values().map(|v| v.len()).sum();
    let listeners_total: usize = snap
        .listeners_by_wt
        .values()
        .map(|v| v.len())
        .sum::<usize>()
        + snap.unattributed.len();
    let running = snap.active_runs.len();
    let mut external = 0usize;
    for (wt_path, list) in &snap.services {
        for resolved in list {
            let Some(port) = resolved.expected_port else {
                continue;
            };
            let run = snap.run_for_resolved(wt_path, resolved);
            let c = is_containerized(resolved);
            if matches!(
                RowState::compute(Some(port), wt_path, run, &snap.by_port, c),
                RowState::ExternalLive { .. }
            ) {
                external += 1;
            }
        }
    }
    let mut s = format!(
        "{} repos · {} worktrees · {} services ({} raw entries) · {} listeners",
        snap.repos.len(),
        snap.worktrees.len(),
        services_total,
        snap.raw_detected_total,
        listeners_total,
    );
    if running > 0 {
        s.push_str(&format!(" · {running} running"));
    }
    if external > 0 {
        s.push_str(&format!(" · {external} external"));
    }
    ui.label(egui::RichText::new(s).color(theme::weak_text()));
}

/// Everything holding on to this worktree, from the three independent places
/// that know: the port scanner, this instance's started services, and the
/// dispatch run table. `pub(super)` — `staleness::merged_and_clean_paths`
/// calls this exactly as it did when it lived in the retired
/// `ui::workspace` module.
pub(super) fn attached_processes(
    snap: &Snapshot,
    wt_path: &Path,
    listener_count: usize,
) -> AttachedProcesses {
    AttachedProcesses {
        listeners: listener_count,
        switchbard_runs: snap
            .active_runs
            .values()
            .filter(|run| run.worktree_path == wt_path)
            .count(),
        dispatch_runs: snap.dispatch_holds_by_wt.get(wt_path).copied().unwrap_or(0),
    }
}

// ── filter (worktree-level) ─────────────────────────────────────────────

/// A worktree row renders iff it passes BOTH the freeform text filter and
/// the staleness filter chip (TASK-41) — the two are independent, ANDed
/// conditions, same as before this task's rewrite.
fn worktree_visible(w: &WorktreeRef, snap: &Snapshot) -> bool {
    worktree_matches(w, snap, &snap.filter_lc)
        && staleness::passes_staleness_filter(snap.staleness_filter, snap.meta.get(&w.path))
}

fn worktree_matches(w: &WorktreeRef, snap: &Snapshot, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    if w.repo_name.to_lowercase().contains(filter_lc)
        || w.branch
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(filter_lc)
        || w.path.to_string_lossy().to_lowercase().contains(filter_lc)
    {
        return true;
    }
    if let Some(svcs) = snap.services.get(&w.path) {
        if svcs
            .iter()
            .any(|s| chips::service_matches_filter(s, w, filter_lc))
        {
            return true;
        }
    }
    if let Some(list) = snap.listeners_by_wt.get(&w.path) {
        if list.iter().any(|l| chips::listener_matches(l, filter_lc)) {
            return true;
        }
    }
    false
}

// ── kill-all confirm modal + accessor for top bar ───────────────────────

pub fn unique_pgids_in_filter(app: &HiveApp) -> Vec<i32> {
    let filter_lc = app.filter().to_lowercase();
    let show_only_managed = app.show_only_managed;
    let listeners = app.state.lock().unwrap().listeners.clone();
    let mut set: BTreeSet<i32> = BTreeSet::new();
    for listener in &listeners {
        if show_only_managed && listener.worktree_path.is_none() {
            continue;
        }
        if chips::listener_matches(listener, &filter_lc) {
            set.insert(listener.listener.pgid);
        }
    }
    set.into_iter().collect()
}

/// Confirmation dialog for `git worktree remove`. Reads state from the
/// `Arc<Mutex<>>` once per frame; the worker thread driving the actual
/// removal can flip `busy`/`error` between frames so the dialog stays
/// responsive. Unchanged from the retired `ui::workspace` module.
fn render_remove_worktree_modal(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    let state = match app.confirm_remove_worktree.lock().unwrap().clone() {
        Some(s) => s,
        None => return,
    };

    let has_runs = !state.active_runs.is_empty();
    let is_dirty = !state.dirty_files.is_empty();
    let action_label = match (has_runs, is_dirty) {
        (false, false) => "Remove worktree",
        (true, false) => "Stop services and remove",
        (false, true) => "Discard changes and remove",
        (true, true) => "Stop services, discard changes, and remove",
    };

    let mut do_confirm = false;
    let mut do_cancel = false;
    let mut delete_branch = state.delete_branch;

    egui::Window::new("Remove worktree")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(540.0);
            ui.label(
                egui::RichText::new(format!(
                    "Remove worktree at {} ?",
                    state.worktree_path.display()
                ))
                .strong(),
            );
            render_shared_checks(ui, &state);
            render_branch_delete_section(ui, &state, &mut delete_branch);

            if has_runs {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "⚠ {} service{} running here (started by switchbard):",
                        state.active_runs.len(),
                        if state.active_runs.len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ))
                    .color(theme::amber()),
                );
                for run in &state.active_runs {
                    ui.label(format!("    {}    (pgid {})", run.service_name, run.pgid));
                }
            }

            if is_dirty {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "⚠ {} uncommitted change{}:",
                        state.dirty_files.len(),
                        if state.dirty_files.len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ))
                    .color(theme::amber()),
                );
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .id_salt("remove_wt_dirty")
                    .show(ui, |ui| {
                        for f in &state.dirty_files {
                            ui.monospace(format!("    {}  {}", f.status, f.path.display()));
                        }
                    });
            }

            if let Some(err) = &state.error {
                ui.add_space(6.0);
                ui.colored_label(theme::danger(), err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!state.busy, |ui| {
                    if ui.button("Cancel").clicked() {
                        do_cancel = true;
                    }
                    let confirm_label = if delete_branch {
                        format!("{action_label} + delete branch")
                    } else {
                        action_label.to_string()
                    };
                    if ui.add(theme::danger_button(&confirm_label)).clicked() {
                        do_confirm = true;
                    }
                });
                if state.busy {
                    ui.add_space(4.0);
                    ui.spinner();
                    ui.label("removing…");
                }
            });
        });

    if delete_branch != state.delete_branch {
        if let Some(s) = app.confirm_remove_worktree.lock().unwrap().as_mut() {
            s.delete_branch = delete_branch;
        }
    }

    if do_confirm {
        app.execute_remove_worktree(ctx);
    } else if do_cancel {
        app.cancel_remove_worktree_confirm();
    }
}

fn render_shared_checks(ui: &mut egui::Ui, state: &ConfirmRemoveWorktree) {
    let safety = RemovalSafety::evaluate(&state.removal_facts, RemovalIntent::WorktreeOnly);
    ui.add_space(6.0);
    for check in safety.checks() {
        let color = match check.outcome {
            CheckOutcome::Pass => theme::weak_text(),
            CheckOutcome::Fail | CheckOutcome::Unknown => theme::amber(),
            CheckOutcome::Pending => theme::weak_text(),
        };
        ui.colored_label(
            color,
            format!("{} {}", check.outcome.marker(), check.detail),
        );
    }
}

fn render_branch_delete_section(
    ui: &mut egui::Ui,
    state: &ConfirmRemoveWorktree,
    delete_branch: &mut bool,
) {
    let Some(branch) = &state.branch else {
        return; // detached HEAD — no branch to delete
    };
    let Some(assessment) = &state.branch_assessment else {
        ui.label(format!("Branch '{branch}' will remain after removal."));
        return;
    };

    ui.add_space(6.0);

    if assessment.is_blocked() {
        *delete_branch = false;
        let where_ = assessment
            .other_checkouts
            .first()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "another worktree".to_string());
        ui.colored_label(
            theme::muted_text(),
            format!("Branch '{branch}' is checked out at {where_} - can't delete it here."),
        );
        return;
    }

    let landed = RemovalSafety::evaluate(&state.removal_facts, RemovalIntent::WorktreeAndBranch)
        .checks()
        .iter()
        .find(|c| c.check == RemovalCheck::WorkLanded)
        .map(|c| (c.outcome, c.detail.clone()));

    match landed {
        Some((CheckOutcome::Pass, detail)) => {
            ui.checkbox(
                delete_branch,
                format!("Also delete branch '{branch}' ({})", detail.to_lowercase()),
            );
        }
        Some((_, detail)) => {
            ui.checkbox(
                delete_branch,
                egui::RichText::new(format!("⚠ Force-delete branch '{branch}' - {detail}"))
                    .color(theme::danger()),
            );
        }
        None => {
            ui.label(format!("Branch '{branch}' will remain after removal."));
        }
    }
}

fn render_kill_all_modal(app: &mut HiveApp, ui: &mut egui::Ui) {
    let ctx = &ui.ctx().clone();
    if !app.confirm_kill_all {
        return;
    }
    let pgids = unique_pgids_in_filter(app);
    let mut open = true;
    let mut do_confirm = false;
    let mut do_cancel = false;
    let n = pgids.len();
    egui::Window::new("Confirm kill all")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "Send SIGTERM (then SIGKILL after 3s) to {n} unique process group{} in \
                 the current filter?",
                if n == 1 { "" } else { "s" }
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(theme::danger_button("Confirm")).clicked() {
                    do_confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    do_cancel = true;
                }
            });
        });
    if do_confirm {
        app.spawn_kill_many(pgids, ctx);
        app.confirm_kill_all = false;
    } else if do_cancel || !open {
        app.confirm_kill_all = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::RemovalVerdict;

    /// The row's lavender "this holds work" signal (`has_unlanded_work`,
    /// consumed by the landing-stage chip) and
    /// its `remove ok` badge (the Actions cell's trash icon color, via
    /// `RemovalSafety`) must never contradict each other — carried over
    /// verbatim from the retired swimlane view's identical regression test.
    ///
    /// They used to: the signal counted commits ahead of the *local* `main`
    /// by ancestry, while the badge asked whether the content was on
    /// `default_branch()`. A rebase-merged worktree therefore lit the drift
    /// chip, the repo card's count, the lavender dot and the auto-expand
    /// rule, all while its own badge said the work was safely upstream. On
    /// one real machine that was 9 of 41 worktrees.
    ///
    /// Both now read the same patch-equivalence probe, so this walks the two
    /// states that matter and asserts they agree in each.
    #[test]
    fn the_rows_unlanded_signal_agrees_with_its_removal_badge() {
        use switchbard_core::{Fact, LandedEvidence, TrunkDivergence, WorktreeStaleness};

        let meta = |unlanded: u32, staleness: WorktreeStaleness| WorktreeMeta {
            dirty_files: Some(vec![]),
            lock: Fact::Known(None),
            trunk: Some(TrunkDivergence {
                base: "origin/main".into(),
                unlanded,
                // Ahead by two more than are at risk: the rebase-merged case
                // this test exists for.
                ancestry_ahead: unlanded + 2,
                behind: 12,
            }),
            staleness: Some(staleness),
            ..Default::default()
        };

        // Rebase-merged: ahead by ancestry, nothing at risk.
        let landed = meta(
            0,
            WorktreeStaleness::Merged {
                base: "origin/main".into(),
                evidence: LandedEvidence::PatchEquivalent,
            },
        );
        assert!(
            !has_unlanded_work(&landed.trunk),
            "a rebase-merged worktree holds nothing the trunk lacks"
        );
        assert_eq!(
            RemovalSafety::evaluate(
                &crate::runtime::removal_facts(false, &landed, AttachedProcesses::default()),
                RemovalIntent::WorktreeAndBranch,
            )
            .verdict(),
            RemovalVerdict::Safe,
            "…and the badge has to say so too"
        );

        // Genuinely unlanded: both surfaces must flag it.
        let at_risk = meta(5, WorktreeStaleness::NoUpstream);
        assert!(has_unlanded_work(&at_risk.trunk));
        assert_eq!(
            RemovalSafety::evaluate(
                &crate::runtime::removal_facts(false, &at_risk, AttachedProcesses::default()),
                RemovalIntent::WorktreeAndBranch,
            )
            .verdict(),
            RemovalVerdict::Blocked,
        );
    }
}
