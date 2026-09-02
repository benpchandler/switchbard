//! Background threads that feed the GUI.
//!
//! The first four workers follow the same shape:
//!   1. Take a snapshot of whatever inputs they need (under a brief lock).
//!   2. Do work outside any lock.
//!   3. Write results back into the shared `Mutex`, then `ctx.request_repaint()`.
//!   4. Sleep via `kick.wait(period)`.
//!
//! The fifth (`spawn_dispatch`) is the odd one out: it has no `Mutex` of its
//! own to write into, because a dispatched task's state — `dispatch` /
//! `dispatching` / `dispatched` / `dispatch-failed` — already lives on the
//! task itself (its label) via `switchbard_core::dispatch`. It publishes
//! nothing new; it just runs the pipeline and kicks `backlog_kick` so the
//! *existing* backlog worker's next (or forced) reload picks up the label
//! and notes change. (TASK-43's pgid sidecar does not change that: it is a
//! file the *pipeline* writes and deletes within one run's lifetime, read
//! back off disk by `refresh_dispatch_runs` like every other `DispatchRun`
//! field — no worker state, nothing for this thread to publish or own.)
//! The backlog worker additionally *sweeps* sidecars left behind by a
//! Switchbard that died mid-run; see `sweep_sidecar_if_finished` for why that
//! deliberately spares a sidecar whose task is still claimed. See its own doc for why one iteration can block far
//! longer than the other workers' — this reuses `drain_dispatch_queue`'s
//! serial-by-design batching rather than reimplementing it. TASK-46 removed
//! the wall-clock kill inside that pipeline, so this is no longer "far longer,
//! bounded by 30 minutes per queued task" — it is unbounded: a single long
//! run blocks this thread's loop for as long as that run takes, and every
//! other task queued behind it in the same drain call waits with it. Accepted
//! consequence, not a bug — see `switchbard_core::dispatch`'s module doc.
//!
//! Centralizing the spawning here keeps `HiveApp::new` short and stops the
//! "what does this anonymous closure do?" question from recurring.
//!
//! ## Cadence policy (owner audit, 2026-08-05 real machine: 6 repos, 84
//! worktrees via `~/.switchbard/config.toml` — see `examples/
//! scan_cadence_audit.rs`, a read-only instrument you can re-run to get
//! fresh before/after numbers)
//!
//! | Worker | Period | Measured per-tick cost (84 worktrees) | Why |
//! |---|---|---|---|
//! | scanner | 3s | ~0.2s, 1 subprocess, independent of worktree count | UX-critical: the Servers view's whole point is "what's listening right now." Kept snappy. |
//! | git probe | 120s (was 60s) | ~6-8s once `probe_ignored_files` is decoupled (was ~37-40s every tick — see below) | Real git-subprocess cost (~10/worktree) that scales with worktree count; drift/dirty/recent-commits data is useful within a couple minutes' staleness, not seconds. |
//! | — ignored-files sub-probe | every 5th probe tick (~10 min) | ~32s of the ~37s pre-fix tick (measured in isolation) — `git status --ignored` can't prune subtrees the way plain status does | Tooltip-only cosmetic data (see `IGNORED_FILES_PREVIEW_LIMIT`'s own doc); by far the single most expensive call in this module. Decoupling it from the main probe cadence is the highest-leverage fix found by this audit. |
//! | — staleness sub-probe (TASK-41) | every probe tick | **free** — derived, not probed | Was ~1.9s / ~2-3 subprocesses per worktree (61 worktrees, re-measured 2026-08-26). It asked git the same three questions `probe_trunk_divergence` asks, so `staleness_from_trunk` now derives the Merged/NoUpstream/Live badge from that one comparison. Removes the duplicate calls *and* makes badge-vs-chip disagreement unrepresentable rather than merely unlikely. |
//! | — trunk comparison | every probe tick | ~1.5-1.7s over 61 worktrees (~25-28ms each, measured 2026-08-26 via `examples/scan_cadence_audit.rs`) | Replaced `probe_main_drift` (~0.6s): it resolves the trunk via `default_branch` instead of assuming local `main`, and counts by patch-equivalence rather than ancestry, so it is ~3 subprocesses rather than 2. The staleness derivation above more than pays for it — the whole tick went 21.5s → 21.1s. |
//! | detection | 60s (was 30s) | ~0.15s cold, ~0 steady-state (idempotent — skips worktrees already in `services`) | No urgency: a newly tracked worktree still gets detected within a minute. |
//! | agent-context | 60s (was 30s), capped at `AGENT_CONTEXT_MAX_MISSING_PER_TICK` new worktrees per tick | ~47s in one unbroken burst pre-fix (cold scan of all 84 at once) | Recursive per-worktree filesystem walk; cheap in steady state (only rescans missing/>24h-stale entries) but a cold launch or adding several repos at once used to stall the thread for tens of seconds in a single tick. Capping the batch turns that into several bounded, interleaved ticks instead. |
//! | backlog | 30s (unchanged) | ~0.15-0.2s over 6 repo *roots*, not per-worktree | Already cheap at this scale (one load per tracked repo, not per worktree) and users watch task state change in near-real-time — no evidence to slow this down. |
//! | dispatch | 90s (unchanged) | negligible when the queue is empty (the common case); unbounded while a run is in flight (TASK-46 removed the wall-clock kill — see `spawn_dispatch`'s own doc) | Opt-in and rare by design — see its own doc. Unaffected by worktree count. |
//! | mission projection | 2s focused / 16s unfocused | one bounded local JSON read, capped at 4 MiB / 500 missions | Decision and hold state should feel live, but the optional xplan snapshot never belongs on the render path. Missing and invalid files publish explicit cache states. |
//! | size (TASK-41) | 300s, bounded catch-up batch of 5 | ~650ms **per worktree** average (measured 2026-08-19 via `examples/scan_cadence_audit.rs`, sampled 20/84 real worktrees, `du -sk`; a manual sweep of `~/Dev/.worktrees`'s larger checkouts saw individual calls up to ~1.5s) — an order of magnitude past every other per-worktree probe | `du` walks the whole tree (node_modules/target/build artifacts); see `worktree_size.rs`'s own doc. Never runs inline with the git-probe tick — its own worker, own cadence, catches up a bounded batch of never-yet-sized worktrees per tick (same shape as agent-context's cold-start batching below) rather than blocking on a full sweep. |
//! | landing (feat/landing-stage) | 300s, bounded catch-up batch of 5 | `probe_push_state` is one free local `rev-parse`; `probe_pr_state` is ~1s of `gh` per branch by that function's own doc — same order of magnitude as `du`'s per-worktree cost above, so this worker reuses `size`'s exact period/batch rather than deriving a new pair | Only ever probes worktrees `spawn_probe` has already found to have unlanded commits (a clean worktree has no "why" to ask) with a real branch (a detached HEAD has nothing to push or open a PR from). Never runs inline with the git-probe tick, same reason `size` doesn't — see `spawn_landing`'s own doc and the hard constraint in `switchbard_core::landing`'s module doc: **never call `probe_pr_state` from the git-probe tick.** |
//! | reaper | 2s (unchanged) | negligible, in-memory PGID check only | Not part of the worktree-count scaling problem this audit targets. |
//! | agent-sessions (TASK-98) | 5s | one `ps`/`/proc` walk, independent of worktree count — same shape as `scanner`'s own cost | Conservative rather than matching `scanner`'s 3s: an interactive agent session changing is a human starting/stopping a terminal, not a live listening socket the Servers view has to track second-by-second. 5s keeps the Command place's Fleet section close to live without adding a second sub-3s subprocess tick alongside the scanner's. |
//!
//! Two cross-cutting mechanisms apply on top of the table above:
//! - **Startup stagger** (`stagger_offset`): each worker's *first* tick is
//!   offset by `WORKER_STAGGER_SPACING`, so `spawn_all` doesn't fire every
//!   worker's cold (most expensive) pass in the same instant.
//! - **Focus-aware backoff** (`effective_period`): every worker multiplies
//!   its period by `UNFOCUSED_BACKOFF_MULTIPLIER` while the OS window
//!   doesn't have focus (`ctx.input(|i| i.focused)` — read directly off the
//!   `egui::Context` handle already threaded into every worker, so this
//!   needs no new plumbing through `HiveApp`/`Channels`). A backgrounded
//!   Switchbard alt-tabbed away doesn't need second-by-second freshness.

use crate::mission_control::MissionControlModel;
use crate::runtime::worktrees::expand_worktrees;
use crate::runtime::{
    attached_processes_for, is_retired_worktree, ActiveRun, BacklogTaskKey, FileListSummary,
    LandingEntry, OrderingState, TasksReadState, WorktreeMeta, WorktreeSizeEntry,
};
use crate::sync::Kick;
use eframe::egui;
use switchbard_core::dispatch_inspect::{inspect_dispatch_run, DispatchRun};
use switchbard_core::{
    agent_context_needs_rescan, attribute, attribute_agent_sessions, detect_services,
    drain_dispatch_queue, find_hub_repo, is_backlog_repo, list_dispatch_queue, load_backlog_repo,
    load_ordering_overlay, probe_dirty_files, probe_fetch_age, probe_head_commit_time,
    probe_ignored_files, probe_pr_state, probe_push_state, probe_recent_commits,
    probe_ref_drift_detail, probe_remote_drift, probe_trunk_detail, probe_trunk_divergence,
    probe_worktree_lock, probe_worktree_size, save_agent_context_cache, scan_agent_context,
    scan_agent_sessions, scan_listeners, staleness_from_trunk, sweep_dead_sidecar, AgentContextMap,
    AgentSession, BacklogRepo, DecisionScope, DecisionStatus, DetectedService, DispatchOptions,
    DriftProbe, Fact, LandingStage, MissionProjectionLoad, MissionStatus, PushState, Repo,
    WorktreeRef, DISPATCHED_LABEL, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};

