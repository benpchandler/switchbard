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

use crate::runtime::worktrees::expand_worktrees;
use crate::runtime::{ActiveRun, PickerState, ViewMode, WorktreeMeta};
use crate::sync::{Kick, Status};
use crate::ui;
use crate::workers::{self, Channels};
use eframe::egui;
use hive_core::config::Config;
use hive_core::{
    config, kill_pgid, open_url, spawn_in_session, url_for_port, AttributedListener,
    DetectedService, KillOutcome, Repo, WorktreeRef, BROWSER_APP_NAMES,
};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    pub active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    pub state: Arc<Mutex<ScanState>>,
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
    pub picker: Arc<Mutex<PickerState>>,

    // Per-view feedback channels. One per UI surface so messages don't
    // overwrite each other when several actions land in the same frame.
    pub config_status: Status,
    pub kill_status: Status,
    pub server_status: Status,

    // Persisted config (single source of truth for repos + UI defaults).
    pub config: Config,

    // View-only state.
    pub view: ViewMode,
    pub show_only_managed: bool,
    pub filter: String,
    /// When true (default), the Listeners central panel renders one section
    /// per repo, with worktree sub-headings, instead of a single flat table.
    pub group_listeners: bool,
    pub confirm_kill_all: bool,
    pub expanded_repos: BTreeSet<String>,
    pub wt_filter: String,
    pub server_filter: String,
    /// When false (default), hide rows whose classifier verdict is NotServer
    /// (test scripts, build wrappers, ship-gate runners, etc.).
    pub show_non_servers: bool,
    /// 0 = system default; 1..=BROWSER_APP_NAMES.len() = specific browser.
    pub browser_choice: usize,
}

impl HiveApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: Config,
        repos: Vec<Repo>,
        worktrees: Vec<WorktreeRef>,
    ) -> Self {
        ui::theme::apply(&cc.egui_ctx);

        let state = Arc::new(Mutex::new(ScanState::default()));
        let scanner_kick = Kick::new();
        let probe_kick = Kick::new();
        let detection_kick = Kick::new();
        let repos_arc = Arc::new(Mutex::new(repos));
        let worktrees_arc = Arc::new(Mutex::new(worktrees));
        let meta = Arc::new(Mutex::new(HashMap::new()));
        let services = Arc::new(Mutex::new(HashMap::new()));
        let active_runs = Arc::new(Mutex::new(HashMap::new()));
        let picker = Arc::new(Mutex::new(PickerState::Idle));

        workers::spawn_all(
            cc.egui_ctx.clone(),
            Channels {
                state: state.clone(),
                repos: repos_arc.clone(),
                worktrees: worktrees_arc.clone(),
                meta: meta.clone(),
                services: services.clone(),
                active_runs: active_runs.clone(),
                scanner_kick: scanner_kick.clone(),
                probe_kick: probe_kick.clone(),
                detection_kick: detection_kick.clone(),
            },
        );

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
        let group_listeners = cfg.ui.group_listeners;
        let show_non_servers = cfg.ui.show_non_servers;

        Self {
            repos: repos_arc,
            worktrees: worktrees_arc,
            meta,
            services,
            active_runs,
            state,
            scanner_kick,
            probe_kick,
            detection_kick,
            config: cfg,
            picker,
            config_status: Status::new(),
            kill_status: Status::new(),
            server_status: Status::new(),
            view: ViewMode::Listeners,
            show_only_managed: false,
            filter: String::new(),
            group_listeners,
            confirm_kill_all: false,
            expanded_repos: BTreeSet::new(),
            wt_filter: String::new(),
            server_filter: String::new(),
            show_non_servers,
            browser_choice,
        }
    }

    pub fn repos_snapshot(&self) -> Vec<Repo> {
        self.repos.lock().unwrap().clone()
    }

    pub fn worktrees_snapshot(&self) -> Vec<WorktreeRef> {
        self.worktrees.lock().unwrap().clone()
    }

    pub fn kick_all(&self) {
        self.scanner_kick.notify();
        self.probe_kick.notify();
        self.detection_kick.notify();
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
        self.config.ui.group_listeners = self.group_listeners;
        self.config.ui.show_non_servers = self.show_non_servers;
        self.save_config();
    }

    /// Add a new repo (after the user picked a path). Idempotent: a path
    /// that's already configured returns a "already configured" notice
    /// without touching state.
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
            let log_root = std::env::temp_dir().join("hive-logs");
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

fn describe_kill(pgid: i32, result: std::io::Result<KillOutcome>) -> String {
    match result {
        Ok(KillOutcome::Terminated) => format!("killed pgid {pgid} (SIGTERM)"),
        Ok(KillOutcome::Killed) => format!("killed pgid {pgid} (SIGKILL)"),
        Ok(KillOutcome::NotFound) => format!("pgid {pgid} already gone"),
        Err(e) => format!("kill {pgid} failed: {e}"),
    }
}

impl eframe::App for HiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_picker();

        // Snapshot persistable UI state so we can save the config if any
        // toggle was flipped this update.
        let ui_before = (
            self.browser_choice,
            self.group_listeners,
            self.show_non_servers,
        );

        ui::top_bar::render(self, ctx);
        match self.view {
            ViewMode::Listeners => ui::listeners::render(self, ctx),
            ViewMode::Worktrees => ui::worktrees::render(self, ctx),
            ViewMode::Servers => ui::servers::render(self, ctx),
        }

        let ui_after = (
            self.browser_choice,
            self.group_listeners,
            self.show_non_servers,
        );
        if ui_before != ui_after {
            self.save_ui_to_config();
        }
    }
}
