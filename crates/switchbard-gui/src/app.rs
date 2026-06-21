//! `HiveApp` — the eframe::App. Owns shared state, hosts user actions,
//! delegates rendering to the `views` module.
//!
//! Design rules in here:
//! - Anything a worker thread needs goes in an `Arc<Mutex<>>` field.
//! - Anything purely view-state (filters, expansion toggles, view tab) is
//!   owned directly by the struct.
//! - The persisted `Config` is the single source of truth for repos +
//!   user-visible UI defaults; the runtime `repos` Mutex is kept in lock-step
//!   via `rebuild_worktrees` after every mutation.
//! - `update()` is just dispatch — each view module owns its own central
//!   panel.

use crate::perf::{PerfSession, PerfSummary};
use crate::runtime::worktree_create::{CreateWorktreeDialog, CreateWorktreeOutcome};
use crate::runtime::worktree_rename::RenameWorktreeDialog;
use crate::runtime::worktrees::expand_worktrees;
use crate::runtime::{
    ActiveRun, ActiveRunSummary, AgentContextViewState, BacklogViewState, ConfirmRemoveWorktree,
    PickerState, ViewTab, WorktreeMeta,
};
use crate::sync::{Kick, Status};
use crate::ui;
use crate::ui::onboarding::DiscoveryState;
use crate::workers::{self, Channels};
use eframe::egui;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use switchbard_core::config::Config;
use switchbard_core::{
    assess_branch_delete, collect_dirty_files, config, delete_branch, is_primary_worktree,
    kill_pgid, load_agent_context_cache, load_backlog_project, open_url, remove_worktree,
    spawn_in_session, url_for_port, AgentContextMap, AttributedListener, BacklogProject,
    BacklogTaskPatch, DetectedService, KillOutcome, NewBacklogTask, Repo, WorktreeRef,
    BROWSER_APP_NAMES,
};

/// Legible band for the persisted UI zoom factor. A hand-edited config or an
/// enthusiastic ⌘+ can't push the window outside this on load; the top-bar
/// stepper steps within it. (egui's own keyboard zoom may briefly exceed it at
/// runtime — `clamp_ui_scale` pulls it back on the next launch.)
pub const MIN_UI_SCALE: f32 = 0.6;
pub const MAX_UI_SCALE: f32 = 3.0;
/// One stepper click; matches the feel of egui's keyboard-zoom granularity.
pub const UI_SCALE_STEP: f32 = 0.1;

/// Clamp a zoom factor into the legible band, mapping a corrupt NaN/∞ back to
/// native scale.
pub fn clamp_ui_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
    } else {
        1.0
    }
}

#[derive(Default)]
pub struct ScanState {
    pub listeners: Vec<AttributedListener>,
    pub last_scan: Option<Instant>,
    pub last_error: Option<String>,
}

pub struct HiveApp {
    // Shared with worker threads.
    pub repos: Arc<Mutex<Vec<Repo>>>,
    pub worktrees: Arc<Mutex<Vec<WorktreeRef>>>,
    pub meta: Arc<Mutex<HashMap<PathBuf, WorktreeMeta>>>,
    pub services: Arc<Mutex<HashMap<PathBuf, Vec<DetectedService>>>>,
    pub agent_contexts: Arc<Mutex<HashMap<PathBuf, AgentContextMap>>>,
    pub backlog_projects: Arc<Mutex<HashMap<PathBuf, BacklogProject>>>,
    pub active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    pub state: Arc<Mutex<ScanState>>,
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
    pub agent_context_kick: Kick,
    pub backlog_kick: Kick,
    pub picker: Arc<Mutex<PickerState>>,

    // Per-view feedback channels. One per UI surface so messages don't
    // overwrite each other when several actions land in the same frame.
    pub config_status: Status,
    pub kill_status: Status,
    pub server_status: Status,
    pub backlog_status: Status,

    // Persisted config (single source of truth for repos + UI defaults).
    pub config: Config,

