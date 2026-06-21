//! Background threads that feed the GUI.
//!
//! All four workers follow the same shape:
//!   1. Take a snapshot of whatever inputs they need (under a brief lock).
//!   2. Do work outside any lock.
//!   3. Write results back into the shared `Mutex`, then `ctx.request_repaint()`.
//!   4. Sleep via `kick.wait(period)`.
//!
//! Centralizing the spawning here keeps `HiveApp::new` short and stops the
//! "what does this anonymous closure do?" question from recurring.

use crate::runtime::worktrees::expand_worktrees;
use crate::runtime::{ActiveRun, FileListSummary, WorktreeMeta};
use crate::sync::Kick;
use eframe::egui;
use switchbard_core::{
    agent_context_needs_rescan, attribute, detect_services, is_backlog_project,
    load_backlog_project, probe_dirty_files, probe_fetch_age, probe_head_commit_time,
    probe_ignored_files, probe_main_drift, probe_recent_commits, probe_ref_drift_detail,
    probe_remote_drift, save_agent_context_cache, scan_agent_context, scan_listeners,
    AgentContextMap, BacklogProject, DetectedService, DriftProbe, Repo, WorktreeRef,
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
const PROBE_PERIOD: Duration = Duration::from_secs(60);
const DETECT_PERIOD: Duration = Duration::from_secs(30);
const CONTEXT_PERIOD: Duration = Duration::from_secs(30);
const BACKLOG_PERIOD: Duration = Duration::from_secs(30);
const CONTEXT_CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24);
const REAPER_PERIOD: Duration = Duration::from_secs(2);

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
    pub active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
    pub agent_context_kick: Kick,
    pub backlog_kick: Kick,
}

pub fn spawn_all(ctx: egui::Context, ch: Channels) {
    spawn_scanner(ctx.clone(), ch.clone());
    spawn_probe(ctx.clone(), ch.clone());
    spawn_detection(ctx.clone(), ch.clone());
    spawn_agent_context(ctx.clone(), ch.clone());
    spawn_backlog(ctx.clone(), ch.clone());
    spawn_reaper(ctx, ch);
}

/// Scanner: re-runs `lsof` every SCAN_PERIOD (or sooner if kicked), attributes
/// each listener to a worktree, publishes the result to `state.listeners`.
fn spawn_scanner(ctx: egui::Context, ch: Channels) {
    thread::spawn(move || loop {
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
        ch.scanner_kick.wait(SCAN_PERIOD);
    });
}

/// Git probe: each iteration re-enumerates worktrees from `git worktree list`
/// (so external `git worktree prune` / `add` get picked up), then walks the
/// fresh list running dirty/ahead/behind/last-commit probes.
fn spawn_probe(ctx: egui::Context, ch: Channels) {
    thread::spawn(move || loop {
        // Step 1: re-enumerate worktrees from disk and publish.
        {
            let repos = ch.repos.lock().unwrap().clone();
            let fresh = expand_worktrees(&repos);
            *ch.worktrees.lock().unwrap() = fresh;
            ctx.request_repaint();
        }
        // Step 2: probe each.
        let wts = ch.worktrees.lock().unwrap().clone();
        for w in &wts {
            let main_drift = probe_main_drift(&w.path);
            let remote_drift = probe_remote_drift(&w.path);
            let main_drift_detail = drift_detail_for_probe(&w.path, main_drift.as_ref());
            let remote_drift_detail = drift_detail_for_probe(&w.path, remote_drift.as_ref());
            let m = WorktreeMeta {
                dirty_files: probe_dirty_files(&w.path),
                ignored_files: probe_ignored_files(&w.path)
                    .map(|files| FileListSummary::from_lines(files, IGNORED_FILES_PREVIEW_LIMIT)),
                main_drift,
                remote_drift,
                main_drift_detail,
                remote_drift_detail,
                head_commit_unix: probe_head_commit_time(&w.path),
                fetch_unix: probe_fetch_age(&w.path),
                recent_commits: probe_recent_commits(&w.path, RECENT_COMMITS_LIMIT),
                probed_at: Some(Instant::now()),
            };
            ch.meta.lock().unwrap().insert(w.path.clone(), m);
            ctx.request_repaint();
        }
        ch.probe_kick.wait(PROBE_PERIOD);
    });
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
fn spawn_detection(ctx: egui::Context, ch: Channels) {
    thread::spawn(move || loop {
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
        ch.detection_kick.wait(DETECT_PERIOD);
    });
}

fn spawn_agent_context(ctx: egui::Context, ch: Channels) {
    thread::spawn(move || loop {
        let wts = ch.worktrees.lock().unwrap().clone();
        let live_paths: std::collections::HashSet<PathBuf> =
            wts.iter().map(|w| w.path.clone()).collect();

        let (missing, stale, pruned) = {
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
            (missing, stale, maps.len() != before)
        };

        let mut refreshed = false;
        if missing.is_empty() {
            if let Some(w) = stale {
                scan_and_publish_agent_context(&ch, &w);
                refreshed = true;
            }
        } else {
            for w in &missing {
                scan_and_publish_agent_context(&ch, w);
            }
            refreshed = true;
        }

        if refreshed || pruned {
            persist_agent_context_cache(&ch);
            ctx.request_repaint();
        }
        ch.agent_context_kick.wait(CONTEXT_PERIOD);
    });
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

fn spawn_backlog(ctx: egui::Context, ch: Channels) {
    thread::spawn(move || loop {
        let wts = ch.worktrees.lock().unwrap().clone();
        let live_paths: std::collections::HashSet<PathBuf> =
            wts.iter().map(|w| w.path.clone()).collect();
        let mut projects = HashMap::new();
        for w in &wts {
            if !is_backlog_project(&w.path) {
                continue;
            }
            if let Ok(project) = load_backlog_project(&w.path) {
                projects.insert(w.path.clone(), project);
            }
        }
        projects.retain(|path, _| live_paths.contains(path));
        *ch.backlog_projects.lock().unwrap() = projects;
        ctx.request_repaint();
        ch.backlog_kick.wait(BACKLOG_PERIOD);
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