/// How many commits we list per side (ahead / behind) in the drift tooltip.
/// Larger lists overflow the tooltip; the count badge in the cell still
/// communicates the total.
const DRIFT_DETAIL_LIMIT: usize = 5;

/// How many recent commits we keep per worktree for the ACTIVITY column. 10
/// covers the typical "agent-burst over the last hour" hover with room to
/// spare while still bounding the `git log` cost.
const RECENT_COMMITS_LIMIT: usize = 10;
/// Ignored files are tooltip context only; keep a bounded preview so large
/// dependency trees do not make UI snapshots expensive to clone.
const IGNORED_FILES_PREVIEW_LIMIT: usize = 8;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::app::ScanState;

const SCAN_PERIOD: Duration = Duration::from_secs(3);
/// Raised from 60s after the 2026-08-05 cadence audit measured a full
/// per-worktree probe pass at ~37-40s wall time on an 84-worktree machine —
/// see this module's doc table. Even with `probe_ignored_files` decoupled
/// (below), a ~6-8s real cost every 60s was still a needlessly tight duty
/// cycle for data that's fine a couple minutes stale.
const PROBE_PERIOD: Duration = Duration::from_secs(120);
/// Raised from 30s: detection is idempotent (skips any worktree already in
/// `services`), so there is no per-tick cost to amortize past "a newly
/// tracked worktree gets detected within about a minute."
const DETECT_PERIOD: Duration = Duration::from_secs(60);
/// Raised from 30s: only rescans worktrees missing from the cache or older
/// than `CONTEXT_CACHE_MAX_AGE` (24h), so — same reasoning as detection —
/// there's no steady-state cost to trade off against faster polling.
const CONTEXT_PERIOD: Duration = Duration::from_secs(60);
/// Unchanged: measured at ~0.15-0.2s over 6 repo *roots* (not per-worktree)
/// on the real audit machine — already cheap at this scale, and users watch
/// task/status changes close to real time.
const BACKLOG_PERIOD: Duration = Duration::from_secs(30);
/// Longer than the other periods on purpose: dispatch is opt-in and rare
/// (a task only enters the queue via an explicit user action), and one
/// iteration can itself take many minutes (a full headless `claude -p` run
/// per queued task) — a short poll period would just mean more overlapping
/// wakeups against a worker that's usually idle-checking an empty queue.
const DISPATCH_PERIOD: Duration = Duration::from_secs(90);
/// A tiny local JSON file with user-visible supervision state. Two seconds
/// keeps decision/hold changes perceptibly fresh without putting file I/O on
/// the render path; focus backoff stretches this to 16 seconds in background.
const MISSION_PROJECTION_PERIOD: Duration = Duration::from_secs(2);
const CONTEXT_CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24);
const REAPER_PERIOD: Duration = Duration::from_secs(2);
/// TASK-98: see this module's cadence-policy doc table for the "why 5s, not
/// `scanner`'s 3s" rationale.
const AGENT_SESSIONS_PERIOD: Duration = Duration::from_secs(5);
/// TASK-41: on-disk size (`du`) is by far the most expensive per-worktree
/// probe in the app — measured ~0.8-1.5s each against real worktrees (see
/// this module's cadence-policy doc table), roughly an order of magnitude
/// past `probe_ignored_files` (the previous record-holder). A worktree's
/// size also changes slowly in practice (mostly build-artifact churn), so a
/// 5-minute steady-state cadence for the single-stalest-entry refresh is
/// generous, not stingy.
const SIZE_PERIOD: Duration = Duration::from_secs(300);
/// An entry older than this becomes eligible for the steady-state
/// single-entry refresh — mirrors `CONTEXT_CACHE_MAX_AGE`'s role for
/// agent-context, scaled down because size is refreshed far less urgently
/// than dirty/drift but still ought to catch up within the hour.
const SIZE_CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 30);
/// Bounded per-tick batch for never-yet-sized worktrees, same rationale as
/// `AGENT_CONTEXT_MAX_MISSING_PER_TICK`: at ~1s/worktree, sizing all of a
/// 138-worktree machine in one unbroken tick would stall this thread for
/// well over a minute. 5/tick keeps each tick's worst case bounded to a few
/// seconds while a large backlog still drains promptly (this worker skips
/// its sleep and loops again immediately when more remain — same shape as
/// `spawn_agent_context`).
const SIZE_MAX_MISSING_PER_TICK: usize = 5;

/// Same period as `SIZE_PERIOD` and for the same reason: `probe_pr_state`'s
/// own doc prices `gh` at roughly a second per branch, the same order of
/// magnitude as `du`'s per-worktree cost that justifies `SIZE_PERIOD` — see
/// this module's cadence-policy table. A PR's review status also does not
/// change minute to minute, so a 5-minute steady-state refresh for the
/// single-stalest entry is generous, not stingy.
const LANDING_PERIOD: Duration = Duration::from_secs(300);
/// Mirrors `SIZE_CACHE_MAX_AGE`: an entry older than this becomes eligible
/// for the steady-state single-entry refresh.
const LANDING_CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 30);
/// Mirrors `SIZE_MAX_MISSING_PER_TICK`: bounds a cold-start catch-up (many
/// worktrees with unlanded work and no cached stage yet) to a few seconds per
/// tick instead of one multi-minute stall, while a large backlog still drains
/// promptly — this worker also skips its sleep and loops again immediately
/// when more remain, same shape as `spawn_size`.
const LANDING_MAX_MISSING_PER_TICK: usize = 5;

/// Every Nth git-probe tick recomputes `probe_ignored_files`; other ticks
/// carry forward the previously cached value. The 2026-08-05 cadence audit
/// isolated this one call at ~32s of a ~37s tick over 84 real worktrees —
/// `git status --ignored` cannot prune ignored subtrees the way a plain
/// status can, so it dominates every other probe combined by roughly 5x.
/// The data itself is tooltip-only context (see `IGNORED_FILES_PREVIEW_LIMIT`'s
/// doc), so decoupling its cadence from the rest of the probe (which stays
/// fresh every `PROBE_PERIOD`) trades cosmetic staleness — bounded at
/// `IGNORED_FILES_REFRESH_EVERY_N_PROBES * PROBE_PERIOD`, ~10 minutes at the
/// current settings — for the single biggest win this audit found.
const IGNORED_FILES_REFRESH_EVERY_N_PROBES: u64 = 5;

/// Cap on how many never-scanned worktrees `spawn_agent_context` processes
/// per tick. Without this cap, a cold launch (or tracking several new repos
/// at once) scans every missing worktree in one unbroken burst — measured
/// at ~47s over 84 worktrees in the same audit. When more than this many
/// are still missing after a tick, the worker loops again immediately
/// (skipping the sleep) rather than waiting a full `CONTEXT_PERIOD`, so a
/// large backlog still drains promptly — just as several bounded,
/// interleaved ticks (each followed by its own `ctx.request_repaint()`)
/// instead of one long stall with no visible progress.
const AGENT_CONTEXT_MAX_MISSING_PER_TICK: usize = 10;

/// Multiply a worker's period by this factor while the OS window doesn't
/// have focus. See `effective_period`.
const UNFOCUSED_BACKOFF_MULTIPLIER: u32 = 8;

/// Spacing between each worker's first tick (see `stagger_offset`), so
/// `spawn_all` doesn't fire every worker's cold, most-expensive pass in the
/// same instant.
const WORKER_STAGGER_SPACING: Duration = Duration::from_secs(2);

/// The period a worker should actually sleep for, given its nominal
/// `period` and whether the window currently has OS focus. Pure so it's
/// unit-testable without a real `egui::Context`.
fn effective_period(period: Duration, focused: bool) -> Duration {
    if focused {
        period
    } else {
        period * UNFOCUSED_BACKOFF_MULTIPLIER
    }
}

/// `true` on the probe tick that should recompute `probe_ignored_files`.
/// Ticks are 0-indexed so the very first probe of a freshly launched app
/// still gets an accurate ignored-files list instead of starting from
/// `None` and waiting `IGNORED_FILES_REFRESH_EVERY_N_PROBES` ticks for one.
fn should_refresh_ignored_files(tick: u64) -> bool {
    tick.is_multiple_of(IGNORED_FILES_REFRESH_EVERY_N_PROBES)
}

/// Stagger offset for the `index`-th worker `spawn_all` starts (0-indexed),
/// so their first ticks land spread out rather than simultaneous.
fn stagger_offset(index: u32) -> Duration {
    WORKER_STAGGER_SPACING * index
}

