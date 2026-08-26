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
//! | — staleness sub-probe (TASK-41) | every probe tick (no decoupling needed) | ~2.0s isolated over 84 worktrees (~24ms each, measured 2026-08-19 via `examples/scan_cadence_audit.rs`) | Same cost class as `probe_main_drift`/`probe_remote_drift` (2-3 git subprocesses each) — cheap enough to compute alongside `DriftProbe` on every tick, unlike size below. |
//! | detection | 60s (was 30s) | ~0.15s cold, ~0 steady-state (idempotent — skips worktrees already in `services`) | No urgency: a newly tracked worktree still gets detected within a minute. |
//! | agent-context | 60s (was 30s), capped at `AGENT_CONTEXT_MAX_MISSING_PER_TICK` new worktrees per tick | ~47s in one unbroken burst pre-fix (cold scan of all 84 at once) | Recursive per-worktree filesystem walk; cheap in steady state (only rescans missing/>24h-stale entries) but a cold launch or adding several repos at once used to stall the thread for tens of seconds in a single tick. Capping the batch turns that into several bounded, interleaved ticks instead. |
//! | backlog | 30s (unchanged) | ~0.15-0.2s over 6 repo *roots*, not per-worktree | Already cheap at this scale (one load per tracked repo, not per worktree) and users watch task state change in near-real-time — no evidence to slow this down. |
//! | dispatch | 90s (unchanged) | negligible when the queue is empty (the common case); unbounded while a run is in flight (TASK-46 removed the wall-clock kill — see `spawn_dispatch`'s own doc) | Opt-in and rare by design — see its own doc. Unaffected by worktree count. |
//! | size (TASK-41) | 300s, bounded catch-up batch of 5 | ~650ms **per worktree** average (measured 2026-08-19 via `examples/scan_cadence_audit.rs`, sampled 20/84 real worktrees, `du -sk`; a manual sweep of `~/Dev/.worktrees`'s larger checkouts saw individual calls up to ~1.5s) — an order of magnitude past every other per-worktree probe | `du` walks the whole tree (node_modules/target/build artifacts); see `worktree_size.rs`'s own doc. Never runs inline with the git-probe tick — its own worker, own cadence, catches up a bounded batch of never-yet-sized worktrees per tick (same shape as agent-context's cold-start batching below) rather than blocking on a full sweep. |
//! | reaper | 2s (unchanged) | negligible, in-memory PGID check only | Not part of the worktree-count scaling problem this audit targets. |
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

use crate::runtime::worktrees::expand_worktrees;
use crate::runtime::{
    is_retired_worktree, ActiveRun, BacklogTaskKey, FileListSummary, OrderingState, WorktreeMeta,
    WorktreeSizeEntry,
};
use crate::sync::Kick;
use eframe::egui;
use switchbard_core::dispatch_inspect::{inspect_dispatch_run, DispatchRun};
use switchbard_core::{
    agent_context_needs_rescan, attribute, detect_services, drain_dispatch_queue, find_hub_repo,
    is_backlog_project, list_dispatch_queue, load_backlog_project, load_ordering_overlay,
    probe_dirty_files, probe_fetch_age, probe_head_commit_time, probe_ignored_files,
    probe_main_drift, probe_recent_commits, probe_ref_drift_detail, probe_remote_drift,
    probe_worktree_lock, probe_worktree_size, probe_worktree_staleness, save_agent_context_cache,
    scan_agent_context, scan_listeners, sweep_dead_sidecar, AgentContextMap, BacklogProject,
    DetectedService, DispatchOptions, DriftProbe, Fact, Repo, WorktreeRef, DISPATCHED_LABEL,
    DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
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
const CONTEXT_CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24);
const REAPER_PERIOD: Duration = Duration::from_secs(2);
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
    pub backlog_projects: Arc<Mutex<HashMap<PathBuf, BacklogProject>>>,
    pub dispatch_runs: Arc<Mutex<HashMap<BacklogTaskKey, DispatchRun>>>,
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
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
    pub agent_context_kick: Kick,
    pub backlog_kick: Kick,
    pub dispatch_kick: Kick,
    pub size_kick: Kick,
}

