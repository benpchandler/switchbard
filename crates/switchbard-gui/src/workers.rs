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
//! and notes change. See its own doc for why one iteration can block far
//! longer than the other workers' — this reuses `drain_dispatch_queue`'s
//! serial-by-design batching rather than reimplementing it.
//!
//! Centralizing the spawning here keeps `HiveApp::new` short and stops the
//! "what does this anonymous closure do?" question from recurring.

use crate::runtime::worktrees::expand_worktrees;
use crate::runtime::{ActiveRun, FileListSummary, OrderingState, WorktreeMeta};
use crate::sync::Kick;
use eframe::egui;
use switchbard_core::{
    agent_context_needs_rescan, attribute, detect_services, drain_dispatch_queue, find_hub_repo,
    is_backlog_project, list_dispatch_queue, load_backlog_project, load_ordering_overlay,
    probe_dirty_files, probe_fetch_age, probe_head_commit_time, probe_ignored_files,
    probe_main_drift, probe_recent_commits, probe_ref_drift_detail, probe_remote_drift,
    save_agent_context_cache, scan_agent_context, scan_listeners, AgentContextMap, BacklogProject,
    DetectedService, DispatchOptions, DriftProbe, Repo, WorktreeRef,
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
/// Longer than the other periods on purpose: dispatch is opt-in and rare
/// (a task only enters the queue via an explicit user action), and one
/// iteration can itself take many minutes (a full headless `claude -p` run
/// per queued task) — a short poll period would just mean more overlapping
/// wakeups against a worker that's usually idle-checking an empty queue.
const DISPATCH_PERIOD: Duration = Duration::from_secs(90);
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
    pub ordering: Arc<Mutex<OrderingState>>,
    pub active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
    pub agent_context_kick: Kick,
    pub backlog_kick: Kick,
    pub dispatch_kick: Kick,
}

pub fn spawn_all(ctx: egui::Context, ch: Channels) {
    spawn_scanner(ctx.clone(), ch.clone());
    spawn_probe(ctx.clone(), ch.clone());
    spawn_detection(ctx.clone(), ch.clone());
    spawn_agent_context(ctx.clone(), ch.clone());
    spawn_backlog(ctx.clone(), ch.clone());
    spawn_dispatch(ctx.clone(), ch.clone());
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

fn spawn_backlog(ctx: egui::Context, ch: Channels) {
    thread::spawn(move || loop {
        let repos = ch.repos.lock().unwrap().clone();
        let roots = backlog_project_roots(&repos);
        let projects = collect_backlog_projects(&roots);
        merge_backlog_projects(&mut ch.backlog_projects.lock().unwrap(), &roots, projects);

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
        ch.backlog_kick.wait(BACKLOG_PERIOD);
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
fn spawn_dispatch(ctx: egui::Context, ch: Channels) {
    let opts = DispatchOptions::default();
    thread::spawn(move || loop {
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
        ch.dispatch_kick.wait(DISPATCH_PERIOD);
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