/// Shared handles that every worker reads from / writes to. Bundling them
/// lets `spawn_all` take one argument instead of nine.
#[derive(Clone)]
pub struct Channels {
    pub state: Arc<Mutex<ScanState>>,
    pub repos: Arc<Mutex<Vec<Repo>>>,
    pub worktrees: Arc<Mutex<Vec<WorktreeRef>>>,
    pub meta: Arc<Mutex<HashMap<PathBuf, WorktreeMeta>>>,
    pub services: Arc<Mutex<HashMap<PathBuf, Vec<DetectedService>>>>,
    pub agent_contexts: Arc<Mutex<HashMap<PathBuf, AgentContextMap>>>,
    pub backlog_repos: Arc<Mutex<HashMap<PathBuf, BacklogRepo>>>,
    pub tasks_read_state: Arc<Mutex<TasksReadState>>,
    pub dispatch_runs: Arc<Mutex<HashMap<BacklogTaskKey, DispatchRun>>>,
    /// TASK-98: live interactive `claude`/`codex` sessions, refreshed by
    /// `spawn_agent_sessions` — see that worker's doc.
    pub agent_sessions: Arc<Mutex<Vec<AgentSession>>>,
    pub ordering: Arc<Mutex<OrderingState>>,
    pub active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    /// TASK-41: on-disk size per worktree, refreshed by `spawn_size` on its
    /// own slow cadence — see that worker's doc for why it can't share the
    /// git-probe tick.
    pub sizes: Arc<Mutex<HashMap<PathBuf, WorktreeSizeEntry>>>,
    /// TASK-41: count of non-primary, clean, fully-merged worktrees —
    /// written once per git-probe tick (`spawn_probe`, alongside `meta`
    /// itself) rather than recomputed every frame the top bar renders. The
    /// top bar reads this directly (`ui::top_bar::render_retired_worktrees_
    /// nudge`) instead of cloning `repos`/`worktrees` and locking `meta` on
    /// every frame across every tab.
    pub retired_worktree_count: Arc<Mutex<usize>>,
    /// feat/landing-stage: why each unlanded worktree is still unlanded,
    /// refreshed by `spawn_landing` on its own slow cadence — see that
    /// worker's doc for why it can't share the git-probe tick.
    pub landing: Arc<Mutex<HashMap<PathBuf, LandingEntry>>>,
    pub mission_projection: Arc<Mutex<Arc<MissionProjectionLoad>>>,
    pub mission_control: Arc<Mutex<MissionControlModel>>,
    pub mission_projection_path: Option<PathBuf>,
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
    pub agent_context_kick: Kick,
    pub backlog_kick: Kick,
    pub dispatch_kick: Kick,
    pub size_kick: Kick,
    pub landing_kick: Kick,
    pub agent_sessions_kick: Kick,
    pub mission_projection_kick: Kick,
}

pub fn spawn_all(ctx: egui::Context, ch: Channels) {
    spawn_scanner(ctx.clone(), ch.clone(), stagger_offset(0));
    spawn_probe(ctx.clone(), ch.clone(), stagger_offset(1));
    spawn_detection(ctx.clone(), ch.clone(), stagger_offset(2));
    spawn_agent_context(ctx.clone(), ch.clone(), stagger_offset(3));
    spawn_backlog(ctx.clone(), ch.clone(), stagger_offset(4));
    spawn_dispatch(ctx.clone(), ch.clone(), stagger_offset(5));
    spawn_size(ctx.clone(), ch.clone(), stagger_offset(6));
    spawn_landing(ctx.clone(), ch.clone(), stagger_offset(7));
    spawn_agent_sessions(ctx.clone(), ch.clone(), stagger_offset(8));
    spawn_mission_projection(ctx.clone(), ch.clone());
    spawn_reaper(ctx, ch);
}

/// Optional xplan Mission Command projection: bounded file read off the UI
/// thread, one cache swap, one repaint, then a focus-aware sleep. A validated
/// v2 contract hold may separately trigger one bounded private read through
/// the supervisor; the projection itself is never written back to xplan.
fn spawn_mission_projection(ctx: egui::Context, ch: Channels) {
    let Some(path) = ch.mission_projection_path.clone() else {
        return;
    };
    thread::spawn(move || loop {
        let loaded = switchbard_core::load_mission_projection(&path);
        if let MissionProjectionLoad::Ready { freshness, .. } = &loaded {
            ch.mission_control.lock().unwrap().projection_freshness = freshness.clone();
        }
        spawn_pending_contract_recovery(&ctx, &ch, &loaded);
        *ch.mission_projection.lock().unwrap() = Arc::new(loaded);
        ctx.request_repaint();
        let focused = ctx.input(|i| i.focused);
        ch.mission_projection_kick
            .wait(effective_period(MISSION_PROJECTION_PERIOD, focused));
    });
}

fn spawn_pending_contract_recovery(
    ctx: &egui::Context,
    ch: &Channels,
    loaded: &MissionProjectionLoad,
) {
    let MissionProjectionLoad::Ready { projection, .. } = loaded else {
        return;
    };
    if !loaded.controls_enabled() {
        return;
    }
    let Some(mission) = projection.portfolio.missions.iter().find(|mission| {
        mission.status == MissionStatus::NeedsDecision
            && mission.decision.as_ref().is_some_and(|decision| {
                decision.status == DecisionStatus::Open
                    && decision.scope == Some(DecisionScope::MissionContract)
            })
    }) else {
        return;
    };
    let (Some(revision), Some(decision)) = (mission.mission_revision, mission.decision.as_ref())
    else {
        return;
    };
    let prepared = ch.mission_control.lock().unwrap().begin_contract_recovery(
        &mission.id,
        revision,
        &decision.id,
        decision.version,
    );
    let Ok(Some((supervisor, request))) = prepared else {
        return;
    };
    let model = ch.mission_control.clone();
    let ctx = ctx.clone();
    thread::spawn(move || {
        let result = supervisor.invoke(request);
        model.lock().unwrap().finish_contract_recovery(result);
        ctx.request_repaint();
    });
}

/// Scanner: re-runs `lsof` every SCAN_PERIOD (or sooner if kicked), attributes
/// each listener to a worktree, publishes the result to `state.listeners`.
fn spawn_scanner(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.scanner_kick.wait(initial_delay);
        loop {
            let result = scan_listeners();
            let now = Instant::now();
            let wts = ch.worktrees.lock().unwrap().clone();
            {
                let mut s = ch.state.lock().unwrap();
                match result {
                    Ok(listeners) => {
                        s.listeners = attribute(&listeners, &wts);
                        s.last_error = None;
                    }
                    Err(e) => s.last_error = Some(e.to_string()),
                }
                s.last_scan = Some(now);
            }
            ctx.request_repaint();
            let focused = ctx.input(|i| i.focused);
            ch.scanner_kick.wait(effective_period(SCAN_PERIOD, focused));
        }
    });
}

/// Agent sessions (TASK-98): re-scans the OS for running `claude`/`codex`
/// processes every `AGENT_SESSIONS_PERIOD` (or sooner if kicked), attributes
/// each to a worktree, publishes the result to `ch.agent_sessions`. Same
/// shape as `spawn_scanner` — this is the same kind of read-only OS-process
/// scan, just for interactive agent CLIs instead of listening sockets. Feeds
/// the Command place's Fleet section (`ui::places::command`) exclusively;
/// nothing else reads `agent_sessions`.
fn spawn_agent_sessions(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.agent_sessions_kick.wait(initial_delay);
        loop {
            let wts = ch.worktrees.lock().unwrap().clone();
            if let Ok(rows) = scan_agent_sessions() {
                *ch.agent_sessions.lock().unwrap() = attribute_agent_sessions(&rows, &wts);
                ctx.request_repaint();
            }
            let focused = ctx.input(|i| i.focused);
            ch.agent_sessions_kick
                .wait(effective_period(AGENT_SESSIONS_PERIOD, focused));
        }
    });
}