pub fn spawn_all(ctx: egui::Context, ch: Channels) {
    spawn_scanner(ctx.clone(), ch.clone(), stagger_offset(0));
    spawn_probe(ctx.clone(), ch.clone(), stagger_offset(1));
    spawn_detection(ctx.clone(), ch.clone(), stagger_offset(2));
    spawn_agent_context(ctx.clone(), ch.clone(), stagger_offset(3));
    spawn_backlog(ctx.clone(), ch.clone(), stagger_offset(4));
    spawn_dispatch(ctx.clone(), ch.clone(), stagger_offset(5));
    spawn_size(ctx.clone(), ch.clone(), stagger_offset(6));
    spawn_reaper(ctx, ch);
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
            let refresh_ignored = should_refresh_ignored_files(tick);
            // TASK-41: accumulated alongside the per-worktree probe loop
            // rather than in a second pass over `wts` — see
            // `retired_worktree_count`'s own doc for why this is cached at
            // all instead of recomputed per-frame by the top bar.
            let mut retired = 0usize;
            for w in &wts {
                let main_drift = probe_main_drift(&w.path);
                let remote_drift = probe_remote_drift(&w.path);
                let main_drift_detail = drift_detail_for_probe(&w.path, main_drift.as_ref());
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
                let staleness =
                    repo_path.map(|repo_path| probe_worktree_staleness(repo_path, &w.path));
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
                    main_drift,
                    remote_drift,
                    main_drift_detail,
                    remote_drift_detail,
                    head_commit_unix: probe_head_commit_time(&w.path),
                    fetch_unix: probe_fetch_age(&w.path),
                    recent_commits: probe_recent_commits(&w.path, RECENT_COMMITS_LIMIT),
                    probed_at: Some(Instant::now()),
                    staleness,
                    lock,
                };
                if is_retired_worktree(w, &repos, Some(&m)) {
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
pub(crate) fn backlog_project_roots(repos: &[Repo]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    repos
        .iter()
        .map(|r| r.path.clone())
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

/// Load every root that actually is a Backlog project. Split from the worker
/// loop so the root-set semantics above are testable without threads.
pub(crate) fn collect_backlog_projects(roots: &[PathBuf]) -> HashMap<PathBuf, BacklogProject> {
    let mut projects = HashMap::new();
    for root in roots {
        if !is_backlog_project(root) {
            continue;
        }
        if let Ok(project) = load_backlog_project(root) {
            projects.insert(root.clone(), project);
        }
    }
    projects
}

/// TASK-29 fix wave (owner-reported: a task created via the Create modal
/// sometimes didn't appear on Board — reproduced as a stale-write race, not
/// a Board-specific rendering bug): applies a freshly-scanned
/// `HashMap<PathBuf, BacklogProject>` onto the *existing* shared cache
/// per-entry, keeping whichever snapshot of each project is actually newer,
/// rather than the caller doing a wholesale `*cache = fresh` swap or a
/// blind per-key overwrite (`HashMap::extend`, which is just as vulnerable
/// — "last write wins" regardless of which write is *older* data).
///
/// `collect_backlog_projects` scans every tracked root's disk state
/// sequentially — for a handful of real repos that's real, multi-repo wall
/// time, not an instant. `HiveApp::spawn_backlog_create` (app.rs, TASK-28)
/// does its own single-project `refresh_backlog_project_cache` insert
/// immediately after a create succeeds, so the periodic scan and a
/// mutation's targeted refresh can legitimately interleave: if this
/// worker's scan had *already read* a repo's pre-create state earlier in
/// its own loop, applying that stale snapshot after the mutation's fresher
/// one lands would silently revert it — the newly created task "vanishes"
/// until the next periodic cycle corrects it. Comparing each project's own
/// `loaded_at_unix` (millisecond precision — see its doc, core/backlog.rs)
/// before overwriting closes the race outright rather than merely
/// shrinking its window: a scan's stale read of a project can never
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
/// The filesystem work is deliberately done outside the `backlog_projects`
/// lock: collecting the (root, id) pairs first keeps that mutex held for a map
/// walk rather than for a `read_dir` per dispatched task. Whether each task is
/// still *claimed* is collected in the same pass, because the sidecar sweep
/// below needs it and re-locking to ask would be a second walk.
fn refresh_dispatch_runs(ch: &Channels) {
    let targets: Vec<(PathBuf, String, bool)> = {
        let projects = ch.backlog_projects.lock().unwrap();
        projects
            .iter()
            .flat_map(|(root, project)| {
                project
                    .tasks
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

pub(crate) fn merge_backlog_projects(
    cache: &mut HashMap<PathBuf, BacklogProject>,
    roots: &[PathBuf],
    fresh: HashMap<PathBuf, BacklogProject>,
) {
    cache.retain(|root, _| roots.contains(root));
    for (root, project) in fresh {
        match cache.get(&root) {
            Some(existing) if existing.loaded_at_unix > project.loaded_at_unix => {
                // A newer snapshot (e.g. a mutation's own targeted refresh)
                // is already cached — this scan's read of the same project
                // was taken earlier and would revert it. Keep the newer one.
            }
            _ => {
                cache.insert(root, project);
            }
        }
    }
}

fn spawn_backlog(ctx: egui::Context, ch: Channels, initial_delay: Duration) {
    thread::spawn(move || {
        ch.backlog_kick.wait(initial_delay);
        loop {
            let repos = ch.repos.lock().unwrap().clone();
            let roots = backlog_project_roots(&repos);
            let projects = collect_backlog_projects(&roots);
            merge_backlog_projects(&mut ch.backlog_projects.lock().unwrap(), &roots, projects);
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

/// Dispatch: for every tracked Backlog project with at least one task
/// labeled `dispatch`, drain up to `DispatchOptions::default().max_concurrent`
/// of them (claim → worktree → headless `claude -p` → PR → notes — see
/// `switchbard_core::dispatch`'s module doc). Reads the already-cached
/// `backlog_projects` snapshot rather than reloading from disk — it's at
/// most `BACKLOG_PERIOD` stale, which is nothing next to how long a single
/// dispatch run itself takes. Skips a project entirely when its queue is
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
            // Iterate the (repo-primary-keyed) projects map directly: one drain
            // per repo. Iterating worktrees here would drain the same logical
            // queue once per sibling checkout — a real double-dispatch, since
            // each checkout carries its own copy of the task files.
            let projects = ch.backlog_projects.lock().unwrap().clone();
            for (root, project) in &projects {
                if list_dispatch_queue(project).is_empty() {
                    continue;
                }
                drain_dispatch_queue(root, project, &opts);
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
    /// linked worktree must yield exactly ONE backlog project (the primary
    /// checkout), even though the linked worktree is itself a full Backlog
    /// project on disk. Scanning per-worktree multiplied every task by the
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
        // Sanity: the linked worktree really is a Backlog project on disk —
        // the exact condition that used to duplicate every task.
        assert!(is_backlog_project(&linked));

        let repos = vec![Repo {
            name: "fixture".to_string(),
            path: primary.clone(),
        }];
        let roots = backlog_project_roots(&repos);
        assert_eq!(
            roots,
            vec![primary.clone()],
            "one root per repo, primary only"
        );
        let projects = collect_backlog_projects(&roots);
        assert_eq!(projects.len(), 1, "one project despite the linked worktree");
        assert!(projects.contains_key(&primary));
        assert_eq!(projects[&primary].tasks.len(), 1);
    }

    /// `task_titles` stands in for "what this snapshot of the project
    /// looked like" — the merge tests below only care about which
    /// snapshot (stale vs. fresh) survives, not task content specifics.
    fn fixture_project(root: &Path, loaded_at_unix: u64, task_titles: &[&str]) -> BacklogProject {
        BacklogProject {
            root: root.to_path_buf(),
            cli_path: None,
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
                    milestone: None,
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
            loaded_at_unix,
            configured_statuses: vec![],
        }
    }

    /// TASK-29 fix wave: the exact race the owner reported. A periodic
    /// scan's own read of a project (taken *before* a task was created,
    /// hence no `"New task"` in its task list, hence an *older*
    /// `loaded_at_unix`) must not overwrite a mutation's fresher targeted
    /// refresh of the same project once both land in the shared cache —
    /// regardless of which of the two `merge_backlog_projects` calls
    /// happens to run second. Before this fix, a plain `HashMap::extend`
    /// (or a wholesale `*cache = fresh` replace) would have reverted the
    /// cache to the stale, task-less snapshot here.
    #[test]
    fn merge_keeps_a_newer_cached_snapshot_over_a_stale_scan_result() {
        let root = PathBuf::from("/fixture/repo");
        let mut cache = HashMap::new();
        // The mutation's own refresh_backlog_project_cache-style insert
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

        merge_backlog_projects(&mut cache, std::slice::from_ref(&root), stale_scan);

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

        merge_backlog_projects(&mut cache, std::slice::from_ref(&root), newer_scan);

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

        merge_backlog_projects(&mut cache, std::slice::from_ref(&tracked), fresh);

        assert!(cache.contains_key(&tracked));
        assert!(
            !cache.contains_key(&removed),
            "an untracked repo's stale entry should be dropped, not linger"
        );
    }
}
