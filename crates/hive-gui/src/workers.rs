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
use crate::runtime::{ActiveRun, WorktreeMeta};
use crate::sync::Kick;
use eframe::egui;
use hive_core::{
    attribute, detect_services, probe_ahead_behind, probe_dirty, probe_head_commit_time,
    scan_listeners, DetectedService, Repo, WorktreeRef,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::app::ScanState;

const SCAN_PERIOD: Duration = Duration::from_secs(3);
const PROBE_PERIOD: Duration = Duration::from_secs(60);
const DETECT_PERIOD: Duration = Duration::from_secs(30);
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
    pub active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
}

pub fn spawn_all(ctx: egui::Context, ch: Channels) {
    spawn_scanner(ctx.clone(), ch.clone());
    spawn_probe(ctx.clone(), ch.clone());
    spawn_detection(ctx.clone(), ch.clone());
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
            let (ahead, behind) = probe_ahead_behind(&w.path)
                .map(|(a, b)| (Some(a), Some(b)))
                .unwrap_or((None, None));
            let m = WorktreeMeta {
                dirty: probe_dirty(&w.path),
                ahead,
                behind,
                head_commit_unix: probe_head_commit_time(&w.path),
                probed_at: Some(Instant::now()),
            };
            ch.meta.lock().unwrap().insert(w.path.clone(), m);
            ctx.request_repaint();
        }
        ch.probe_kick.wait(PROBE_PERIOD);
    });
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