/// Git probe: each iteration re-enumerates worktrees from `git worktree list`
/// (so external `git worktree prune` / `add` get picked up), then walks the
/// fresh list running dirty/ahead/behind/last-commit probes.
///
/// `probe_ignored_files` — by far the most expensive single call here (see
/// this module's cadence-policy doc table) — only actually re-runs every
/// `IGNORED_FILES_REFRESH_EVERY_N_PROBES`th tick; other ticks carry forward
/// whatever was last cached for that worktree in `ch.meta`.
fn spawn_probe(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.probe_kick.wait(initial_delay);
        let mut tick: u64 = 0;
        loop {
            // Step 1: re-enumerate worktrees from disk and publish.
            let repos = ch.repos.lock().unwrap().clone();
            {
                let fresh = expand_worktrees(&repos);
                *ch.worktrees.lock().unwrap() = fresh;
                ctx.request_repaint();
            }
            // Step 2: probe each. `repo_paths` resolves a worktree's
            // `repo_name` back to its primary checkout path — `WorktreeRef`
            // itself only carries the name (see its own doc), and
            // `probe_worktree_staleness` needs the repo path to find the
            // local default branch to compare against.
            let repo_paths = repo_paths_by_name(&repos);
            let wts = ch.worktrees.lock().unwrap().clone();
            // Snapshot the three process sources once per tick, not once per
            // worktree: the retired count needs them for every worktree, and
            // re-locking 75 times a tick to answer the same question is a
            // contention source the render thread would feel.
            let runs_now = ch.active_runs.lock().unwrap().clone();
            let dispatch_now = ch.dispatch_runs.lock().unwrap().clone();
            let listener_counts: HashMap<PathBuf, usize> = {
                let state = ch.state.lock().unwrap();
                let mut counts: HashMap<PathBuf, usize> = HashMap::new();
                for listener in &state.listeners {
                    if let Some(path) = &listener.worktree_path {
                        *counts.entry(path.clone()).or_default() += 1;
                    }
                }
                counts
            };
            let refresh_ignored = should_refresh_ignored_files(tick);
            // TASK-41: accumulated alongside the per-worktree probe loop
            // rather than in a second pass over `wts` — see
            // `retired_worktree_count`'s own doc for why this is cached at
            // all instead of recomputed per-frame by the top bar.
            let mut retired = 0usize;
            for w in &wts {
                let remote_drift = probe_remote_drift(&w.path);
                let remote_drift_detail = drift_detail_for_probe(&w.path, remote_drift.as_ref());
                let ignored_files = if refresh_ignored {
                    probe_ignored_files(&w.path).map(|files| {
                        FileListSummary::from_lines(files, IGNORED_FILES_PREVIEW_LIMIT)
                    })
                } else {
                    ch.meta
                        .lock()
                        .unwrap()
                        .get(&w.path)
                        .and_then(|m| m.ignored_files.clone())
                };
                let repo_path = repo_paths.get(&w.repo_name);
                // One trunk comparison, two surfaces. The Merged/NoUpstream/Live
                // badge and the row's unlanded chip are the same question
                // asked twice, so they are derived from one probe rather than
                // run as two — which costs three fewer git subprocesses per
                // worktree per tick, and makes "badge and chip disagree"
                // unrepresentable rather than merely unlikely.
                let trunk =
                    repo_path.and_then(|repo_path| probe_trunk_divergence(repo_path, &w.path));
                let staleness =
                    repo_path.map(|_| staleness_from_trunk(trunk.as_ref(), remote_drift.as_ref()));
                let trunk_detail = trunk
                    .as_ref()
                    .and_then(|d| probe_trunk_detail(&w.path, d, DRIFT_DETAIL_LIMIT));
                // Same tick as `staleness`: a lock is a removal precondition
                // that changes about as often, and pairing them means the
                // row's badge never shows a lock state from a different
                // moment than the merged state it sits beside.
                let lock = match repo_path {
                    Some(repo_path) => probe_worktree_lock(repo_path, &w.path),
                    None => Fact::Unavailable(
                        "This worktree's repo isn't tracked, so its lock state can't be read"
                            .to_string(),
                    ),
                };
                let m = WorktreeMeta {
                    dirty_files: probe_dirty_files(&w.path),
                    ignored_files,
                    trunk,
                    remote_drift,
                    trunk_detail,
                    remote_drift_detail,
                    head_commit_unix: probe_head_commit_time(&w.path),
                    fetch_unix: probe_fetch_age(&w.path),
                    recent_commits: probe_recent_commits(&w.path, RECENT_COMMITS_LIMIT),
                    probed_at: Some(Instant::now()),
                    staleness,
                    lock,
                };
                let attached = attached_processes_for(
                    &w.path,
                    listener_counts.get(&w.path).copied().unwrap_or(0),
                    &runs_now,
                    &dispatch_now,
                );
                if is_retired_worktree(w, &repos, Some(&m), attached) {
                    retired += 1;
                }
                ch.meta.lock().unwrap().insert(w.path.clone(), m);
                ctx.request_repaint();
            }
            *ch.retired_worktree_count.lock().unwrap() = retired;
            tick = tick.wrapping_add(1);
            let focused = ctx.input(|i| i.focused);
            ch.probe_kick.wait(effective_period(PROBE_PERIOD, focused));
        }
    });
}

/// `repo_name -> primary checkout path`, built fresh each probe tick from the
/// same `repos` snapshot `expand_worktrees` just consumed. Small (one entry
/// per tracked repo, not per worktree) so rebuilding it every tick is free.
fn repo_paths_by_name(repos: &[Repo]) -> HashMap<String, PathBuf> {
    repos
        .iter()
        .map(|r| (r.name.clone(), r.path.clone()))
        .collect()
}

fn drift_detail_for_probe(
    path: &Path,
    probe: Option<&DriftProbe>,
) -> Option<switchbard_core::DriftDetail> {
    let Some(DriftProbe::Ready {
        base,
        ahead,
        behind,
    }) = probe
    else {
        return None;
    };
    if ahead + behind == 0 {
        return None;
    }
    probe_ref_drift_detail(path, base, DRIFT_DETAIL_LIMIT)
}

/// Service detection: for each worktree we haven't seen, parse its Procfile /
/// package.json / Makefile / scripts/ and cache the result. Idempotent — once
/// detected, a worktree is skipped on subsequent passes.
fn spawn_detection(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.detection_kick.wait(initial_delay);
        loop {
            let wts = ch.worktrees.lock().unwrap().clone();
            for w in &wts {
                let already = ch.services.lock().unwrap().contains_key(&w.path);
                if already {
                    continue;
                }
                let detected = detect_services(&w.path);
                ch.services.lock().unwrap().insert(w.path.clone(), detected);
                ctx.request_repaint();
            }
            let focused = ctx.input(|i| i.focused);
            ch.detection_kick
                .wait(effective_period(DETECT_PERIOD, focused));
        }
    });
}

/// Agent-context: scans any worktree missing from the cache, or the single
/// stalest entry older than `CONTEXT_CACHE_MAX_AGE`. Missing worktrees are
/// processed in bounded batches of `AGENT_CONTEXT_MAX_MISSING_PER_TICK` —
/// see that constant's doc for why: an unbounded single-tick sweep measured
/// ~47s over 84 worktrees on a cold cache. When more missing worktrees
/// remain after a batch, the loop continues immediately (no sleep) so a
/// large backlog still drains promptly, just as several bounded ticks.
fn spawn_agent_context(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.agent_context_kick.wait(initial_delay);
        loop {
            let wts = ch.worktrees.lock().unwrap().clone();
            let live_paths: std::collections::HashSet<PathBuf> =
                wts.iter().map(|w| w.path.clone()).collect();

            let (batch, more_missing, stale, pruned) = {
                let mut maps = ch.agent_contexts.lock().unwrap();
                let before = maps.len();
                maps.retain(|path, _| live_paths.contains(path));
                let missing: Vec<WorktreeRef> = wts
                    .iter()
                    .filter(|w| !maps.contains_key(&w.path))
                    .cloned()
                    .collect();
                let now = SystemTime::now();
                let stale = wts
                    .iter()
                    .find(|w| {
                        maps.get(&w.path).is_some_and(|map| {
                            agent_context_needs_rescan(map, now, CONTEXT_CACHE_MAX_AGE)
                        })
                    })
                    .cloned();
                let batch: Vec<WorktreeRef> = missing
                    .iter()
                    .take(AGENT_CONTEXT_MAX_MISSING_PER_TICK)
                    .cloned()
                    .collect();
                let more_missing = missing.len() > batch.len();
                (batch, more_missing, stale, maps.len() != before)
            };

            let mut refreshed = false;
            if batch.is_empty() {
                if let Some(w) = stale {
                    scan_and_publish_agent_context(&ch, &w);
                    refreshed = true;
                }
            } else {
                for w in &batch {
                    scan_and_publish_agent_context(&ch, w);
                }
                refreshed = true;
            }

            if refreshed || pruned {
                persist_agent_context_cache(&ch);
                ctx.request_repaint();
            }
            if more_missing {
                continue;
            }
            let focused = ctx.input(|i| i.focused);
            ch.agent_context_kick
                .wait(effective_period(CONTEXT_PERIOD, focused));
        }
    });
}

/// TASK-41: on-disk size per worktree. Same shape as `spawn_agent_context`
/// (bounded catch-up batch for never-yet-sized worktrees, then a
/// single-stalest-entry refresh per tick) because it solves the identical
/// problem — an expensive per-worktree scan that must never block a whole
/// tick on a cold, unbounded sweep. `du`'s per-call cost (~0.8-1.5s measured,
/// see this module's cadence-policy doc) is what actually justifies its own
/// worker rather than folding into `spawn_probe`: even the *bounded* batch
/// here is meaningfully slower than that entire git-probe tick over every
/// worktree.
fn spawn_size(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.size_kick.wait(initial_delay);
        loop {
            let wts = ch.worktrees.lock().unwrap().clone();
            let live_paths: std::collections::HashSet<PathBuf> =
                wts.iter().map(|w| w.path.clone()).collect();

            let (batch, more_missing, stale) = {
                let mut sizes = ch.sizes.lock().unwrap();
                sizes.retain(|path, _| live_paths.contains(path));
                let missing: Vec<PathBuf> = wts
                    .iter()
                    .map(|w| w.path.clone())
                    .filter(|p| !sizes.contains_key(p))
                    .collect();
                let now = Instant::now();
                let stale = wts
                    .iter()
                    .map(|w| w.path.clone())
                    .filter(|p| {
                        sizes
                            .get(p)
                            .is_some_and(|e| now.duration_since(e.computed_at) > SIZE_CACHE_MAX_AGE)
                    })
                    .min_by_key(|p| sizes.get(p).map(|e| e.computed_at));
                let batch: Vec<PathBuf> = missing
                    .iter()
                    .take(SIZE_MAX_MISSING_PER_TICK)
                    .cloned()
                    .collect();
                let more_missing = missing.len() > batch.len();
                (batch, more_missing, stale)
            };

            let mut refreshed = false;
            if batch.is_empty() {
                if let Some(path) = stale {
                    size_and_publish(&ch, &path);
                    refreshed = true;
                }
            } else {
                for path in &batch {
                    size_and_publish(&ch, path);
                }
                refreshed = true;
            }

            if refreshed {
                ctx.request_repaint();
            }
            if more_missing {
                continue;
            }
            let focused = ctx.input(|i| i.focused);
            ch.size_kick.wait(effective_period(SIZE_PERIOD, focused));
        }
    });
}