    // View-only state.
    /// One workspace-wide filter. Each section's match function reads it.
    pub filter: String,
    /// When on, the workspace hides unattributed listeners.
    pub show_only_managed: bool,
    pub confirm_kill_all: bool,
    /// When Some, the sidebar shows a "Remove '{name}'?" confirmation modal
    /// for the repo at the given path. The ✕ button in the sidebar sets this;
    /// the modal clears it on Confirm or Cancel.
    pub confirm_remove_repo: Option<(PathBuf, String)>,
    /// Modal state for `git worktree remove`. Shared with the worker thread
    /// so it can flip `busy`/`error` while the dialog is visible.
    pub confirm_remove_worktree: Arc<Mutex<Option<ConfirmRemoveWorktree>>>,
    /// Modal state for `git worktree add`.
    pub create_worktree_dialog: Arc<Mutex<Option<CreateWorktreeDialog>>>,
    /// Worker-to-UI completion queue for create operations. The worker runs
    /// git; the UI thread mutates persisted config after success.
    pub create_worktree_outcomes: Arc<Mutex<Vec<CreateWorktreeOutcome>>>,
    /// Modal state for renaming the Switchbard-local worktree label.
    pub rename_worktree_dialog: Option<RenameWorktreeDialog>,
    pub expanded_repos: BTreeSet<String>,
    /// When false (default), hide rows whose classifier verdict is NotServer
    /// (test scripts, build wrappers, ship-gate runners, etc.).
    pub show_non_servers: bool,
    pub view_tab: ViewTab,
    pub agent_context_view: AgentContextViewState,
    pub backlog_view: BacklogViewState,
    /// 0 = system default; 1..=BROWSER_APP_NAMES.len() = specific browser.
    pub browser_choice: usize,
    /// First-launch discovery state. Hidden by default; flips to Scanning
    /// → Ready while the welcome modal is on screen. After dismissal it
    /// returns to Hidden permanently for this session.
    pub onboarding: Arc<Mutex<DiscoveryState>>,
    /// Optional frame/render telemetry. Enabled with `SWITCHBARD_PERF=1`.
    perf: Option<PerfSession>,
}

fn cached_agent_contexts(worktrees: &[WorktreeRef]) -> HashMap<PathBuf, AgentContextMap> {
    let live_paths: BTreeSet<PathBuf> = worktrees.iter().map(|w| w.path.clone()).collect();
    load_agent_context_cache()
        .unwrap_or_default()
        .into_iter()
        .filter(|map| live_paths.contains(&map.worktree))
        .map(|map| (map.worktree.clone(), map))
        .collect()
}