fn size_and_publish(ch: &Channels, path: &Path) {
    let entry = WorktreeSizeEntry {
        bytes: probe_worktree_size(path),
        computed_at: Instant::now(),
    };
    ch.sizes.lock().unwrap().insert(path.to_path_buf(), entry);
}

/// Landing-stage worker (feat/landing-stage): answers *why* a worktree's
/// unlanded commits are still unlanded — see `switchbard_core::landing`'s
/// module doc for the four situations one "N unlanded" number was hiding on
/// a real machine. Same shape as `spawn_size`: snapshot candidates → probe
/// outside any lock → write back → repaint → sleep, with the same
/// bounded-catch-up-batch/single-stalest-refresh split.
///
/// **Never folded into the git-probe tick** (`spawn_probe`): `probe_pr_state`
/// shells out to `gh`, which costs roughly a second per branch, needs
/// network + auth, and fails for reasons that say nothing about the
/// worktree — see this module's cadence-policy table and
/// `switchbard_core::landing::probe_pr_state`'s own doc, which names this
/// worker as the one place that call belongs.
fn spawn_landing(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.landing_kick.wait(initial_delay);
        loop {
            let wts = ch.worktrees.lock().unwrap().clone();
            let meta = ch.meta.lock().unwrap().clone();
            let candidates = landing_candidates(&wts, &meta);
            let live_paths: std::collections::HashSet<PathBuf> =
                candidates.iter().map(|(p, ..)| p.clone()).collect();

            let LandingBatch {
                batch,
                more_missing,
                stale,
            } = {
                let mut landing = ch.landing.lock().unwrap();
                landing.retain(|path, _| live_paths.contains(path));
                partition_landing_batch(&candidates, &landing, Instant::now())
            };

            let mut refreshed = false;
            if batch.is_empty() {
                if let Some((path, branch, unlanded)) = stale {
                    landing_and_publish(&ch, &path, &branch, unlanded);
                    refreshed = true;
                }
            } else {
                for (path, branch, unlanded) in &batch {
                    landing_and_publish(&ch, path, branch, *unlanded);
                }
                refreshed = true;
            }

            if refreshed {
                ctx.request_repaint();
            }
            if more_missing {
                continue;
            }
            let focused = ctx.input(|i| i.focused);
            ch.landing_kick
                .wait(effective_period(LANDING_PERIOD, focused));
        }
    });
}

/// One worktree's `(path, branch, unlanded-commit-count)` — what
/// [`landing_candidates`] and [`partition_landing_batch`] pass around. Named
/// so the 3-tuple only has to be spelled out once (clippy's
/// `type_complexity`, and a future reader's patience).
type LandingCandidate = (PathBuf, String, u32);

/// [`partition_landing_batch`]'s verdict for one tick.
struct LandingBatch {
    /// Never-cached candidates to probe right now, capped at
    /// `LANDING_MAX_MISSING_PER_TICK`.
    batch: Vec<LandingCandidate>,
    /// More never-cached candidates remain beyond `batch` — the worker loops
    /// again immediately instead of sleeping, same shape as `spawn_size`.
    more_missing: bool,
    /// Only set when `batch` is empty: the single stalest cached candidate,
    /// due for its steady-state refresh.
    stale: Option<LandingCandidate>,
}

/// Which worktrees are candidates for a landing-stage probe at all: a real
/// branch (a detached HEAD has nothing to push or open a PR from) with
/// something unlanded (a clean worktree has no "why" to ask). Everything
/// else gets no cache entry, which is exactly the "render nothing" state the
/// chip wants for those rows — see `ui::places::ops::landing::landing_chip`.
///
/// Pure and worker-thread-free, split out of `spawn_landing`'s loop
/// specifically so this filter (the two hard constraints the mission around
/// this worker turns on) is directly testable without a `gh`/`git`
/// subprocess or a real `Channels`.
fn landing_candidates(
    worktrees: &[WorktreeRef],
    meta: &HashMap<PathBuf, WorktreeMeta>,
) -> Vec<LandingCandidate> {
    worktrees
        .iter()
        .filter_map(|w| {
            let branch = w.branch.clone()?;
            let unlanded = meta.get(&w.path)?.trunk.as_ref()?.unlanded;
            (unlanded > 0).then_some((w.path.clone(), branch, unlanded))
        })
        .collect()
}

/// One tick's batching decision, given this tick's `candidates` and the
/// cache as it stood right after eviction — see [`LandingBatch`] for what
/// each field means.
///
/// Pure (takes `now` rather than reading the clock itself) so batching
/// behavior — cold-start capping, catch-up looping, steady-state refresh
/// picking the *stalest* entry — is testable without threads or real time
/// passing.
fn partition_landing_batch(
    candidates: &[LandingCandidate],
    cached: &HashMap<PathBuf, LandingEntry>,
    now: Instant,
) -> LandingBatch {
    let missing: Vec<LandingCandidate> = candidates
        .iter()
        .filter(|(p, ..)| !cached.contains_key(p))
        .cloned()
        .collect();
    let stale = candidates
        .iter()
        .filter(|(p, ..)| {
            cached
                .get(p)
                .is_some_and(|e| now.duration_since(e.computed_at) > LANDING_CACHE_MAX_AGE)
        })
        .cloned()
        .min_by_key(|(p, ..)| cached.get(p).map(|e| e.computed_at));
    let batch: Vec<LandingCandidate> = missing
        .iter()
        .take(LANDING_MAX_MISSING_PER_TICK)
        .cloned()
        .collect();
    let more_missing = missing.len() > batch.len();
    LandingBatch {
        batch,
        more_missing,
        stale,
    }
}

/// Probe push + PR state for one worktree and publish the derived
/// [`LandingStage`]. `origin` is hardcoded as the remote name, matching every
/// other convention in this codebase that assumes it
/// (`worktree_remove::default_branch`, `dispatch::DispatchOptions`) — nothing
/// here re-derives a remote name from `git remote -v`.
///
/// The one place `Ok(None)` and `Err` from `probe_pr_state` must stay apart
/// (see that function's own doc): `Err` becomes `LandingStage::PrStateUnknown`
/// directly, **bypassing `LandingStage::derive` entirely**, because
/// `derive`'s `pr: None` argument means "GitHub confirmed no PR" — feeding it
/// a failed probe would silently promote "couldn't ask" into "confirmed
/// un-offered", exactly the collapse that module's doc forbids.
fn landing_and_publish(ch: &Channels, path: &Path, branch: &str, unlanded: u32) {
    let push = probe_push_state(path, "origin", branch);
    let stage = match probe_pr_state(path, branch) {
        Ok(pr) => LandingStage::derive(unlanded, &push, pr.as_ref()),
        Err(why) => LandingStage::PrStateUnknown {
            pushed: matches!(push, PushState::Pushed | PushState::PushedStale { .. }),
            why,
        },
    };
    ch.landing.lock().unwrap().insert(
        path.to_path_buf(),
        LandingEntry {
            stage,
            computed_at: Instant::now(),
        },
    );
}

fn scan_and_publish_agent_context(ch: &Channels, w: &WorktreeRef) {
    let map = scan_agent_context(&w.path);
    ch.agent_contexts
        .lock()
        .unwrap()
        .insert(w.path.clone(), map);
}

fn persist_agent_context_cache(ch: &Channels) {
    let maps: Vec<AgentContextMap> = ch
        .agent_contexts
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect();
    let _ = save_agent_context_cache(&maps);
}