impl HiveApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: Config,
        repos: Vec<Repo>,
        worktrees: Vec<WorktreeRef>,
    ) -> Self {
        ui::theme::apply(&cc.egui_ctx);
        // Restore the user's saved zoom before the first frame paints (eframe's
        // own zoom memory doesn't persist without the `persistence` feature).
        cc.egui_ctx.set_zoom_factor(clamp_ui_scale(cfg.ui.ui_scale));

        // Seed the first frame from the on-disk agent-context cache before any
        // worker scan completes, then start the workers against this state.
        let cached = cached_agent_contexts(&worktrees);
        let app = Self::new_headless(cfg, repos, worktrees);
        *app.agent_contexts.lock().unwrap() = cached;
        app.spawn_workers(cc.egui_ctx.clone());
        app
    }

    /// Assemble `HiveApp` and all of its shared state **without** spawning
    /// worker threads, touching an egui context, or reading the on-disk cache.
    /// `new` builds on top of this (theme + cache seed + workers); UI tests and
    /// headless harnesses use it directly and drive [`render_ui`] by hand.
    ///
    /// [`render_ui`]: HiveApp::render_ui
    pub fn new_headless(cfg: Config, repos: Vec<Repo>, worktrees: Vec<WorktreeRef>) -> Self {
        let browser_choice = cfg
            .ui
            .browser
            .as_deref()
            .and_then(|name| {
                BROWSER_APP_NAMES
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(name))
                    .map(|i| i + 1)
            })
            .unwrap_or(0);
        let show_non_servers = cfg.ui.show_non_servers;

        Self {
            repos: Arc::new(Mutex::new(repos)),
            worktrees: Arc::new(Mutex::new(worktrees)),
            meta: Arc::new(Mutex::new(HashMap::new())),
            services: Arc::new(Mutex::new(HashMap::new())),
            agent_contexts: Arc::new(Mutex::new(HashMap::new())),
            backlog_projects: Arc::new(Mutex::new(HashMap::new())),
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(ScanState::default())),
            scanner_kick: Kick::new(),
            probe_kick: Kick::new(),
            detection_kick: Kick::new(),
            agent_context_kick: Kick::new(),
            backlog_kick: Kick::new(),
            config: cfg,
            picker: Arc::new(Mutex::new(PickerState::Idle)),
            config_status: Status::new(),
            kill_status: Status::new(),
            server_status: Status::new(),
            backlog_status: Status::new(),
            filter: String::new(),
            show_only_managed: false,
            confirm_kill_all: false,
            confirm_remove_repo: None,
            confirm_remove_worktree: Arc::new(Mutex::new(None)),
            create_worktree_dialog: Arc::new(Mutex::new(None)),
            create_worktree_outcomes: Arc::new(Mutex::new(Vec::new())),
            rename_worktree_dialog: None,
            expanded_repos: BTreeSet::new(),
            show_non_servers,
            view_tab: ViewTab::Servers,
            agent_context_view: AgentContextViewState::default(),
            backlog_view: BacklogViewState::default(),
            browser_choice,
            onboarding: Arc::new(Mutex::new(DiscoveryState::default())),
            perf: PerfSession::from_env(),
        }
    }

    /// Spawn the four background workers, wiring them to this app's shared
    /// state. Separated from `new_headless` so tests can build an app that
    /// never starts threads.
    fn spawn_workers(&self, ctx: egui::Context) {
        workers::spawn_all(
            ctx,
            Channels {
                state: self.state.clone(),
                repos: self.repos.clone(),
                worktrees: self.worktrees.clone(),
                meta: self.meta.clone(),
                services: self.services.clone(),
                agent_contexts: self.agent_contexts.clone(),
                backlog_projects: self.backlog_projects.clone(),
                active_runs: self.active_runs.clone(),
                scanner_kick: self.scanner_kick.clone(),
                probe_kick: self.probe_kick.clone(),
                detection_kick: self.detection_kick.clone(),
                agent_context_kick: self.agent_context_kick.clone(),
                backlog_kick: self.backlog_kick.clone(),
            },
        );
    }

    pub fn repos_snapshot(&self) -> Vec<Repo> {
        self.repos.lock().unwrap().clone()
    }

    pub fn worktrees_snapshot(&self) -> Vec<WorktreeRef> {
        self.worktrees.lock().unwrap().clone()
    }

    pub fn backlog_projects_snapshot(&self) -> HashMap<PathBuf, BacklogProject> {
        self.backlog_projects.lock().unwrap().clone()
    }

    pub fn kick_all(&self) {
        self.scanner_kick.notify();
        self.probe_kick.notify();
        self.detection_kick.notify();
        self.agent_context_kick.notify();
        self.backlog_kick.notify();
    }

    pub fn mark_agent_contexts_stale(&self) {
        for map in self.agent_contexts.lock().unwrap().values_mut() {
            map.scanned_at = None;
        }
        self.agent_context_kick.notify();
    }

    /// Save the in-memory config to disk. Reports failures via `config_status`
    /// so the user sees what happened — we don't swallow the cause.
    pub fn save_config(&self) {
        if let Err(e) = config::save(&self.config) {
            self.config_status.set(format!("config save failed: {e}"));
        }
    }

    /// Push current UI fields into `self.config` and persist.
    pub fn save_ui_to_config(&mut self) {
        self.config.ui.browser = if self.browser_choice == 0 {
            None
        } else {
            BROWSER_APP_NAMES
                .get(self.browser_choice - 1)
                .map(|s| s.to_string())
        };
        self.config.ui.show_non_servers = self.show_non_servers;
        self.save_config();
    }

    /// Add a new repo (after the user picked a path). Idempotent: a path
    /// that's already configured returns a "already configured" notice
    /// without touching state.
    ///
    /// Side effect: dismisses the first-launch onboarding modal on the
    /// first real add, so the browse-flow exit path doesn't keep the
    /// welcome modal hanging around.
    pub fn add_repo_from_path(&mut self, path: PathBuf) {
        if self.config.repos.iter().any(|r| r.path == path) {
            self.config_status
                .set(format!("'{}' already configured", path.display()));
            return;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "repo".to_string());
        self.config.repos.push(Repo {
            name: name.clone(),
            path,
        });
        if !self.config.ui.onboarding_dismissed {
            self.config.ui.onboarding_dismissed = true;
            *self.onboarding.lock().unwrap() = DiscoveryState::Hidden;
        }
        self.save_config();
        self.rebuild_worktrees();
        self.config_status.set(format!("added '{name}'"));
        self.kick_all();
    }

    /// Remove a configured repo by path. Worktrees for that repo are dropped
    /// from the shared list; any running services we launched from those
    /// worktrees keep running (kill them via Listeners > Kill if needed).
    pub fn remove_repo(&mut self, repo_path: PathBuf) {
        let before = self.config.repos.len();
        self.config.repos.retain(|r| r.path != repo_path);
        if self.config.repos.len() == before {
            return;
        }
        self.save_config();
        self.rebuild_worktrees();
        self.config_status
            .set(format!("removed '{}'", repo_path.display()));
        self.scanner_kick.notify();
    }

    /// Open the "Remove worktree?" confirmation dialog. If the target is the
    /// repo's primary worktree, refuses outright with a status message — the
    /// user should use Remove repo instead, and `git worktree remove` would
    /// fail on a primary anyway. Otherwise collects dirty files + active runs
    /// synchronously (fast: one `git status` call) and stores the dialog state.
    pub fn open_remove_worktree_confirm(
        &self,
        repo_path: PathBuf,
        worktree_path: PathBuf,
        branch: Option<String>,
    ) {
        if is_primary_worktree(&repo_path, &worktree_path) {
            self.config_status.set(format!(
                "'{}' is the primary worktree — remove the repo to drop it",
                worktree_path.display()
            ));
            return;
        }
        // Surface git-status failures rather than treating them as "clean".
        // A locked index, safe.directory misconfig, or permission error must
        // never lead the user to confirm a removal believing nothing is dirty.
        let dirty_files = match collect_dirty_files(&worktree_path) {
            Ok(files) => files,
            Err(e) => {
                self.config_status.set(format!(
                    "cannot verify worktree state at '{}': {} — fix git state and try again",
                    worktree_path.display(),
                    e
                ));
                return;
            }
        };
        let active_runs = self.snapshot_runs_for_worktree(&worktree_path);
        // Best-effort local assessment of deleting the backing branch. A few
        // fast git calls (worktree list + rev-list count); same latency budget
        // as the dirty-file probe above.
        let branch_assessment = branch
            .as_ref()
            .map(|b| assess_branch_delete(&repo_path, b, &worktree_path));
        *self.confirm_remove_worktree.lock().unwrap() = Some(ConfirmRemoveWorktree {
            repo_path,
            worktree_path,
            branch,
            dirty_files,
            active_runs,
            branch_assessment,
            delete_branch: false,
            busy: false,
            error: None,
        });
    }

    /// Active runs whose `worktree_path` matches, projected to the lightweight
    /// summary the dialog renders. Used at dialog-open time AND at confirm
    /// time so the worker thread can detect drift before signaling anything.
    fn snapshot_runs_for_worktree(&self, worktree_path: &Path) -> Vec<ActiveRunSummary> {
        self.active_runs
            .lock()
            .unwrap()
            .values()
            .filter(|r| r.worktree_path == worktree_path)
            .map(|r| ActiveRunSummary {
                service_name: r.service_name.clone(),
                pgid: r.pgid,
            })
            .collect()
    }

    /// Close the dialog without doing anything. The X / Cancel button calls
    /// this — never call it while `busy` is true (the UI hides Cancel during
    /// execution, so this is enforced at the call site).
    pub fn cancel_remove_worktree_confirm(&self) {
        *self.confirm_remove_worktree.lock().unwrap() = None;
    }

    /// Run the confirmed removal on a worker thread:
    ///   0. **Preflight re-snapshot** — re-collect dirty files + active runs.
    ///      If either drifted from the dialog snapshot (new uncommitted files,
    ///      new tracked runs), abort and re-populate the dialog with fresh
    ///      state instead of acting on stale info. Prevents silently
    ///      discarding changes the user never saw and prevents orphaning a
    ///      service that wasn't running when the dialog opened.
    ///   1. SIGTERM (then SIGKILL after grace) every `active_runs` pgid. If
    ///      a kill returns an error, abort: the run is still alive and we
    ///      shouldn't pretend we stopped it.
    ///   2. Drop killed entries from `active_runs` so the UI stops showing them.
    ///   3. `git worktree remove [--force]` — `--force` iff the worktree was dirty.
    ///   4. On success: clear the dialog, refresh the worktrees list from disk,
    ///      kick scanner + probes so the row vanishes immediately.
    ///   5. On failure: leave the dialog open with `error` populated so the
    ///      user can read git's complaint and either retry or cancel.
    pub fn execute_remove_worktree(&self, ctx: &egui::Context) {
        let snapshot = {
            let mut guard = self.confirm_remove_worktree.lock().unwrap();
            let Some(state) = guard.as_mut() else {
                return;
            };
            if state.busy {
                return;
            }
            state.busy = true;
            state.error = None;
            state.clone()
        };

        let confirm = self.confirm_remove_worktree.clone();
        let active_runs = self.active_runs.clone();
        let worktrees = self.worktrees.clone();
        let repos = self.repos.clone();
        let scanner_kick = self.scanner_kick.clone();
        let probe_kick = self.probe_kick.clone();
        let detection_kick = self.detection_kick.clone();
        let agent_context_kick = self.agent_context_kick.clone();
        let config_status = self.config_status.clone();
        let ctx = ctx.clone();
        let fresh_runs = self.snapshot_runs_for_worktree(&snapshot.worktree_path);

        thread::spawn(move || {
            // 0: preflight re-snapshot. The dialog's view of "what's dirty"
            //    and "what's running" was captured when the dialog opened,
            //    possibly seconds ago. Background tooling could have touched
            //    files, or the user could have started a service. Re-check
            //    both before we kill anything or invoke --force.
            let fresh_dirty = match switchbard_core::collect_dirty_files(&snapshot.worktree_path) {
                Ok(files) => files,
                Err(e) => {
                    drift_abort(
                        &confirm,
                        format!("cannot verify worktree state: {e} — try again"),
                    );
                    ctx.request_repaint();
                    return;
                }
            };
            if state_drifted(&snapshot.dirty_files, &fresh_dirty)
                || runs_drifted(&snapshot.active_runs, &fresh_runs)
            {
                drift_abort_and_refresh(
                    &confirm,
                    fresh_dirty,
                    fresh_runs,
                    "state changed since dialog opened — review the updated list and confirm again",
                );
                ctx.request_repaint();
                return;
            }

            // 1+2: kill running services in this worktree. Honor kill_pgid's
            //      result — if it errors we are NOT confident the process is
            //      gone, so abort the whole removal rather than risk
            //      losing track of a live process.
            let mut killed = 0usize;
            for run in &snapshot.active_runs {
                match kill_pgid(run.pgid, Duration::from_secs(3)) {
                    Ok(_) => {
                        active_runs.lock().unwrap().remove(&run.pgid);
                        killed += 1;
                    }
                    Err(e) => {
                        drift_abort(
                            &confirm,
                            format!(
                                "could not stop '{}' (pgid {}): {} — service may still be running",
                                run.service_name, run.pgid, e
                            ),
                        );
                        ctx.request_repaint();
                        return;
                    }
                }
            }

            // 3: shell out to git.
            let force = !snapshot.dirty_files.is_empty();
            let result = remove_worktree(&snapshot.repo_path, &snapshot.worktree_path, force);

            match result {
                Ok(()) => {
                    // 4: drop the row from the shared worktrees list so the
                    //    UI stops rendering it before the next probe tick
                    //    catches up.
                    worktrees
                        .lock()
                        .unwrap()
                        .retain(|w| w.path != snapshot.worktree_path);
                    let _ = repos; // kept in scope for parity; rebuild not needed
                    *confirm.lock().unwrap() = None;
                    let name = snapshot
                        .branch
                        .clone()
                        .unwrap_or_else(|| snapshot.worktree_path.display().to_string());
                    let extras = if killed > 0 {
                        format!(
                            " (stopped {killed} service{})",
                            if killed == 1 { "" } else { "s" }
                        )
                    } else {
                        String::new()
                    };

                    // 5: opt-in branch cleanup, only now that the worktree is
                    //    gone (git refuses to delete a checked-out branch). The
                    //    worktree removal already succeeded and is irreversible,
                    //    so a branch-delete failure is reported as a non-fatal
                    //    addendum, never an error that "undoes" the removal.
                    let branch_note =
                        delete_branch_after_removal(&snapshot, snapshot.branch.as_deref());
                    config_status.set(format!("removed worktree '{name}'{extras}{branch_note}"));
                    scanner_kick.notify();
                    probe_kick.notify();
                    detection_kick.notify();
                    agent_context_kick.notify();
                }
                Err(e) => {
                    if let Some(state) = confirm.lock().unwrap().as_mut() {
                        state.busy = false;
                        state.error = Some(e.to_string());
                    }
                }
            }
            ctx.request_repaint();
        });
    }

    /// Move the repo at index `i` up (delta = -1) or down (delta = 1). Saves
    /// the new order to `~/.switchbard/config.toml` and refreshes the runtime view
    /// so the sidebar / per-repo sections re-render in the new order.
    pub fn move_repo(&mut self, i: usize, delta: isize) {
        let len = self.config.repos.len();
        let j = (i as isize + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
        if i == j {
            return;
        }
        self.config.repos.swap(i, j);
        self.save_config();
        self.rebuild_worktrees();
    }

    /// Recompute the runtime `repos` + `worktrees` mutexes from
    /// `self.config.repos`. Called after the user adds/removes a repo.
    fn rebuild_worktrees(&self) {
        let runtime_repos: Vec<Repo> = self.config.repos.clone();
        let wts = expand_worktrees(&runtime_repos);
        *self.repos.lock().unwrap() = runtime_repos;
        *self.worktrees.lock().unwrap() = wts;
    }

    /// Re-run `git worktree list` against the currently-configured repos.
    /// Unlike `rebuild_worktrees`, this leaves the repo list alone — it's the
    /// "user externally pruned/added some worktrees, pick up the changes" path.
    pub fn refresh_worktrees_from_disk(&self) -> WorktreeDelta {
        let repos = self.repos_snapshot();
        let before: usize = self.worktrees.lock().unwrap().len();
        let fresh = expand_worktrees(&repos);
        let after = fresh.len();
        *self.worktrees.lock().unwrap() = fresh;
        WorktreeDelta { before, after }
    }

    /// Open the native folder-picker on a worker thread; result lands in
    /// `self.picker` and is drained next frame.
    pub fn open_repo_picker(&self, ctx: &egui::Context) {
        {
            let mut p = self.picker.lock().unwrap();
            if !matches!(*p, PickerState::Idle) {
                return;
            }
            *p = PickerState::InFlight;
        }
        let picker = self.picker.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Select a repository directory")
                .pick_folder();
            *picker.lock().unwrap() = match picked {
                Some(path) => PickerState::Picked(path),
                None => PickerState::Idle,
            };
            ctx.request_repaint();
        });
    }

    /// Drain the picker if a path was returned; called once per frame.
    fn drain_picker(&mut self) {
        let picked = {
            let mut p = self.picker.lock().unwrap();
            if let PickerState::Picked(_) = &*p {
                if let PickerState::Picked(path) = std::mem::replace(&mut *p, PickerState::Idle) {
                    Some(path)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(path) = picked {
            self.add_repo_from_path(path);
        }
    }

    pub fn spawn_kill(&self, pgid: i32, ctx: &egui::Context) {
        let kick = self.scanner_kick.clone();
        let status = self.kill_status.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            status.set(describe_kill(pgid, kill_pgid(pgid, Duration::from_secs(3))));
            kick.notify();
            ctx.request_repaint();
        });
    }

    pub fn spawn_kill_many(&self, pgids: Vec<i32>, ctx: &egui::Context) {
        let kick = self.scanner_kick.clone();
        let status = self.kill_status.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let mut terminated = 0usize;
            let mut killed = 0usize;
            let mut not_found = 0usize;
            let mut errored = 0usize;
            for pgid in &pgids {
                match kill_pgid(*pgid, Duration::from_secs(3)) {
                    Ok(KillOutcome::Terminated) => terminated += 1,
                    Ok(KillOutcome::Killed) => killed += 1,
                    Ok(KillOutcome::NotFound) => not_found += 1,
                    Err(_) => errored += 1,
                }
            }
            status.set(format!(
                "kill-all: {} terminated, {} killed, {} gone, {} errored ({} pgids)",
                terminated,
                killed,
                not_found,
                errored,
                pgids.len()
            ));
            kick.notify();
            ctx.request_repaint();
        });
    }

    fn browser_app_name(&self) -> Option<&'static str> {
        if self.browser_choice == 0 {
            None
        } else {
            BROWSER_APP_NAMES.get(self.browser_choice - 1).copied()
        }
    }

    pub fn spawn_start(
        &self,
        worktree_path: PathBuf,
        service: DetectedService,
        ctx: &egui::Context,
    ) {
        let active_runs = self.active_runs.clone();
        let status = self.server_status.clone();
        let kick = self.scanner_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let log_root = std::env::temp_dir().join("switchbard-logs");
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
            let safe_name: String = service
                .name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let log_path = log_root.join(format!("{ts}-{safe_name}.log"));
            let cwd = worktree_path.join(&service.cwd_rel);
            match spawn_in_session(&service.command, &cwd, &log_path) {
                Ok(run) => {
                    let active = ActiveRun {
                        worktree_path: worktree_path.clone(),
                        service_name: service.name.clone(),
                        command: service.command.clone(),
                        pid: run.pid,
                        pgid: run.pgid,
                        started_at: Instant::now(),
                        log_path: run.log_path,
                    };
                    active_runs.lock().unwrap().insert(run.pgid, active);
                    status.set(format!("started '{}' (pid {})", service.name, run.pid));
                    kick.notify();
                }
                Err(e) => {
                    status.set(format!("spawn failed for '{}': {}", service.name, e));
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn spawn_stop_run(&self, pgid: i32, service_name: String, ctx: &egui::Context) {
        let active_runs = self.active_runs.clone();
        let status = self.server_status.clone();
        let kick = self.scanner_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let msg = match kill_pgid(pgid, Duration::from_secs(5)) {
                Ok(KillOutcome::Terminated) => format!("stopped '{service_name}' (SIGTERM)"),
                Ok(KillOutcome::Killed) => format!("force-killed '{service_name}' (SIGKILL)"),
                Ok(KillOutcome::NotFound) => format!("'{service_name}' already gone"),
                Err(e) => format!("stop '{service_name}' failed: {e}"),
            };
            active_runs.lock().unwrap().remove(&pgid);
            status.set(msg);
            kick.notify();
            ctx.request_repaint();
        });
    }

    pub fn open_in_browser(&self, port: u16) {
        let url = url_for_port(port);
        let browser = self.browser_app_name();
        match open_url(&url, browser) {
            Ok(()) => {
                let label = browser.unwrap_or("default browser");
                self.server_status.set(format!("opened {url} in {label}"));
            }
            Err(e) => self.server_status.set(format!("open failed: {e}")),
        }
    }

    pub fn spawn_backlog_save(
        &self,
        project_root: PathBuf,
        task_id: String,
        patch: BacklogTaskPatch,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::edit_backlog_task(&project_root, &task_id, &patch) {
                Ok(_) => {
                    refresh_backlog_project_cache(&projects, &project_root);
                    status.set(format!("saved {task_id}"));
                    kick.notify();
                }
                Err(e) => status.set(format!("save {task_id} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    pub fn spawn_backlog_bulk_save(
        &self,
        project_root: PathBuf,
        task_ids: Vec<String>,
        patch: BacklogTaskPatch,
        action_label: String,
        ctx: &egui::Context,
    ) {
        if task_ids.is_empty() {
            return;
        }
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let total = task_ids.len();
            let mut saved = 0usize;
            let mut first_error: Option<String> = None;
            for task_id in &task_ids {
                match switchbard_core::edit_backlog_task(&project_root, task_id, &patch) {
                    Ok(_) => saved += 1,
                    Err(e) => {
                        if first_error.is_none() {
                            first_error = Some(format!("{task_id}: {e}"));
                        }
                    }
                }
            }
            if saved > 0 {
                refresh_backlog_project_cache(&projects, &project_root);
                kick.notify();
            }
            match first_error {
                Some(error) => status.set(format!(
                    "{action_label}: saved {saved}/{total} tasks; first failure: {error}"
                )),
                None => status.set(format!("{action_label}: updated {saved} task(s)")),
            }
            ctx.request_repaint();
        });
    }

    pub fn spawn_backlog_acceptance_toggle(
        &self,
        project_root: PathBuf,
        task_id: String,
        index: usize,
        checked: bool,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::set_backlog_acceptance_checked(
                &project_root,
                &task_id,
                index,
                checked,
            ) {
                Ok(_) => {
                    refresh_backlog_project_cache(&projects, &project_root);
                    let verb = if checked { "checked" } else { "unchecked" };
                    status.set(format!("{verb} {task_id} AC #{index}"));
                    kick.notify();
                }
                Err(e) => status.set(format!("update {task_id} AC #{index} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    pub fn spawn_backlog_append_note(
        &self,
        project_root: PathBuf,
        task_id: String,
        note: String,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::append_backlog_notes(&project_root, &task_id, &note) {
                Ok(_) => {
                    refresh_backlog_project_cache(&projects, &project_root);
                    status.set(format!("appended note to {task_id}"));
                    kick.notify();
                }
                Err(e) => status.set(format!("append note to {task_id} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    pub fn spawn_backlog_create(
        &self,
        project_root: PathBuf,
        task: NewBacklogTask,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::create_backlog_task(&project_root, &task) {
                Ok(output) => {
                    refresh_backlog_project_cache(&projects, &project_root);
                    let msg = if output.is_empty() {
                        "created task".to_string()
                    } else {
                        format!("created task: {output}")
                    };
                    status.set(msg);
                    kick.notify();
                }
                Err(e) => status.set(format!("create task failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    pub fn perf_count_worktree_row(&mut self, expanded: bool, services: usize, listeners: usize) {
        if let Some(perf) = &mut self.perf {
            perf.count_worktree_row(expanded, services, listeners);
        }
    }
}

/// Result of a worktree re-enumeration. Surfaces visible "+N / -M" feedback
/// to the Worktrees view so the user sees Refresh actually did something.
#[derive(Debug, Clone, Copy)]
pub struct WorktreeDelta {
    pub before: usize,
    pub after: usize,
}

impl WorktreeDelta {
    pub fn summary(&self) -> String {
        if self.before == self.after {
            format!("refreshed: {} worktrees (no change)", self.after)
        } else if self.after > self.before {
            format!(
                "refreshed: {} worktrees (+{})",
                self.after,
                self.after - self.before
            )
        } else {
            format!(
                "refreshed: {} worktrees (-{})",
                self.after,
                self.before - self.after
            )
        }
    }
}

/// Did the set of dirty files change between the open-time snapshot and the
/// confirm-time re-scan? Order-independent — `git status --porcelain` doesn't
/// guarantee order across invocations. Uses (status, path) tuples so an
/// edit that flips a file from `??` (untracked) to `A ` (staged add) also
/// trips the drift check.
fn state_drifted(
    original: &[switchbard_core::DirtyFile],
    fresh: &[switchbard_core::DirtyFile],
) -> bool {
    if original.len() != fresh.len() {
        return true;
    }
    let mut a: Vec<_> = original
        .iter()
        .map(|f| (f.status.clone(), f.path.clone()))
        .collect();
    let mut b: Vec<_> = fresh
        .iter()
        .map(|f| (f.status.clone(), f.path.clone()))
        .collect();
    a.sort();
    b.sort();
    a != b
}

/// Did the set of switchbard-tracked runs in this worktree change? Keys on
/// pgid since service names can be non-unique across services.
fn runs_drifted(original: &[ActiveRunSummary], fresh: &[ActiveRunSummary]) -> bool {
    if original.len() != fresh.len() {
        return true;
    }
    let mut a: Vec<i32> = original.iter().map(|r| r.pgid).collect();
    let mut b: Vec<i32> = fresh.iter().map(|r| r.pgid).collect();
    a.sort();
    b.sort();
    a != b
}

/// Run the opt-in branch deletion after the worktree has already been removed,
/// and return a short suffix for the status line describing what happened.
/// Empty string when the user didn't ask to delete the branch.
///
/// Force is taken straight from the dialog's stored assessment: a branch with
/// unlanded commits gets `git branch -D`, which the dialog made the user opt
/// into explicitly via the loud force-delete checkbox. Failure is non-fatal —
/// the worktree is already gone — so we report it inline rather than erroring.
fn delete_branch_after_removal(snapshot: &ConfirmRemoveWorktree, branch: Option<&str>) -> String {
    if !snapshot.will_delete_branch() {
        return String::new();
    }
    let Some(branch) = branch else {
        return String::new();
    };
    let force = snapshot
        .branch_assessment
        .as_ref()
        .is_some_and(|a| a.needs_force());
    match delete_branch(&snapshot.repo_path, branch, force) {
        Ok(()) => format!(" and deleted branch '{branch}'"),
        Err(e) => format!(" — branch '{branch}' NOT deleted: {e}"),
    }
}

/// Bail out of a removal that hasn't actually touched anything yet. Sets the
/// dialog's `error` so the user sees why, and clears `busy` so the buttons
/// re-enable.
fn drift_abort(confirm: &Arc<Mutex<Option<ConfirmRemoveWorktree>>>, message: String) {
    if let Some(state) = confirm.lock().unwrap().as_mut() {
        state.busy = false;
        state.error = Some(message);
    }
}

/// Bail out AND re-populate the dialog with fresh dirty/run lists so the user
/// can re-review and re-confirm. Used when the world changed under us but the
/// world's new state is something we can show.
fn drift_abort_and_refresh(
    confirm: &Arc<Mutex<Option<ConfirmRemoveWorktree>>>,
    fresh_dirty: Vec<switchbard_core::DirtyFile>,
    fresh_runs: Vec<ActiveRunSummary>,
    message: &str,
) {
    if let Some(state) = confirm.lock().unwrap().as_mut() {
        state.dirty_files = fresh_dirty;
        state.active_runs = fresh_runs;
        state.busy = false;
        state.error = Some(message.to_string());
    }
}

fn describe_kill(pgid: i32, result: std::io::Result<KillOutcome>) -> String {
    match result {
        Ok(KillOutcome::Terminated) => format!("killed pgid {pgid} (SIGTERM)"),
        Ok(KillOutcome::Killed) => format!("killed pgid {pgid} (SIGKILL)"),
        Ok(KillOutcome::NotFound) => format!("pgid {pgid} already gone"),
        Err(e) => format!("kill {pgid} failed: {e}"),
    }
}

impl HiveApp {
    /// Render every panel for one frame. The single source of truth for what
    /// the window shows: `update` wraps this with per-frame bookkeeping
    /// (picker draining, config persistence) that has no place in a test, and
    /// the egui_kittest UI harness calls it directly against seeded state.
    pub fn render_ui(&mut self, ctx: &egui::Context) {
        let frame_start = Instant::now();
        if let Some(perf) = &mut self.perf {
            perf.begin_frame();
        }
        self.drain_create_worktree_outcomes();

        let top_start = Instant::now();
        ui::top_bar::render(self, ctx);
        if let Some(perf) = &mut self.perf {
            perf.record_top_bar(top_start.elapsed());
        }

        // Sidebar must render BEFORE the central panel so the SidePanel claims
        // its docked space first; otherwise the central panel sizes to the full
        // window and the side panel overlays it.
        let sidebar_start = Instant::now();
        ui::sidebar::render(self, ctx);
        if let Some(perf) = &mut self.perf {
            perf.record_sidebar(sidebar_start.elapsed());
        }

        let central_start = Instant::now();
        match self.view_tab {
            ViewTab::Servers => ui::workspace::render(self, ctx),
            ViewTab::AgentContext => ui::agent_context::render(self, ctx),
            ViewTab::Backlog => ui::backlog::render(self, ctx),
        }
        let central_elapsed = central_start.elapsed();
        if let Some(perf) = &mut self.perf {
            perf.record_central(central_elapsed);
            if self.view_tab == ViewTab::Servers {
                perf.record_workspace(central_elapsed);
            }
        }

        // Onboarding overlay paints last so it sits on top of everything
        // else when shown. It no-ops when already dismissed.
        let onboarding_start = Instant::now();
        ui::onboarding::render(self, ctx);
        if let Some(perf) = &mut self.perf {
            perf.record_onboarding(onboarding_start.elapsed());
        }

        if let Some(summary) = self.perf.as_ref().and_then(PerfSession::summary) {
            render_perf_overlay(ctx, &summary);
        }
        if let Some(perf) = &mut self.perf {
            perf.finish_frame(frame_start.elapsed());
        }
    }
}

fn render_perf_overlay(ctx: &egui::Context, summary: &PerfSummary) {
    egui::Area::new(egui::Id::new("switchbard_perf_overlay"))
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 64.0])
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 235))
                .stroke(egui::Stroke::new(
                    1.0,
                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                ))
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(summary.overlay_text())
                            .monospace()
                            .color(ui.visuals().text_color()),
                    );
                });
        });
}

fn refresh_backlog_project_cache(
    projects: &Arc<Mutex<HashMap<PathBuf, BacklogProject>>>,
    project_root: &Path,
) {
    if let Ok(project) = load_backlog_project(project_root) {
        projects
            .lock()
            .unwrap()
            .insert(project_root.to_path_buf(), project);
    }
}

impl eframe::App for HiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_picker();

        // Snapshot persistable UI state so we can save the config if any
        // toggle was flipped this update.
        let ui_before = (self.browser_choice, self.show_non_servers);

        self.render_ui(ctx);

        // Capture the live zoom (top-bar stepper or ⌘+/⌘−/⌘0) so it survives a
        // restart. egui's keyboard zoom lands one frame late, which the next
        // frame's read-back picks up — invisible to the user.
        let zoom = ctx.zoom_factor();
        let zoom_changed = (zoom - self.config.ui.ui_scale).abs() > f32::EPSILON;
        if zoom_changed {
            self.config.ui.ui_scale = zoom;
        }

        let ui_after = (self.browser_choice, self.show_non_servers);
        if ui_before != ui_after || zoom_changed {
            self.save_ui_to_config();
        }
    }
}