/// One Backlog scan root per configured repo — the primary checkout, NOT
/// every worktree. Sibling worktrees each carry a full copy of the same
/// logical backlog, so scanning `ch.worktrees` (as this worker originally
/// did) multiplied every task by the repo's worktree count: with 42 budget
/// worktrees the unified List lens showed 42 copies of each budget task
/// (~48k phantom rows) and the dispatch worker saw 42 drainable queues.
/// The repo's primary checkout is the system-of-record view of its backlog.
pub(crate) fn backlog_repo_roots(repos: &[Repo]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    repos
        .iter()
        .map(|r| r.path.clone())
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

/// One scan's successful snapshots plus the number of task sources that
/// could not be read. Failed roots are intentionally absent from `repos`:
/// the merge step treats that absence as "keep the cached snapshot", while
/// the explicit failure count prevents the UI from calling the result fresh.
#[derive(Debug, Default)]
pub(crate) struct TasksReadResult {
    repos: HashMap<PathBuf, BacklogRepo>,
    failed_repos: usize,
}

/// Load every root that actually is a Backlog repo. Split from the worker
/// loop so the root-set semantics above are testable without threads.
pub(crate) fn collect_backlog_repos(roots: &[PathBuf]) -> TasksReadResult {
    let mut repos = HashMap::new();
    let mut failed_repos = 0;
    for root in roots {
        if !is_backlog_repo(root) {
            continue;
        }
        match load_backlog_repo(root) {
            Ok(repo) => {
                repos.insert(root.clone(), repo);
            }
            Err(error) => {
                failed_repos += 1;
                eprintln!(
                    "Switchbard: failed to refresh tasks from {}: {error}",
                    root.display()
                );
            }
        }
    }
    TasksReadResult {
        repos,
        failed_repos,
    }
}

/// Publish one scan atomically from the read model's point of view. A
/// failed source contributes no fresh entry, so `merge_backlog_repos` keeps
/// its last-known cached rows; the returned lifecycle makes that retention
/// visible as stale rather than silently presenting it as current.
fn apply_tasks_read(
    cache: &mut HashMap<PathBuf, BacklogRepo>,
    roots: &[PathBuf],
    result: TasksReadResult,
) -> TasksReadState {
    merge_backlog_repos(cache, roots, result.repos);
    if result.failed_repos == 0 {
        TasksReadState::Ready
    } else {
        TasksReadState::Stale {
            failed_repos: result.failed_repos,
        }
    }
}

/// TASK-29 fix wave (owner-reported: a task created via the Create modal
/// sometimes didn't appear on Board — reproduced as a stale-write race, not
/// a Board-specific rendering bug): applies a freshly-scanned
/// `HashMap<PathBuf, BacklogRepo>` onto the *existing* shared cache
/// per-entry, keeping whichever snapshot of each repo is actually newer,
/// rather than the caller doing a wholesale `*cache = fresh` swap or a
/// blind per-key overwrite (`HashMap::extend`, which is just as vulnerable
/// — "last write wins" regardless of which write is *older* data).
///
/// `collect_backlog_repos` scans every tracked root's disk state
/// sequentially — for a handful of real repos that's real, multi-repo wall
/// time, not an instant. `HiveApp::spawn_backlog_create` (app.rs, TASK-28)
/// does its own single-repo `refresh_backlog_repo_cache` insert
/// immediately after a create succeeds, so the periodic scan and a
/// mutation's targeted refresh can legitimately interleave: if this
/// worker's scan had *already read* a repo's pre-create state earlier in
/// its own loop, applying that stale snapshot after the mutation's fresher
/// one lands would silently revert it — the newly created task "vanishes"
/// until the next periodic cycle corrects it. Comparing each repo's own
/// `loaded_at_unix` (millisecond precision — see its doc, core/backlog.rs)
/// before overwriting closes the race outright rather than merely
/// shrinking its window: a scan's stale read of a repo can never
/// overwrite a genuinely newer one, whichever order the two locks happen
/// to land in.
///
/// Repo removal still works correctly: `roots` is this scan's authoritative
/// set of *currently tracked* repos, so any cache entry outside it (an
/// untracked repo) is dropped rather than lingering forever.
/// Re-derive `dispatch_runs` for every task carrying a dispatch label.
///
/// Lives on the backlog worker rather than the dispatch worker because
/// `spawn_dispatch` *blocks* for the entire length of a run — it could not
/// refresh anything while the thing worth watching is happening. This runs on
/// the backlog cadence instead, which is the same data's natural refresh rate.
///
/// The filesystem work is deliberately done outside the `backlog_repos`
/// lock: collecting the (root, id) pairs first keeps that mutex held for a map
/// walk rather than for a `read_dir` per dispatched task. Whether each task is
/// still *claimed* is collected in the same pass, because the sidecar sweep
/// below needs it and re-locking to ask would be a second walk.
fn refresh_dispatch_runs(ch: &Channels) {
    let targets: Vec<(PathBuf, String, bool)> = {
        let repos = ch.backlog_repos.lock().unwrap();
        repos
            .iter()
            .flat_map(|(root, repo)| {
                repo.tasks
                    .iter()
                    .filter(|task| task.labels.iter().any(|label| is_dispatch_label(label)))
                    .map(|task| {
                        let claimed = task.labels.iter().any(|label| label == DISPATCHING_LABEL);
                        (root.clone(), task.id.clone(), claimed)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    let runs = targets
        .into_iter()
        .map(|(root, task_id, claimed)| {
            let run = inspect_dispatch_run(&root, &task_id);
            sweep_sidecar_if_finished(&run, claimed);
            ((root, task_id), run)
        })
        .collect();
    *ch.dispatch_runs.lock().unwrap() = runs;
}

/// TASK-43: delete a pgid sidecar that can no longer name a live run — but
/// only once the task is no longer claimed.
///
/// A Switchbard force-quit mid-run never reaches `dispatch_one`'s release
/// boundary, so its sidecar outlives the run. `dispatch_inspect` already
/// refuses to arm a Kill button from one, so this is hygiene rather than the
/// safety mechanism — deleting a file never signals anything.
///
/// Two verdicts qualify, for the same reason from opposite directions:
///
/// - `Gone` — the group was positively identified as dead.
/// - `Unverifiable(StaleBoot)` — the sidecar was minted under a previous boot,
///   so its pgid names a number the kernel has since reissued. It can never
///   authenticate again, however long it sits there, and after a reboot this
///   is the whole surviving population (audit N2).
///
/// `LegacyFormat` is deliberately *not* swept: a pre-versioning sidecar
/// carries no boot epoch, which is exactly why it cannot be dated — sweeping
/// it would mean guessing that it is old, and the file is already inert.
///
/// The `!claimed` condition is the part worth reading twice. While a task is
/// still labelled `dispatching`, a verified-dead group is the **only**
/// evidence that the run died: the log is empty (which is also what a healthy
/// in-flight run looks like), so `looks_orphaned` cannot see it, and deleting
/// the sidecar would erase the one fact that lets the Dispatch view move that
/// row out of "In flight" and into the attention section. So the file is kept
/// exactly as long as it is load-bearing, and swept the moment the claim it
/// belongs to is gone. (Recovering a claimed-but-dead run is TASK-39's
/// reaper; this deliberately stops at not lying about it.)
fn sweep_sidecar_if_finished(run: &DispatchRun, claimed: bool) {
    if claimed || !sidecar_is_spent(run) {
        return;
    }
    if let Some(started) = run.started_at_unix {
        sweep_dead_sidecar(&run.task_id, started);
    }
}

/// A sidecar that can never name a live run again — see
/// [`sweep_sidecar_if_finished`] for why these two verdicts and no others.
fn sidecar_is_spent(run: &DispatchRun) -> bool {
    run.liveness.is_gone()
        || matches!(
            run.liveness.doubt(),
            Some(switchbard_core::dispatch_inspect::SidecarDoubt::StaleBoot)
        )
}

/// Any of the four labels that make up the dispatch state machine. A task
/// carrying none of them has never been flagged and needs no run lookup.
fn is_dispatch_label(label: &str) -> bool {
    matches!(
        label,
        DISPATCH_LABEL | DISPATCHING_LABEL | DISPATCHED_LABEL | DISPATCH_FAILED_LABEL
    )
}

pub(crate) fn merge_backlog_repos(
    cache: &mut HashMap<PathBuf, BacklogRepo>,
    roots: &[PathBuf],
    fresh: HashMap<PathBuf, BacklogRepo>,
) {
    cache.retain(|root, _| roots.contains(root));
    for (root, repo) in fresh {
        match cache.get(&root) {
            Some(existing) if existing.loaded_at_unix > repo.loaded_at_unix => {
                // A newer snapshot (e.g. a mutation's own targeted refresh)
                // is already cached — this scan's read of the same repo
                // was taken earlier and would revert it. Keep the newer one.
            }
            _ => {
                cache.insert(root, repo);
            }
        }
    }
}

fn spawn_backlog(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.backlog_kick.wait(initial_delay);
        loop {
            let repos = ch.repos.lock().unwrap().clone();
            let roots = backlog_repo_roots(&repos);
            let result = collect_backlog_repos(&roots);
            let read_state = {
                let mut cache = ch.backlog_repos.lock().unwrap();
                apply_tasks_read(&mut cache, &roots, result)
            };
            *ch.tasks_read_state.lock().unwrap() = read_state;
            refresh_dispatch_runs(&ch);

            // The unified triage overlay lives in whichever tracked repo hosts
            // `ordering.yml` (the "hub" repo — see backlog_triage module doc).
            // No tracked repo having one is the expected steady state and yields
            // an empty overlay with no warning.
            let hub_repo = find_hub_repo(repos.iter().map(|r| r.path.as_path()));
            let (overlay, warning) = match &hub_repo {
                Some(hub_root) => load_ordering_overlay(hub_root),
                None => Default::default(),
            };
            *ch.ordering.lock().unwrap() = OrderingState { overlay, warning };

            ctx.request_repaint();
            let focused = ctx.input(|i| i.focused);
            ch.backlog_kick
                .wait(effective_period(BACKLOG_PERIOD, focused));
        }
    });
}

/// Dispatch: for every tracked Backlog repo with at least one task
/// labeled `dispatch`, drain up to `DispatchOptions::default().max_concurrent`
/// of them (claim → worktree → headless `claude -p` → PR → notes — see
/// `switchbard_core::dispatch`'s module doc). Reads the already-cached
/// `backlog_repos` snapshot rather than reloading from disk — it's at
/// most `BACKLOG_PERIOD` stale, which is nothing next to how long a single
/// dispatch run itself takes. Skips a repo entirely when its queue is
/// empty, which is the common case: dispatch is opt-in, so most polls do
/// nothing.
///
/// TASK-46: `drain_dispatch_queue` no longer kills a run for taking too
/// long, so this loop's own `wait(effective_period(DISPATCH_PERIOD, ...))`
/// tick can be blocked, sometimes for a very long time, inside a single
/// `drain_dispatch_queue` call rather than at `DISPATCH_PERIOD`'s usual
/// cadence. That's fine: this thread has nothing else to publish while
/// blocked (see the module doc above), and the GUI stays responsive because
/// nothing else waits on this thread specifically.
fn spawn_dispatch(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    let opts = DispatchOptions::default();
    thread::spawn(move || {
        ch.dispatch_kick.wait(initial_delay);
        loop {
            // Iterate the (repo-primary-keyed) repos map directly: one drain
            // per repo. Iterating worktrees here would drain the same logical
            // queue once per sibling checkout — a real double-dispatch, since
            // each checkout carries its own copy of the task files.
            let repos = ch.backlog_repos.lock().unwrap().clone();
            for (root, repo) in &repos {
                if list_dispatch_queue(repo).is_empty() {
                    continue;
                }
                drain_dispatch_queue(root, repo, &opts);
                // The pipeline mutates task labels/notes straight through the
                // backlog CLI, bypassing this app's cache entirely — kick the
                // backlog worker so the GUI reflects the outcome immediately
                // instead of waiting up to BACKLOG_PERIOD for its own poll.
                ch.backlog_kick.notify();
                ctx.request_repaint();
            }
            let focused = ctx.input(|i| i.focused);
            ch.dispatch_kick
                .wait(effective_period(DISPATCH_PERIOD, focused));
        }
    });
}

/// Reaper: every REAPER_PERIOD, sweep `active_runs` for processes whose PGID
/// is gone (server crashed / killed externally) and drop them so the UI
/// returns to "idle" state for that row.
fn spawn_reaper(ctx: egui::Context, ch: Channels) {
    thread::spawn(move || loop {
        thread::sleep(REAPER_PERIOD);
        let dead: Vec<i32> = {
            let map = ch.active_runs.lock().unwrap();
            map.keys()
                .copied()
                .filter(|pgid| {
                    // SAFETY: `kill(-pgid, 0)` is the canonical "does this
                    // process group still exist?" probe. ESRCH ⇒ gone.
                    let rc = unsafe { libc::kill(-*pgid, 0) };
                    rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
                })
                .collect()
        };
        if !dead.is_empty() {
            let mut map = ch.active_runs.lock().unwrap();
            for pgid in &dead {
                map.remove(pgid);
            }
            drop(map);
            ctx.request_repaint();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use switchbard_core::BacklogTask;

    #[test]
    fn effective_period_passes_through_unchanged_when_focused() {
        assert_eq!(
            effective_period(Duration::from_secs(60), true),
            Duration::from_secs(60)
        );
    }

    fn run_with(liveness: switchbard_core::dispatch_inspect::DispatchRunLiveness) -> DispatchRun {
        DispatchRun {
            task_id: "TASK-1".to_string(),
            branch: "dispatch/task-1".to_string(),
            worktree_path: PathBuf::from("/repo/.worktrees/dispatch-task-1"),
            worktree_exists: false,
            log_path: None,
            prompt_path: None,
            started_at_unix: Some(1_700_000_000),
            log_bytes: 0,
            log_modified_unix: None,
            liveness,
            progress: switchbard_core::dispatch_inspect::RunProgress::default(),
        }
    }

    /// Audit N2: after a reboot, every sidecar left on disk is `StaleBoot` —
    /// permanently unauthenticatable litter. Deleting one signals nothing, so
    /// it is safe to sweep alongside a positively-dead group.
    #[test]
    fn a_spent_sidecar_is_one_that_can_never_name_a_live_run_again() {
        use switchbard_core::dispatch_inspect::{DispatchRunLiveness, SidecarDoubt};

        assert!(sidecar_is_spent(&run_with(DispatchRunLiveness::Gone)));
        assert!(sidecar_is_spent(&run_with(
            DispatchRunLiveness::Unverifiable(SidecarDoubt::StaleBoot)
        )));

        // A legacy sidecar carries no boot epoch, so it cannot be dated —
        // sweeping it would be guessing that it is old. It is already inert.
        assert!(!sidecar_is_spent(&run_with(
            DispatchRunLiveness::Unverifiable(SidecarDoubt::LegacyFormat)
        )));
        // A probe that failed this tick may well succeed next tick.
        assert!(!sidecar_is_spent(&run_with(
            DispatchRunLiveness::Unverifiable(SidecarDoubt::ProbeFailed)
        )));
        // And never the live ones.
        assert!(!sidecar_is_spent(&run_with(DispatchRunLiveness::Alive {
            pgid: 42,
            supervised: true
        })));
        assert!(!sidecar_is_spent(&run_with(DispatchRunLiveness::NoSidecar)));
    }

    #[test]
    fn effective_period_backs_off_when_unfocused() {
        assert_eq!(
            effective_period(Duration::from_secs(60), false),
            Duration::from_secs(60) * UNFOCUSED_BACKOFF_MULTIPLIER
        );
    }

    #[test]
    fn should_refresh_ignored_files_on_the_very_first_tick() {
        // Tick 0 must refresh — a freshly launched app shouldn't wait
        // `IGNORED_FILES_REFRESH_EVERY_N_PROBES` ticks for its first
        // ignored-files data.
        assert!(should_refresh_ignored_files(0));
    }

    #[test]
    fn should_refresh_ignored_files_only_every_nth_tick() {
        let refreshed: Vec<u64> = (0..IGNORED_FILES_REFRESH_EVERY_N_PROBES * 3)
            .filter(|&tick| should_refresh_ignored_files(tick))
            .collect();
        let expected: Vec<u64> = (0..3)
            .map(|n| n * IGNORED_FILES_REFRESH_EVERY_N_PROBES)
            .collect();
        assert_eq!(refreshed, expected);
    }

    #[test]
    fn stagger_offset_is_zero_for_the_first_worker() {
        // The scanner is UX-critical (SCAN_PERIOD doc) and must not be
        // delayed on startup.
        assert_eq!(stagger_offset(0), Duration::ZERO);
    }

    #[test]
    fn stagger_offset_increases_linearly_and_stays_distinct() {
        let offsets: Vec<Duration> = (0..6).map(stagger_offset).collect();
        for pair in offsets.windows(2) {
            assert!(
                pair[1] > pair[0],
                "each worker's stagger offset must exceed the previous one: {offsets:?}"
            );
        }
        assert_eq!(offsets[1] - offsets[0], WORKER_STAGGER_SPACING);
    }

    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git spawns");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Regression for the 2026-08-05 duplicate-rows defect: a repo with a
    /// linked worktree must yield exactly ONE backlog repo (the primary
    /// checkout), even though the linked worktree is itself a full Backlog
    /// repo on disk. Scanning per-worktree multiplied every task by the
    /// repo's worktree count (42x for budget) in the unified lenses and gave
    /// the dispatch worker one drainable queue per checkout.
    #[test]
    fn linked_worktrees_do_not_duplicate_backlog_projects() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let primary = tmp.path().join("repo");
        fs::create_dir_all(primary.join("backlog").join("tasks")).expect("mkdir");
        fs::write(
            primary.join("backlog").join("config.yml"),
            "projectName: fixture\n",
        )
        .expect("config.yml");
        fs::write(
            primary
                .join("backlog")
                .join("tasks")
                .join("task-1 - Fixture.md"),
            "---\nid: task-1\ntitle: Fixture\nstatus: To Do\n---\n\n## Description\n\nfixture\n",
        )
        .expect("task file");
        git(&primary, &["init", "-q", "-b", "main"]);
        git(&primary, &["add", "-A"]);
        git(
            &primary,
            &[
                "-c",
                "user.email=fixture@test",
                "-c",
                "user.name=fixture",
                "commit",
                "-qm",
                "init",
            ],
        );
        let linked = tmp.path().join("linked");
        git(
            &primary,
            &["worktree", "add", "-q", linked.to_str().expect("utf8 path")],
        );
        // Sanity: the linked worktree really is a Backlog repo on disk —
        // the exact condition that used to duplicate every task.
        assert!(is_backlog_repo(&linked));

        let repos = vec![Repo {
            name: "fixture".to_string(),
            path: primary.clone(),
        }];
        let roots = backlog_repo_roots(&repos);
        assert_eq!(
            roots,
            vec![primary.clone()],
            "one root per repo, primary only"
        );
        let result = collect_backlog_repos(&roots);
        assert_eq!(
            result.repos.len(),
            1,
            "one repo despite the linked worktree"
        );
        assert!(result.repos.contains_key(&primary));
        assert_eq!(result.repos[&primary].tasks.len(), 1);
        assert_eq!(result.failed_repos, 0);
    }

    /// `task_titles` stands in for "what this snapshot of the repo
    /// looked like" — the merge tests below only care about which
    /// snapshot (stale vs. fresh) survives, not task content specifics.
    fn fixture_project(root: &Path, loaded_at_unix: u64, task_titles: &[&str]) -> BacklogRepo {
        BacklogRepo {
            root: root.to_path_buf(),
            tasks: task_titles
                .iter()
                .enumerate()
                .map(|(i, title)| BacklogTask {
                    id: format!("TASK-{}", i + 1),
                    title: title.to_string(),
                    status: "To Do".to_string(),
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
                    acceptance_criteria: vec![],
                    definition_of_done: vec![],
                    source: switchbard_core::BacklogTaskSource::Active,
                    path: root.join("backlog/tasks/fixture.md"),
                })
                .collect(),
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: switchbard_core::RepoRanking::default(),
            loaded_at_unix,
            configured_statuses: vec![],
        }
    }

    /// TASK-29 fix wave: the exact race the owner reported. A periodic
    /// scan's own read of a repo (taken *before* a task was created,
    /// hence no `"New task"` in its task list, hence an *older*
    /// `loaded_at_unix`) must not overwrite a mutation's fresher targeted
    /// refresh of the same repo once both land in the shared cache —
    /// regardless of which of the two `merge_backlog_repos` calls
    /// happens to run second. Before this fix, a plain `HashMap::extend`
    /// (or a wholesale `*cache = fresh` replace) would have reverted the
    /// cache to the stale, task-less snapshot here.
    #[test]
    fn merge_keeps_a_newer_cached_snapshot_over_a_stale_scan_result() {
        let root = PathBuf::from("/fixture/repo");
        let mut cache = HashMap::new();
        // The mutation's own refresh_backlog_repo_cache-style insert
        // landed first, with the new task and a later timestamp.
        cache.insert(
            root.clone(),
            fixture_project(&root, 200, &["Existing task", "New task"]),
        );

        // The periodic worker's own scan started earlier (lower
        // timestamp) and never saw the new task, but its results only
        // reach the shared cache now.
        let mut stale_scan = HashMap::new();
        stale_scan.insert(
            root.clone(),
            fixture_project(&root, 100, &["Existing task"]),
        );

        merge_backlog_repos(&mut cache, std::slice::from_ref(&root), stale_scan);

        assert_eq!(
            cache[&root].tasks.len(),
            2,
            "the newer cached snapshot (with the new task) must survive a stale scan's merge"
        );
        assert_eq!(cache[&root].loaded_at_unix, 200);
    }

    /// The normal, non-racing case: a scan whose own timestamp is newer
    /// than what's cached should still update it — the fix must not make
    /// the cache "stuck" on old data once a genuinely fresher scan lands.
    #[test]
    fn merge_applies_a_genuinely_newer_scan_result() {
        let root = PathBuf::from("/fixture/repo");
        let mut cache = HashMap::new();
        cache.insert(
            root.clone(),
            fixture_project(&root, 100, &["Existing task"]),
        );

        let mut newer_scan = HashMap::new();
        newer_scan.insert(
            root.clone(),
            fixture_project(&root, 200, &["Existing task", "Another new task"]),
        );

        merge_backlog_repos(&mut cache, std::slice::from_ref(&root), newer_scan);

        assert_eq!(cache[&root].tasks.len(), 2);
        assert_eq!(cache[&root].loaded_at_unix, 200);
    }

    /// A repo that's no longer tracked (removed from `roots`) should drop
    /// out of the cache, not linger forever — the merge isn't allowed to
    /// trade "never clobber a fresher write" for "never remove anything."
    #[test]
    fn merge_drops_cache_entries_for_repos_no_longer_tracked() {
        let tracked = PathBuf::from("/fixture/tracked");
        let removed = PathBuf::from("/fixture/removed");
        let mut cache = HashMap::new();
        cache.insert(tracked.clone(), fixture_project(&tracked, 100, &[]));
        cache.insert(removed.clone(), fixture_project(&removed, 100, &[]));

        let mut fresh = HashMap::new();
        fresh.insert(tracked.clone(), fixture_project(&tracked, 200, &[]));
        // `removed` is absent from both `roots` and this scan's own
        // results — it was untracked before this cycle ran.

        merge_backlog_repos(&mut cache, std::slice::from_ref(&tracked), fresh);

        assert!(cache.contains_key(&tracked));
        assert!(
            !cache.contains_key(&removed),
            "an untracked repo's stale entry should be dropped, not linger"
        );
    }

    #[test]
    fn failed_task_read_keeps_last_known_rows_and_marks_model_stale() {
        let root = PathBuf::from("/fixture/repo");
        let mut cache = HashMap::new();
        cache.insert(
            root.clone(),
            fixture_project(&root, 100, &["Last-known task"]),
        );

        let state = apply_tasks_read(
            &mut cache,
            std::slice::from_ref(&root),
            TasksReadResult {
                repos: HashMap::new(),
                failed_repos: 1,
            },
        );

        assert_eq!(state, TasksReadState::Stale { failed_repos: 1 });
        assert_eq!(cache[&root].tasks[0].title, "Last-known task");
    }

    // ── landing-stage worker (feat/landing-stage) ───────────────────────

    fn wt(path: &str, branch: Option<&str>) -> WorktreeRef {
        WorktreeRef {
            repo_name: "demo".to_string(),
            path: PathBuf::from(path),
            branch: branch.map(str::to_string),
            head: "aaaa1111".to_string(),
        }
    }

    fn meta_with_unlanded(unlanded: u32) -> WorktreeMeta {
        WorktreeMeta {
            trunk: Some(switchbard_core::TrunkDivergence {
                base: "origin/main".to_string(),
                unlanded,
                ancestry_ahead: unlanded,
                behind: 0,
            }),
            ..Default::default()
        }
    }

    /// The two hard constraints this worker's whole candidate set rests on:
    /// a branch to push from, and something unlanded to explain.
    #[test]
    fn candidates_need_both_a_branch_and_unlanded_work() {
        let worktrees = vec![
            wt("/repo/a", Some("feat/a")), // unlanded — a real candidate
            wt("/repo/b", Some("feat/b")), // clean — nothing to ask about
            wt("/repo/c", None),           // detached HEAD — nothing to push
            wt("/repo/d", Some("feat/d")), // never probed (no meta entry at all)
        ];
        let mut meta = HashMap::new();
        meta.insert(PathBuf::from("/repo/a"), meta_with_unlanded(3));
        meta.insert(PathBuf::from("/repo/b"), meta_with_unlanded(0));
        meta.insert(PathBuf::from("/repo/c"), meta_with_unlanded(5));
        // /repo/d intentionally absent from `meta`.

        let candidates = landing_candidates(&worktrees, &meta);
        assert_eq!(
            candidates,
            vec![(PathBuf::from("/repo/a"), "feat/a".to_string(), 3)]
        );
    }

    /// `computed_at` is taken directly (never derived by subtracting from
    /// `Instant::now()`) so these fixtures can never underflow `Instant` on
    /// a freshly booted CI runner — every "how stale" comparison below is
    /// built by *adding* to a fixed base instant instead.
    fn cache_entry_at(stage: LandingStage, computed_at: Instant) -> LandingEntry {
        LandingEntry { stage, computed_at }
    }

    /// A never-cached candidate is "missing" and enters this tick's batch —
    /// the worker's whole job for a cold worktree.
    #[test]
    fn an_uncached_candidate_is_missing_and_batched() {
        let candidates = vec![(PathBuf::from("/repo/a"), "feat/a".to_string(), 3)];
        let cached = HashMap::new();
        let result = partition_landing_batch(&candidates, &cached, Instant::now());
        assert_eq!(result.batch, candidates);
        assert!(!result.more_missing);
        assert!(
            result.stale.is_none(),
            "nothing cached, so nothing can be stale"
        );
    }

    /// Cold-start capping: more never-cached candidates than
    /// `LANDING_MAX_MISSING_PER_TICK` still only batches the cap's worth this
    /// tick, and reports `more_missing` so the worker loops again without
    /// sleeping instead of stalling on one giant tick.
    #[test]
    fn missing_candidates_beyond_the_cap_carry_over_to_the_next_tick() {
        let candidates: Vec<LandingCandidate> = (0..LANDING_MAX_MISSING_PER_TICK + 3)
            .map(|i| (PathBuf::from(format!("/repo/{i}")), "feat/x".to_string(), 1))
            .collect();
        let cached = HashMap::new();
        let result = partition_landing_batch(&candidates, &cached, Instant::now());
        assert_eq!(result.batch.len(), LANDING_MAX_MISSING_PER_TICK);
        assert!(result.more_missing);
    }

    /// A cached-and-fresh candidate is neither missing nor stale — the
    /// steady state where the worker has nothing to do for it this tick.
    #[test]
    fn a_fresh_cache_entry_is_neither_missing_nor_stale() {
        let now = Instant::now();
        let path = PathBuf::from("/repo/a");
        let candidates = vec![(path.clone(), "feat/a".to_string(), 3)];
        let mut cached = HashMap::new();
        cached.insert(path, cache_entry_at(LandingStage::Unpushed, now));
        let result = partition_landing_batch(&candidates, &cached, now);
        assert!(result.batch.is_empty());
        assert!(!result.more_missing);
        assert!(result.stale.is_none());
    }

    /// Once nothing is missing, the single *stalest* cached candidate — not
    /// an arbitrary one — is offered for the steady-state refresh. Both
    /// entries are made stale relative to `now` by advancing `now` forward
    /// from a fixed base instant (never by subtracting from `Instant::now()`
    /// — see `cache_entry_at`'s doc), with the "stale" entry computed
    /// earlier than the "fresher" one.
    #[test]
    fn the_stalest_cached_candidate_wins_the_steady_state_refresh() {
        let t0 = Instant::now();
        let stale_path = PathBuf::from("/repo/stale");
        let fresher_path = PathBuf::from("/repo/fresher");
        let candidates = vec![
            (stale_path.clone(), "feat/stale".to_string(), 1),
            (fresher_path.clone(), "feat/fresher".to_string(), 1),
        ];
        let mut cached = HashMap::new();
        cached.insert(
            stale_path.clone(),
            cache_entry_at(LandingStage::Unpushed, t0),
        );
        cached.insert(
            fresher_path,
            cache_entry_at(LandingStage::Unpushed, t0 + Duration::from_secs(5)),
        );
        let now = t0 + LANDING_CACHE_MAX_AGE * 3;

        let result = partition_landing_batch(&candidates, &cached, now);
        assert!(result.batch.is_empty());
        assert!(!result.more_missing);
        assert_eq!(result.stale.map(|(p, ..)| p), Some(stale_path));
    }
}
