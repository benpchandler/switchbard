//! `HiveApp` — the eframe::App. Owns shared state, hosts user actions,
//! delegates rendering to the `views` module.
//!
//! Design rules in here:
//! - Anything a worker thread needs goes in an `Arc<Mutex<>>` field.
//! - Anything purely view-state (expansion toggles, view tab) is owned
//!   directly by the struct. Filter queries and facets persist per surface
//!   through `Config.ui.filters` instead — see `filter`/`filter_mut` and
//!   `persist_filter_facets`.
//! - The persisted `Config` is the single source of truth for repos +
//!   user-visible UI defaults; the runtime `repos` Mutex is kept in lock-step
//!   via `rebuild_worktrees` after every mutation.
//! - `update()` is just dispatch — each view module owns its own central
//!   panel.
//!
//! ## Mutation-method naming convention
//!
//! | Prefix | Role | Examples |
//! |--------|------|---------|
//! | `open_` | Show a confirmation/creation modal; validates preconditions synchronously; sets dialog state. | `open_remove_worktree_confirm`, `open_create_worktree` |
//! | `cancel_` | Dismiss a modal without acting. | `cancel_remove_worktree_confirm`, `cancel_create_worktree` |
//! | `execute_` | Commit a modal action on a **worker thread** (the heavy I/O path). Flips `busy`; result lands back via an outcomes queue or in-place dialog mutation. | `execute_remove_worktree`, `execute_create_worktree` |
//! | `add_` / `remove_` / `move_` | Synchronous repo-list CRUD; calls `after_repos_mutation` on the UI thread. | `add_repo_from_path`, `remove_repo`, `move_repo` |
//! | `spawn_` | Fire-and-forget threaded actions with no confirmation modal (start/stop/kill). | `spawn_start`, `spawn_stop_run`, `spawn_kill` |

use crate::perf::{PerfSession, PerfSummary};
use crate::runtime::worktree_create::{CreateWorktreeDialog, CreateWorktreeOutcome};
use crate::runtime::worktree_rename::RenameWorktreeDialog;
use crate::runtime::worktrees::expand_worktrees;
use crate::runtime::{
    dispatch_run_holds_worktree, ActiveRun, ActiveRunSummary, AgentContextViewState, AgentsSection,
    BacklogTaskKey, BacklogViewState, BoardMoveOutcome, ConfirmBulkRemoveWorktrees,
    ConfirmRemoveWorktree, LandingEntry, OrderingState, PickerState, ViewTab, WorktreeMeta,
    WorktreeSizeEntry,
};
use crate::sync::{Kick, Progress, Status};
use crate::ui;
use crate::ui::backlog::status_migration::StatusMigrationPrompt;
use crate::ui::onboarding::DiscoveryState;
use crate::ui::workspace::staleness::StalenessFilter;
use crate::workers::{self, Channels};
use crate::worktree_actions::RemovedWorktree;
use eframe::egui;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use switchbard_core::config::Config;
use switchbard_core::dispatch_inspect::DispatchRun;
use switchbard_core::instance_lock::{self, AcquireError, InstanceLock};
use switchbard_core::{
    assess_branch_delete, collect_dirty_files, config, delete_branch, is_primary_worktree,
    kill_dispatch_run, kill_pgid, load_agent_context_cache, load_backlog_project, open_url,
    probe_facts, remove_worktree, spawn_in_session, url_for_port, AgentContextMap,
    AttachedProcesses, AttributedListener, BacklogProject, BacklogTaskPatch, DetectedService, Fact,
    KillOutcome, NewBacklogTask, Repo, WorktreeRef, BROWSER_APP_NAMES,
};

/// One `backlog` CLI writer's per-task serialization registry (task-42,
/// post-review revision) — see `HiveApp::task_write_locks`'s own doc for
/// what it's for. A plain type alias rather than a real newtype: nothing
/// here needs its own methods, this exists purely so the nested-Arc/Mutex
/// type doesn't have to be spelled out (and re-triggers clippy's
/// `type_complexity` lint) at every function signature that touches it.
type TaskWriteLocks = Arc<Mutex<HashMap<BacklogTaskKey, Arc<Mutex<()>>>>>;

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
    /// task-42, post-review revision: one board drag-drop's completion
    /// report, keyed by the moved task and written by
    /// `spawn_board_move_save`'s background thread. `board::
    /// resolve_pending_moves` drains this every frame and resolves a
    /// `PendingBoardMove` only against an outcome whose `generation`
    /// matches — see `BoardMoveOutcome`'s doc (runtime/mod.rs) for why this
    /// replaced resolving off any `backlog_projects` reload.
    pub board_move_outcomes: Arc<Mutex<HashMap<BacklogTaskKey, BoardMoveOutcome>>>,
    /// task-42, post-review revision (N9): reports, per key, the
    /// `PendingBoardMove::generation` whose `spawn_board_move_save` thread
    /// has just acquired `task_write_locks`' lock and is about to run the
    /// subprocess (i.e. actually started, as opposed to still queued behind
    /// a prior same-task save). `board::resolve_pending_moves` drains this
    /// every frame and, for a matching generation, refreshes that entry's
    /// `queued_at` — so `PENDING_MOVE_TIMEOUT` measures how long the save
    /// itself has been running, not how long the drop sat queued behind an
    /// earlier same-task save's lock. Without this, a rapid second drop on
    /// the same task could have its overlay entry time out and snap back
    /// before its own save even began.
    pub board_move_started: Arc<Mutex<HashMap<BacklogTaskKey, u64>>>,
    /// task-42, post-review revision (N1/N2, second pass): one `Mutex` per
    /// task, held for the duration of every `backlog task edit` subprocess
    /// this app spawns for that task — by **all three** savers
    /// (`spawn_backlog_save`, `spawn_board_move_save`, and
    /// `spawn_backlog_bulk_save` per task inside its own loop), not just
    /// Board drops. `edit_backlog_task` shells out and blocks until the
    /// process exits, and there is no cheap way to cancel an in-flight one,
    /// so two racing writers touching the *same* task — a Board drag and a
    /// detail-rail field edit, say, or a bulk edit landing mid-drag — could
    /// otherwise complete in either order and leave on-disk state that
    /// doesn't match whichever gesture actually happened last. Because
    /// every writer takes this same lock before touching a task's file, the
    /// last save to actually acquire it — which, since writes only ever get
    /// *queued* synchronously on the UI thread, is always the last one
    /// submitted — is always the last one to touch disk, true across every
    /// writer in this app, not just among concurrent drops. First version
    /// of this field (named `board_move_locks`) only covered
    /// `spawn_board_move_save`'s own writes against each other.
    pub task_write_locks: TaskWriteLocks,
    /// Disk-derived state for every task carrying a dispatch label, refreshed
    /// by `workers::spawn_backlog`. A cache of what `dispatch_inspect`
    /// recomputes from the repo + task id, never a second source of truth —
    /// it exists only to keep `read_dir`/`metadata` calls off the render path.
    pub dispatch_runs: Arc<Mutex<HashMap<BacklogTaskKey, DispatchRun>>>,
    /// Tasks with a Refine run currently in flight (TASK-44). Purely a
    /// concurrency guard for the detail rail's Refine button, deliberately
    /// *not* a label on the task: unlike dispatch, a refine run is one
    /// bounded call whose only effect is an additive `backlog task edit`, so
    /// nothing outside this process needs to know it is happening, and
    /// nothing should be left behind on the task if the app dies mid-run.
    /// Shared rather than plain view state only because the worker thread
    /// that clears an entry cannot reach `&mut HiveApp`.
    pub refining_tasks: Arc<Mutex<BTreeSet<BacklogTaskKey>>>,
    /// The cross-repo triage overlay, refreshed alongside `backlog_projects`.
    pub ordering: Arc<Mutex<OrderingState>>,
    pub active_runs: Arc<Mutex<HashMap<i32, ActiveRun>>>,
    /// TASK-41: on-disk size per worktree, refreshed by `workers::spawn_size`
    /// on its own slow cadence (see that worker's doc for why it's not part
    /// of `meta`/the git-probe tick).
    pub sizes: Arc<Mutex<HashMap<PathBuf, WorktreeSizeEntry>>>,
    /// feat/landing-stage: why each unlanded worktree is still unlanded,
    /// refreshed by `workers::spawn_landing` on its own slow cadence (see
    /// that worker's doc for why it can't share the git-probe tick or
    /// `sizes`' own cadence's Mutex).
    pub landing: Arc<Mutex<HashMap<PathBuf, LandingEntry>>>,
    /// TASK-41: count of non-primary, clean, fully-merged worktrees, written
    /// once per git-probe tick by `workers::spawn_probe`. The top bar's "N
    /// retired worktrees" nudge reads this directly rather than recomputing
    /// it (a `repos`/`worktrees` clone + a `meta` lock) on every frame.
    pub retired_worktree_count: Arc<Mutex<usize>>,
    pub state: Arc<Mutex<ScanState>>,
    pub scanner_kick: Kick,
    pub probe_kick: Kick,
    pub detection_kick: Kick,
    pub agent_context_kick: Kick,
    pub backlog_kick: Kick,
    /// Wakes the dispatch worker (`workers::spawn_dispatch`) early — used by
    /// the per-task "Dispatch" toggle so flagging a task doesn't wait out
    /// the worker's normal poll period before the queue is drained.
    pub dispatch_kick: Kick,
    pub size_kick: Kick,
    pub landing_kick: Kick,
    pub picker: Arc<Mutex<PickerState>>,

    // Per-view feedback channels. One per UI surface so messages don't
    // overwrite each other when several actions land in the same frame.
    pub config_status: Status,
    pub kill_status: Status,
    pub server_status: Status,
    pub backlog_status: Status,
    /// Live progress for a bulk Backlog action (archive / cleanup). Separate
    /// from `backlog_status`: the status carries the completion *message*,
    /// this carries the countable position, and only one of them is showing
    /// at a time.
    pub bulk_progress: Progress,
    /// Determinate progress for the Workspace's bulk worktree removal.
    ///
    /// A second channel rather than sharing `bulk_progress`: these are two
    /// independent surfaces, and one shared value would render a Backlog bar
    /// reading "removing 3/9 worktrees" (or the reverse) whenever both were
    /// touched in a session. `Progress::begin` resets, so sharing would also
    /// let one sweep silently reset the other's bar.
    pub worktree_bulk_progress: Progress,

    // Persisted config (single source of truth for repos + UI defaults).
    pub config: Config,
    /// Overrides where `save_config` writes. Always `None` in production
    /// (`HiveApp::new`) — `save_config` falls back to the real
    /// `~/.switchbard/config.toml` in that case, same as always. Tests that
    /// exercise a real save/delete path (e.g. saved_views' Save/Delete
    /// buttons) MUST set this to an isolated temp path first; skipping it
    /// silently writes to the developer's actual config file on every test
    /// run — this is exactly how TASK-22 happened. `HiveApp::new_headless`
    /// leaves it `None`, so tests opt in explicitly rather than relying on
    /// a default that could quietly regress back to the same bug.
    pub config_save_path: Option<PathBuf>,
    /// RAII guard for the single-instance lock (`switchbard_core::
    /// instance_lock`), held only so its `Drop` removes `~/.switchbard/
    /// switchbard.lock` on exit — never read otherwise. `None` in
    /// `new_headless` (tests never take the lock) and in `new` when the
    /// lock file itself was unavailable (fails open — see
    /// `acquire_instance_lock_or_warn`). A second *live* instance is
    /// refused outright before `HiveApp` is constructed at all.
    _instance_lock: Option<InstanceLock>,

    // View-only state.
    /// When on, the workspace hides unattributed listeners.
    pub show_only_managed: bool,
    pub confirm_kill_all: bool,
    /// When Some, shows a "Remove '{name}'?" confirmation modal for the
    /// repo at the given path — set by either the "Tracked repos" panel
    /// (Servers view only, since the owner UX pass moved it there) or the
    /// Settings window's own repo list (reachable from any view); rendered
    /// unconditionally from `render_ui` so it works from either. The modal
    /// clears it on Confirm or Cancel.
    pub confirm_remove_repo: Option<(PathBuf, String)>,
    /// Owner UX pass (2026-08-05): the Settings window — repo add/remove,
    /// now that "Tracked repos" itself only renders in the Servers view.
    pub settings_open: bool,
    /// Modal state for `git worktree remove`. Shared with the worker thread
    /// so it can flip `busy`/`error` while the dialog is visible.
    pub confirm_remove_worktree: Arc<Mutex<Option<ConfirmRemoveWorktree>>>,
    /// TASK-41: modal state for the bulk "Remove N worktrees" sweep. `Arc<Mutex<>>`
    /// for the same reason as `confirm_remove_worktree` — it renders across
    /// frames while `worktree_actions::execute_bulk_remove_worktrees` (a
    /// worker thread) is running.
    pub confirm_bulk_remove_worktrees: Arc<Mutex<Option<ConfirmBulkRemoveWorktrees>>>,
    /// TASK-41: which staleness class the Workspace filter chips currently
    /// show (`All` by default). Persisted through the shared filter memory.
    pub staleness_filter: StalenessFilter,
    /// TASK-41: worktrees the user has checked for the bulk-remove sweep.
    /// View-only; cleared whenever a selected path stops being visible or
    /// after a bulk removal completes.
    pub bulk_selected_worktrees: BTreeSet<PathBuf>,
    /// Modal state for `git worktree add`.
    pub create_worktree_dialog: Arc<Mutex<Option<CreateWorktreeDialog>>>,
    /// Worker-to-UI completion queue for create operations. The worker runs
    /// git; the UI thread mutates persisted config after success.
    pub create_worktree_outcomes: Arc<Mutex<Vec<CreateWorktreeOutcome>>>,
    /// Worker-to-UI completion queue for remove operations. The worker pushes
    /// a `RemovedWorktree` on success; the UI thread prunes the alias from
    /// `config.worktrees` and persists.
    pub remove_worktree_outcomes: Arc<Mutex<Vec<RemovedWorktree>>>,
    /// Modal state for renaming the Switchbard-local worktree label.
    pub rename_worktree_dialog: Option<RenameWorktreeDialog>,
    /// The standardization offer, raised when a repo's `backlog/config.yml`
    /// omits statuses the shared vocabulary expects.
    ///
    /// UI-thread-only (a plain field, not an `Arc<Mutex<>>`): unlike the
    /// worktree dialogs, nothing here is driven by a worker — the check is a
    /// cheap comparison of two already-loaded lists, and the migration is a
    /// single small file write.
    pub status_migration_prompt: Option<StatusMigrationPrompt>,
    pub expanded_repos: BTreeSet<String>,
    /// When false (default), hide rows whose classifier verdict is NotServer
    /// (test scripts, build wrappers, ship-gate runners, etc.).
    pub show_non_servers: bool,
    pub view_tab: ViewTab,
    /// TASK-43: which in-flight dispatch run has its Kill button armed, if
    /// any. Confirm state only — one at a time, cleared on confirm/cancel —
    /// exactly like `backlog_view.dispatch_confirm` arms the Dispatch button
    /// it is the inverse of. View-only: killing a run publishes nothing here,
    /// because the run's own pipeline releases the task and the label stays
    /// the state machine (see `switchbard_core::dispatch`'s module doc).
    pub dispatch_kill_confirm: Option<BacklogTaskKey>,
    pub agent_context_view: AgentContextViewState,
    pub backlog_view: BacklogViewState,
    /// Shared render cache for the task detail pane's markdown description
    /// (task-15 AC #3). `egui_commonmark` recommends one long-lived cache
    /// rather than rebuilding it every frame.
    pub commonmark_cache: egui_commonmark::CommonMarkCache,
    /// 0 = system default; 1..=BROWSER_APP_NAMES.len() = specific browser.
    pub browser_choice: usize,
    /// First-launch discovery state. Hidden by default; flips to Scanning
    /// → Ready while the welcome modal is on screen. After dismissal it
    /// returns to Hidden permanently for this session.
    pub onboarding: Arc<Mutex<DiscoveryState>>,
    /// Optional frame/render telemetry. Enabled with `SWITCHBARD_PERF=1`.
    perf: Option<PerfSession>,
}

/// Acquire the single-instance lock, or warn and terminate the process if a
/// live instance already holds it. A lock-file I/O error (e.g. an
/// unwritable `~/.switchbard`) fails *open*: the guard is a safety net
/// against racing config saves, not a precondition for launching at all, so
/// we log and return `None` rather than block startup over it.
pub fn acquire_instance_lock_or_warn() -> Option<InstanceLock> {
    let path = instance_lock::default_path()?;
    match instance_lock::acquire(&path) {
        Ok(lock) => Some(lock),
        Err(AcquireError::AlreadyRunning(pid)) => {
            let message = format!(
                "Switchbard is already running (pid {pid}). Only one instance may run \
                 at a time — a second instance would race the first one's config saves."
            );
            eprintln!("Switchbard: {message}");
            rfd::MessageDialog::new()
                .set_title("Switchbard is already running")
                .set_description(&message)
                .set_level(rfd::MessageLevel::Warning)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
            std::process::exit(1);
        }
        Err(AcquireError::Io(e)) => {
            eprintln!("Switchbard: instance lock unavailable ({e}); continuing without it");
            None
        }
    }
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
    /// `instance_lock` is acquired by `main` (via
    /// [`acquire_instance_lock_or_warn`]) *before* eframe opens the window,
    /// so a refused second instance exits without a window flash. `new` just
    /// holds it for its `Drop`.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: Config,
        repos: Vec<Repo>,
        worktrees: Vec<WorktreeRef>,
        instance_lock: Option<InstanceLock>,
    ) -> Self {
        // Fonts are expensive to install (atlas rebuild) so this happens once,
        // here, rather than every frame; the theme's Visuals are cheap and get
        // reapplied every frame in `render_ui` so a live toggle takes effect
        // immediately.
        ui::theme::install_fonts(&cc.egui_ctx);
        ui::theme::apply(&cc.egui_ctx, cfg.ui.theme);
        // Restore the user's saved zoom before the first frame paints (eframe's
        // own zoom memory doesn't persist without the `persistence` feature).
        cc.egui_ctx.set_zoom_factor(clamp_ui_scale(cfg.ui.ui_scale));

        // Seed the first frame from the on-disk agent-context cache before any
        // worker scan completes, then start the workers against this state.
        let cached = cached_agent_contexts(&worktrees);
        let mut app = Self::new_headless(cfg, repos, worktrees);
        app._instance_lock = instance_lock;
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
        let agent_context_view = AgentContextViewState::restore_filters(&cfg.ui);
        let mut backlog_view = BacklogViewState::restore_filters(&cfg.ui);
        if backlog_view
            .selected_project
            .as_ref()
            .is_some_and(|selected| !repos.iter().any(|repo| repo.path == *selected))
        {
            backlog_view.selected_project = None;
        }
        let server_filters = cfg.ui.filters.get("servers");
        let show_only_managed = server_filters
            .and_then(|memory| memory.facets.get("attributed_only"))
            .is_some_and(|value| value == "true");
        let staleness_filter = server_filters
            .and_then(|memory| memory.facets.get("staleness"))
            .and_then(|value| StalenessFilter::from_facet(value))
            .unwrap_or_default();

        Self {
            repos: Arc::new(Mutex::new(repos)),
            worktrees: Arc::new(Mutex::new(worktrees)),
            meta: Arc::new(Mutex::new(HashMap::new())),
            services: Arc::new(Mutex::new(HashMap::new())),
            agent_contexts: Arc::new(Mutex::new(HashMap::new())),
            backlog_projects: Arc::new(Mutex::new(HashMap::new())),
            board_move_outcomes: Arc::new(Mutex::new(HashMap::new())),
            board_move_started: Arc::new(Mutex::new(HashMap::new())),
            task_write_locks: Arc::new(Mutex::new(HashMap::new())),
            dispatch_runs: Arc::new(Mutex::new(HashMap::new())),
            refining_tasks: Arc::new(Mutex::new(BTreeSet::new())),
            ordering: Arc::new(Mutex::new(OrderingState::default())),
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            sizes: Arc::new(Mutex::new(HashMap::new())),
            landing: Arc::new(Mutex::new(HashMap::new())),
            retired_worktree_count: Arc::new(Mutex::new(0)),
            state: Arc::new(Mutex::new(ScanState::default())),
            scanner_kick: Kick::new(),
            probe_kick: Kick::new(),
            detection_kick: Kick::new(),
            agent_context_kick: Kick::new(),
            backlog_kick: Kick::new(),
            dispatch_kick: Kick::new(),
            size_kick: Kick::new(),
            landing_kick: Kick::new(),
            config: cfg,
            config_save_path: None,
            _instance_lock: None,
            picker: Arc::new(Mutex::new(PickerState::Idle)),
            config_status: Status::new(),
            kill_status: Status::new(),
            server_status: Status::new(),
            backlog_status: Status::new(),
            bulk_progress: Progress::new(),
            worktree_bulk_progress: Progress::new(),
            dispatch_kill_confirm: None,
            show_only_managed,
            confirm_kill_all: false,
            confirm_remove_repo: None,
            settings_open: false,
            confirm_remove_worktree: Arc::new(Mutex::new(None)),
            confirm_bulk_remove_worktrees: Arc::new(Mutex::new(None)),
            staleness_filter,
            bulk_selected_worktrees: BTreeSet::new(),
            create_worktree_dialog: Arc::new(Mutex::new(None)),
            create_worktree_outcomes: Arc::new(Mutex::new(Vec::new())),
            remove_worktree_outcomes: Arc::new(Mutex::new(Vec::new())),
            rename_worktree_dialog: None,
            status_migration_prompt: None,
            expanded_repos: BTreeSet::new(),
            show_non_servers,
            view_tab: ViewTab::Servers,
            agent_context_view,
            backlog_view,
            commonmark_cache: egui_commonmark::CommonMarkCache::default(),
            browser_choice,
            onboarding: Arc::new(Mutex::new(DiscoveryState::default())),
            perf: PerfSession::from_env(),
        }
    }

    /// Spawn the five background workers, wiring them to this app's shared
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
                dispatch_runs: self.dispatch_runs.clone(),
                ordering: self.ordering.clone(),
                active_runs: self.active_runs.clone(),
                sizes: self.sizes.clone(),
                landing: self.landing.clone(),
                retired_worktree_count: self.retired_worktree_count.clone(),
                scanner_kick: self.scanner_kick.clone(),
                probe_kick: self.probe_kick.clone(),
                detection_kick: self.detection_kick.clone(),
                agent_context_kick: self.agent_context_kick.clone(),
                backlog_kick: self.backlog_kick.clone(),
                dispatch_kick: self.dispatch_kick.clone(),
                size_kick: self.size_kick.clone(),
                landing_kick: self.landing_kick.clone(),
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

    pub fn dispatch_runs_snapshot(&self) -> HashMap<BacklogTaskKey, DispatchRun> {
        self.dispatch_runs.lock().unwrap().clone()
    }

    /// Whether a Refine run is in flight for this task right now — what the
    /// detail rail's Refine button disables itself on. One lock, one lookup:
    /// the render path asks only about the selected task, never the whole set.
    pub fn is_refining(&self, key: &BacklogTaskKey) -> bool {
        self.refining_tasks.lock().unwrap().contains(key)
    }

    pub fn ordering_snapshot(&self) -> OrderingState {
        self.ordering.lock().unwrap().clone()
    }

    pub fn kick_all(&self) {
        self.scanner_kick.notify();
        self.probe_kick.notify();
        self.detection_kick.notify();
        self.agent_context_kick.notify();
        self.backlog_kick.notify();
        self.size_kick.notify();
        self.landing_kick.notify();
    }

    pub fn mark_agent_contexts_stale(&self) {
        for map in self.agent_contexts.lock().unwrap().values_mut() {
            map.scanned_at = None;
        }
        self.agent_context_kick.notify();
    }

    /// Save the in-memory config to disk. Reports failures via `config_status`
    /// so the user sees what happened — we don't swallow the cause.
    ///
    /// **Deliberate tradeoff — in-memory-first, no rollback:** callers mutate
    /// `self.config` directly before calling `save_config`; if the disk write
    /// fails the in-memory state has already changed and is NOT rolled back.
    /// For a local single-user app this is acceptable: the next successful save
    /// (on the next mutation) will persist the current in-memory state, and
    /// `config_status` surfaces the failure immediately so the user is never
    /// silently left with a stale file.
    pub fn save_config(&self) {
        let result = match &self.config_save_path {
            Some(path) => config::save_to(path, &self.config),
            None => config::save(&self.config),
        };
        if let Err(e) = result {
            self.config_status.set(format!("config save failed: {e}"));
        }
    }

    /// Stable key for the filter surface currently on screen. Queries live
    /// directly in persisted config, so switching views restores each page's
    /// own last-used search instead of carrying one global string everywhere.
    pub fn active_filter_key(&self) -> &'static str {
        match self.view_tab {
            ViewTab::Servers => "servers",
            ViewTab::Agents => match self.agent_context_view.section {
                AgentsSection::Context => "agents.context",
                AgentsSection::Hooks => "agents.hooks",
            },
            ViewTab::Backlog => "backlog",
            ViewTab::Dispatch => "dispatch",
        }
    }

    pub fn filter(&self) -> &str {
        self.config
            .ui
            .filters
            .get(self.active_filter_key())
            .map_or("", |memory| memory.query.as_str())
    }

    pub fn filter_mut(&mut self) -> &mut String {
        let key = self.active_filter_key().to_string();
        &mut self.config.ui.filters.entry(key).or_default().query
    }

    fn persist_filter_facets(&mut self) {
        self.agent_context_view.persist_filters(&mut self.config.ui);
        self.backlog_view.persist_filters(&mut self.config.ui);
        let servers = self
            .config
            .ui
            .filters
            .entry("servers".to_string())
            .or_default();
        if self.show_only_managed {
            servers
                .facets
                .insert("attributed_only".to_string(), "true".to_string());
        } else {
            servers.facets.remove("attributed_only");
        }
        if self.staleness_filter == StalenessFilter::All {
            servers.facets.remove("staleness");
        } else {
            servers.facets.insert(
                "staleness".to_string(),
                self.staleness_filter.facet_value().to_string(),
            );
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

    /// Common tail for any mutation of `config.repos`: persist, re-derive
    /// the runtime worktree list, wake all workers, and show a status line.
    ///
    /// Intentional deviations from the pre-refactor per-method behaviour:
    /// - `remove_repo` previously kicked the scanner only; it now does
    ///   `kick_all()` so probe/detection/agent-context caches are pruned
    ///   immediately rather than waiting for their next scheduled tick.
    /// - `move_repo` does NOT call this helper (see its doc-comment) because
    ///   reordering leaves the worktree *set* unchanged; kicking workers for a
    ///   purely visual reorder would be noise.
    fn after_repos_mutation(&self, status: impl Into<String>) {
        self.save_config();
        self.rebuild_worktrees();
        self.kick_all();
        self.config_status.set(status);
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
        self.after_repos_mutation(format!("added '{name}'"));
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
        self.after_repos_mutation(format!("removed '{}'", repo_path.display()));
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
        // The shared safety facts, probed fresh at open time like everything
        // else here. The dialog renders the same check list the Workspace row
        // shows on hover, so a user who saw "remove blocked" on the row reads
        // the identical sentence here rather than a second opinion.
        let removal_facts = probe_facts(
            &repo_path,
            &worktree_path,
            branch.as_deref(),
            Fact::Known(self.attached_processes(&worktree_path)),
        );
        *self.confirm_remove_worktree.lock().unwrap() = Some(ConfirmRemoveWorktree {
            repo_path,
            worktree_path,
            branch,
            dirty_files,
            active_runs,
            branch_assessment,
            removal_facts,
            delete_branch: false,
            busy: false,
            error: None,
        });
    }

    /// Active runs whose `worktree_path` matches, projected to the lightweight
    /// summary the dialog renders. Used at dialog-open time AND at confirm
    /// time so the worker thread can detect drift before signaling anything.
    /// Everything currently holding on to `worktree_path`, gathered from the
    /// three independent places that know: the port scanner's attribution,
    /// this instance's started services, and the dispatch run table.
    ///
    /// The single source for the confirm dialogs' `NoProcesses` check. The
    /// dispatch third is the one the old listener-only check could not see at
    /// all: a dispatched agent writes into a worktree without necessarily
    /// listening on any port and without having been started by
    /// `spawn_start`, so a run in flight read as "nothing running here".
    pub(crate) fn attached_processes(&self, worktree_path: &Path) -> AttachedProcesses {
        AttachedProcesses {
            listeners: self
                .state
                .lock()
                .unwrap()
                .listeners
                .iter()
                .filter(|l| l.worktree_path.as_deref() == Some(worktree_path))
                .count(),
            switchbard_runs: self
                .active_runs
                .lock()
                .unwrap()
                .values()
                .filter(|r| r.worktree_path == worktree_path)
                .count(),
            dispatch_runs: self
                .dispatch_runs
                .lock()
                .unwrap()
                .values()
                .filter(|r| r.worktree_path == worktree_path)
                .filter(|r| dispatch_run_holds_worktree(&r.liveness))
                .count(),
        }
    }

    pub(crate) fn snapshot_runs_for_worktree(&self, worktree_path: &Path) -> Vec<ActiveRunSummary> {
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
        let size_kick = self.size_kick.clone();
        let landing_kick = self.landing_kick.clone();
        let config_status = self.config_status.clone();
        let remove_outcomes = self.remove_worktree_outcomes.clone();
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

                    // Queue the alias prune for the UI thread; config is owned
                    // directly by HiveApp and cannot be touched from here.
                    remove_outcomes.lock().unwrap().push(
                        crate::worktree_actions::RemovedWorktree {
                            repo_path: snapshot.repo_path.clone(),
                            worktree_path: snapshot.worktree_path.clone(),
                        },
                    );

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
                    size_kick.notify();
                    landing_kick.notify();
                }
                Err(e) => {
                    if let Some(state) = confirm.lock().unwrap().as_mut() {
                        state.busy = false;
                        state.error = Some(crate::worktree_actions::removal_error_message(
                            killed,
                            &e.to_string(),
                        ));
                    }
                }
            }
            ctx.request_repaint();
        });
    }

    /// Move the repo at index `i` up (delta = -1) or down (delta = 1). Saves
    /// the new order to `~/.switchbard/config.toml` and refreshes the runtime view
    /// so the sidebar / per-repo sections re-render in the new order.
    ///
    /// Intentionally does NOT call `after_repos_mutation`: reordering leaves
    /// the worktree *set* unchanged, so kicking workers would be pure noise.
    /// `rebuild_worktrees` is still needed to update the runtime order for the
    /// UI; `save_config` persists the swap; a status note surfaces the change.
    pub fn move_repo(&mut self, i: usize, delta: isize) {
        let len = self.config.repos.len();
        let j = (i as isize + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
        if i == j {
            return;
        }
        self.config.repos.swap(i, j);
        self.save_config();
        self.rebuild_worktrees();
        self.config_status.set("reordered repos");
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

    /// TASK-43: pull the plug on one in-flight dispatch run by signalling the
    /// process group recorded in its pgid sidecar.
    ///
    /// Deliberately *only* a signal. Nothing here touches the task's labels or
    /// notes: `dispatch_one` is blocked in `wait_for_exit` on this exact
    /// process, so killing it makes that wait return and the pipeline walks
    /// its ordinary failure path — `dispatch-failed` plus a note — on its own.
    /// Bookkeeping from this side would be a second writer racing the first.
    ///
    /// **No pgid is passed in, deliberately.** The run's identity is
    /// re-established *on this thread, immediately before signalling*, by
    /// `kill_dispatch_run` (which re-runs `dispatch_inspect::probe_liveness`,
    /// the same authenticated path the worker uses). The verdict cached on a
    /// `DispatchRun` is up to `BACKLOG_PERIOD` × `UNFOCUSED_BACKOFF_MULTIPLIER`
    /// old — roughly four minutes with the window in the background — and
    /// within that window the agent can exit and the OS can reissue its
    /// process group id to something unrelated. Signalling a cached number is
    /// the sidecar-authentication failure with a shorter fuse; the cached
    /// verdict is fit to decide whether to *render* a button, never to decide
    /// what to *signal*.
    ///
    /// A run that no longer authenticates is reported and left alone, so the
    /// two answers this can give are "killed it" and "nothing killed" — never
    /// "killed something". Supervision is read from the fresh probe too: when
    /// the agent outlived the Switchbard that spawned it, the kill stops the
    /// agent and nothing else (no release, no note, the task stays on
    /// `dispatching`) and the status message says so rather than letting the
    /// user infer a bookkeeping step that will never happen.
    ///
    /// The `backlog_kick` afterwards refreshes the labels the pipeline
    /// rewrites in response, the same wake the dispatch worker uses for its
    /// own outcomes.
    pub fn spawn_kill_dispatch(&self, task_id: String, started_at_unix: u64, ctx: &egui::Context) {
        let kick = self.backlog_kick.clone();
        let status = self.backlog_status.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let outcome = kill_dispatch_run(&task_id, started_at_unix, Duration::from_secs(3));
            status.set(outcome.describe(&task_id));
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

    /// Save one task's edit through the real `backlog` CLI, on a background
    /// thread. Shares `save_one_task` (edit → reload → stale-aware status)
    /// with `spawn_board_move_save`'s single-task save (N1/N2, post-review
    /// revision — the two used to duplicate that sequence near-verbatim),
    /// and takes this task's `task_write_locks` entry the same way every
    /// other saver does, so this can't race a concurrent Board drag or bulk
    /// edit on the same task.
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
        let locks = self.task_write_locks.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let key = (project_root.clone(), task_id.clone());
            let task_lock = task_write_lock(&locks, &key);
            let _guard = lock_task(&task_lock);
            save_one_task(&project_root, &task_id, &patch, &projects, &status, &kick);
            ctx.request_repaint();
        });
    }

    /// The Board lens's dedicated drag-drop save (task-42, post-review
    /// revision) — deliberately **not** just a call to `spawn_backlog_save`
    /// above, because a drag-drop needs one thing that path doesn't
    /// provide on its own: a completion report keyed to *this specific
    /// drop* (`generation`, into `board_move_outcomes`, consumed by
    /// `board::resolve_pending_moves` — see `BoardMoveOutcome`'s doc). The
    /// actual edit/reload/status work is `save_one_task`, shared with
    /// `spawn_backlog_save`; the per-task write lock
    /// (`HiveApp::task_write_locks`) is the same one every saver takes.
    ///
    /// `key` and `generation` together identify exactly which
    /// `PendingBoardMove` this save is for; `board::apply_drop` is the only
    /// caller and always passes the same pair it just stamped the overlay
    /// entry with.
    pub fn spawn_board_move_save(
        &self,
        project_root: PathBuf,
        task_id: String,
        patch: BacklogTaskPatch,
        key: BacklogTaskKey,
        generation: u64,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let outcomes = self.board_move_outcomes.clone();
        let started = self.board_move_started.clone();
        let locks = self.task_write_locks.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let task_lock = task_write_lock(&locks, &key);
            // Holds this task's lock for the whole subprocess call, so a
            // second drop on the same task (already queued behind this one)
            // can't run its own `edit_backlog_task` concurrently — see
            // `task_write_locks`'s doc on `HiveApp` for why that matters
            // (N1/N2: every writer takes this same lock, not just Board
            // drops against each other).
            let _guard = lock_task(&task_lock);
            // N9: this generation's save has now actually started (lock
            // acquired, about to run the subprocess) — see
            // `board_move_started`'s doc on `HiveApp`.
            started.lock().unwrap().insert(key.clone(), generation);
            let success = save_one_task(&project_root, &task_id, &patch, &projects, &status, &kick);
            // N8: record the outcome *before* releasing the lock — closes
            // the window where a second, newer same-task save (already
            // queued behind this lock) could acquire it, finish, and report
            // its own outcome first, only for this now-stale report to land
            // a moment later and overwrite it.
            outcomes.lock().unwrap().insert(
                key,
                BoardMoveOutcome {
                    generation,
                    success,
                },
            );
            drop(_guard);
            ctx.request_repaint();
        });
    }

    /// Bulk-edit a shared patch (status/priority/labels) across many tasks,
    /// on a background thread. Unlike `spawn_backlog_save`/
    /// `spawn_board_move_save`, this deliberately does **not** call
    /// `save_one_task` per task in the loop below — reloading and
    /// re-parsing the whole project after every individual edit in an
    /// n-task batch would be an O(n) reload done O(n) times, so this
    /// aggregates one reload/status/kick for the whole batch instead. It
    /// does still take each task's `task_write_locks` entry before editing
    /// it (N1/N2, post-review revision), so a bulk edit landing on a task
    /// that a Board drag or a detail-rail edit is also mid-write on still
    /// serializes correctly against them (both those savers reach the same
    /// lock via `save_one_task`) — only the post-write reload/status/kick
    /// bundling stays batch-specific to this method.
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
        let locks = self.task_write_locks.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let total = task_ids.len();
            let mut saved = 0usize;
            let mut first_error: Option<String> = None;
            for task_id in &task_ids {
                let key = (project_root.clone(), task_id.clone());
                let task_lock = task_write_lock(&locks, &key);
                let _guard = lock_task(&task_lock);
                match switchbard_core::edit_backlog_task(&project_root, task_id, &patch) {
                    Ok(_) => saved += 1,
                    Err(e) => {
                        if first_error.is_none() {
                            first_error = Some(format!("{task_id}: {e}"));
                        }
                    }
                }
            }
            let mut reload = Ok(());
            if saved > 0 {
                reload = refresh_backlog_project_cache(&projects, &project_root);
                kick.notify();
            }
            let summary = match first_error {
                Some(error) => {
                    format!("{action_label}: saved {saved}/{total} tasks; first failure: {error}")
                }
                None => format!("{action_label}: updated {saved} task(s)"),
            };
            status.set(with_stale_warning(reload, summary));
            ctx.request_repaint();
        });
    }

    /// Per-task opt-in for the dispatch pipeline: adds or removes
    /// `dispatch::DISPATCH_LABEL` via `set_backlog_label` (a targeted
    /// add/remove, not a full labels replace — see that function's doc).
    /// Also wakes the dispatch worker on enable, so flagging a task doesn't
    /// sit waiting out the worker's normal poll period before it's picked
    /// up. Strictly opt-in: this only ever touches the one label the user
    /// clicked; the worker (`workers::spawn_dispatch`) owns everything after
    /// that (claim, worktree, headless run, PR).
    pub fn spawn_backlog_dispatch_toggle(
        &self,
        project_root: PathBuf,
        task_id: String,
        enabled: bool,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let backlog_kick = self.backlog_kick.clone();
        let dispatch_kick = self.dispatch_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::set_backlog_label(
                &project_root,
                &task_id,
                switchbard_core::DISPATCH_LABEL,
                enabled,
            ) {
                Ok(_) => {
                    let reload = refresh_backlog_project_cache(&projects, &project_root);
                    let verb = if enabled { "flagged" } else { "unflagged" };
                    status.set(with_stale_warning(
                        reload,
                        format!("{verb} {task_id} for dispatch"),
                    ));
                    backlog_kick.notify();
                    if enabled {
                        dispatch_kick.notify();
                    }
                }
                Err(e) => status.set(format!("dispatch flag for {task_id} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    /// TASK-44 grooming pass: hand one task's current content to a headless,
    /// read-only `claude -p` run and apply what comes back additively
    /// (`switchbard_core::refine_task` owns the whole contract — prompt,
    /// timeout, strict parse, additive merge, single `backlog task edit`).
    ///
    /// Same fire-and-forget shape as every other `spawn_backlog_*` here, with
    /// one addition: the task is inserted into `refining_tasks` *before* the
    /// thread starts and removed when it exits, so the button can disable
    /// itself for the duration. The insert is also the real guard — the
    /// disabled button is UI affordance, this is what makes a second run
    /// impossible if a click ever slips through.
    ///
    /// Nothing is written to the task on any failure path; `refine_task`
    /// parses and merges before it touches the CLI, so the status line is the
    /// only thing a failed run changes.
    pub fn spawn_backlog_refine(
        &self,
        project_root: PathBuf,
        task_id: String,
        ctx: &egui::Context,
    ) {
        let key: BacklogTaskKey = (project_root.clone(), task_id.clone());
        {
            let mut in_flight = self.refining_tasks.lock().unwrap();
            if !in_flight.insert(key.clone()) {
                return;
            }
        }
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let refining = self.refining_tasks.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        status.set(format!("refining {task_id}…"));
        thread::spawn(move || {
            let lease = RefineLease {
                tasks: refining,
                key,
            };
            let msg = match load_refine_target(&projects, &project_root, &task_id) {
                Some(task) => {
                    match switchbard_core::refine_task(
                        &project_root,
                        &task,
                        &switchbard_core::RefineOptions::default(),
                    ) {
                        Ok(outcome) => {
                            let reload = refresh_backlog_project_cache(&projects, &project_root);
                            kick.notify();
                            with_stale_warning(
                                reload,
                                switchbard_core::describe_refine_outcome(&outcome),
                            )
                        }
                        Err(e) => format!("refine {task_id} failed to start: {e}"),
                    }
                }
                None => format!("refine {task_id} failed: task not found in the loaded project"),
            };
            status.set(msg);
            // Released *before* the repaint so the button re-enables in the
            // same frame that shows the result, rather than waiting for
            // whatever happens to request the next one.
            drop(lease);
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
                    let reload = refresh_backlog_project_cache(&projects, &project_root);
                    let verb = if checked { "checked" } else { "unchecked" };
                    status.set(with_stale_warning(
                        reload,
                        format!("{verb} {task_id} AC #{index}"),
                    ));
                    kick.notify();
                }
                Err(e) => status.set(format!("update {task_id} AC #{index} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    pub fn spawn_backlog_dod_toggle(
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
            match switchbard_core::set_backlog_dod_checked(&project_root, &task_id, index, checked)
            {
                Ok(_) => {
                    let reload = refresh_backlog_project_cache(&projects, &project_root);
                    let verb = if checked { "checked" } else { "unchecked" };
                    status.set(with_stale_warning(
                        reload,
                        format!("{verb} {task_id} DoD #{index}"),
                    ));
                    kick.notify();
                }
                Err(e) => status.set(format!("update {task_id} DoD #{index} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    pub fn spawn_backlog_archive(
        &self,
        project_root: PathBuf,
        task_id: String,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::archive_backlog_task(&project_root, &task_id) {
                Ok(_) => {
                    let reload = refresh_backlog_project_cache(&projects, &project_root);
                    status.set(with_stale_warning(reload, format!("archived {task_id}")));
                    kick.notify();
                }
                Err(e) => status.set(format!("archive {task_id} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    /// The Done-task counterpart to `spawn_backlog_archive` — `detail_lists::
    /// render_archive` routes here instead of `spawn_backlog_archive` when
    /// the task is Done, since the real CLI refuses `task archive` on one.
    pub fn spawn_backlog_complete(
        &self,
        project_root: PathBuf,
        task_id: String,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::complete_backlog_task(&project_root, &task_id) {
                Ok(_) => {
                    let reload = refresh_backlog_project_cache(&projects, &project_root);
                    status.set(with_stale_warning(reload, format!("completed {task_id}")));
                    kick.notify();
                }
                Err(e) => status.set(format!("complete {task_id} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    /// "Clean Up Old Tasks" (QA parity matrix LOW gap): complete every Done
    /// task in `per_project` — one `complete_backlog_task` call per task
    /// (not `archive_backlog_task`; the real CLI refuses `task archive` on
    /// a Done task, a defect the 2026-08-05 re-verification caught), across
    /// however many projects the caller found Done tasks in. Mirrors
    /// `spawn_backlog_bulk_save`'s per-project loop shape; the difference is
    /// this always spans every tracked project rather than one bulk
    /// selection, since "clean up" is a workspace-wide housekeeping action,
    /// not scoped to whatever the user happens to be filtering by.
    pub fn spawn_backlog_cleanup(
        &self,
        per_project: Vec<(PathBuf, Vec<String>)>,
        ctx: &egui::Context,
    ) {
        if per_project.is_empty() {
            return;
        }
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let progress = self.bulk_progress.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let project_count = per_project.len();
            let total: usize = per_project.iter().map(|(_, ids)| ids.len()).sum();
            progress.begin("completing", total);
            ctx.request_repaint();
            let mut completed = 0usize;
            let mut first_error: Option<String> = None;
            let mut first_reload_error: Result<(), String> = Ok(());
            for (project_root, task_ids) in &per_project {
                let mut touched = false;
                for task_id in task_ids {
                    // Every candidate here is Done (`cleanup_candidates`
                    // filters on `task.is_done()`) — `complete_backlog_task`,
                    // not `archive_backlog_task`: the real CLI refuses
                    // `task archive` on a Done task ("should be completed,
                    // not archived"). See detail_lists::render_archive's doc
                    // comment for the single-task equivalent.
                    match switchbard_core::complete_backlog_task(project_root, task_id) {
                        Ok(_) => {
                            completed += 1;
                            touched = true;
                        }
                        Err(e) => {
                            if first_error.is_none() {
                                first_error = Some(format!("{task_id}: {e}"));
                            }
                        }
                    }
                    progress.advance();
                    ctx.request_repaint();
                }
                if touched {
                    let reload = refresh_backlog_project_cache(&projects, project_root);
                    if first_reload_error.is_ok() {
                        first_reload_error = reload;
                    }
                }
            }
            if completed > 0 {
                kick.notify();
            }
            let summary = match first_error {
                Some(error) => format!(
                    "cleaned up {completed}/{total} Done tasks across {project_count} projects; first failure: {error}"
                ),
                None => format!(
                    "cleaned up {completed}/{total} Done tasks across {project_count} projects"
                ),
            };
            progress.finish();
            status.set(with_stale_warning(first_reload_error, summary));
            ctx.request_repaint();
        });
    }

    /// Clear a batch off the active board, routing each task to the
    /// disposition Backlog.md actually defines for it: Done tasks are
    /// *completed* into `backlog/completed/`, everything else is *archived*
    /// into `backlog/archive/tasks/`.
    ///
    /// One worker rather than two because it is one user action with one
    /// progress bar and one summary. Splitting it would make a mixed batch
    /// report twice and race its own two halves through the same reload.
    ///
    /// Per-task failures do not abort the batch: a task the CLI rejects
    /// leaves the rest to succeed, and the first failure is reported.
    /// Reporting `N/M` rather than a bare success is the point — a partial
    /// sweep must not read as a complete one.
    pub fn spawn_backlog_bulk_clear(
        &self,
        archive: Vec<(PathBuf, Vec<String>)>,
        complete: Vec<(PathBuf, Vec<String>)>,
        ctx: &egui::Context,
    ) {
        let total: usize = archive
            .iter()
            .chain(complete.iter())
            .map(|(_, ids)| ids.len())
            .sum();
        if total == 0 {
            return;
        }
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let kick = self.backlog_kick.clone();
        let progress = self.bulk_progress.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let verb = match (archive.is_empty(), complete.is_empty()) {
                (true, _) => "completing",
                (_, true) => "archiving",
                _ => "clearing",
            };
            progress.begin(verb, total);
            ctx.request_repaint();

            let mut archived = 0usize;
            let mut completed = 0usize;
            let mut first_error: Option<String> = None;
            let mut first_reload_error: Result<(), String> = Ok(());
            let mut touched_roots: BTreeSet<PathBuf> = BTreeSet::new();

            // `false` = archive, `true` = complete. Both arms are the same
            // loop over the same shape; only the CLI verb differs.
            for (is_complete, per_project) in [(false, &archive), (true, &complete)] {
                for (project_root, task_ids) in per_project {
                    for task_id in task_ids {
                        let outcome = if is_complete {
                            switchbard_core::complete_backlog_task(project_root, task_id)
                        } else {
                            switchbard_core::archive_backlog_task(project_root, task_id)
                        };
                        match outcome {
                            Ok(_) => {
                                if is_complete {
                                    completed += 1;
                                } else {
                                    archived += 1;
                                }
                                touched_roots.insert(project_root.clone());
                            }
                            Err(e) => {
                                if first_error.is_none() {
                                    first_error = Some(format!("{task_id}: {e}"));
                                }
                            }
                        }
                        // Advances on failure too: this measures position in
                        // the batch, not how much of it worked.
                        progress.advance();
                        ctx.request_repaint();
                    }
                }
            }

            // Reloaded once per project after *both* arms, not per arm — a
            // mixed batch touching one repo would otherwise reload it twice.
            for project_root in &touched_roots {
                let reload = refresh_backlog_project_cache(&projects, project_root);
                if first_reload_error.is_ok() {
                    first_reload_error = reload;
                }
            }
            if archived + completed > 0 {
                kick.notify();
            }

            let moved = match (archived, completed) {
                (0, c) => format!("completed {c}"),
                (a, 0) => format!("archived {a}"),
                (a, c) => format!("archived {a} · completed {c}"),
            };
            let summary = match first_error {
                Some(error) => format!("{moved} of {total} tasks; first failure: {error}"),
                None => format!("{moved} of {total} tasks"),
            };
            progress.finish();
            status.set(with_stale_warning(first_reload_error, summary));
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
                    let reload = refresh_backlog_project_cache(&projects, &project_root);
                    status.set(with_stale_warning(
                        reload,
                        format!("appended note to {task_id}"),
                    ));
                    kick.notify();
                }
                Err(e) => status.set(format!("append note to {task_id} failed: {e}")),
            }
            ctx.request_repaint();
        });
    }

    /// TASK-28 (owner-found bug): `create_backlog_task`'s raw stdout is the
    /// *entire* newly-created task's rendered form — file path, a `====`
    /// underline, every section header, even when empty — not a one-line
    /// confirmation like `task archive`'s. That used to land verbatim in
    /// `backlog_status`, stretching the top bar into a many-line void.
    /// Builds a compact "Created {repo}:{id}" instead, the same way every
    /// other mutation status in this file already discards raw CLI stdout
    /// (see `spawn_backlog_save`'s `Ok(_) => ...`) — this was the one
    /// exception. `ui::components::action_status_label` is the defense in
    /// depth for whatever future case still slips through: no status
    /// message renders unbounded, regardless of what built it.
    pub fn spawn_backlog_create(
        &self,
        project_root: PathBuf,
        task: NewBacklogTask,
        ctx: &egui::Context,
    ) {
        let status = self.backlog_status.clone();
        let projects = self.backlog_projects.clone();
        let repos = self.repos.clone();
        let kick = self.backlog_kick.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            match switchbard_core::create_backlog_task(&project_root, &task) {
                Ok(output) => {
                    let reload = refresh_backlog_project_cache(&projects, &project_root);
                    let repo_label = repos
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|repo| repo.path == project_root)
                        .map(|repo| repo.name.clone())
                        .unwrap_or_else(|| project_root.display().to_string());
                    let msg = match switchbard_core::parse_created_task_id(&output) {
                        Some(task_id) => format!("Created {repo_label}:{task_id}"),
                        None => format!("created task in {repo_label}"),
                    };
                    status.set(with_stale_warning(reload, msg));
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
pub fn state_drifted(
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
pub fn runs_drifted(original: &[ActiveRunSummary], fresh: &[ActiveRunSummary]) -> bool {
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
pub fn delete_branch_after_removal(
    snapshot: &ConfirmRemoveWorktree,
    branch: Option<&str>,
) -> String {
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
    pub fn render_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        let frame_start = Instant::now();
        if let Some(perf) = &mut self.perf {
            perf.begin_frame();
        }
        // Cheap (Visuals swap only, no font-atlas work) — reapplied every
        // frame so the theme toggle in the top bar takes effect immediately.
        ui::theme::apply(ctx, self.config.ui.theme);
        self.drain_create_worktree_outcomes();
        self.drain_remove_worktree_outcomes();

        let top_start = Instant::now();
        ui::top_bar::render(self, ui);
        if let Some(perf) = &mut self.perf {
            perf.record_top_bar(top_start.elapsed());
        }

        // Owner UX pass (2026-08-05): "Tracked repos" is now Servers-local
        // (a left-side panel, not the global right-side one it used to be)
        // — repo add/remove for every other view goes through the Settings
        // window instead (`ui::settings`). Side panels must still render
        // BEFORE the central panel so they claim their docked space first;
        // otherwise the central panel sizes to the full window and the side
        // panel overlays it. The Backlog view's own detail rail (also a
        // right-side panel) follows the identical ordering rule, inside
        // `ui::backlog::render` itself (it needs the same `Snapshot`/
        // `Pending` the lens content does).
        let sidebar_start = Instant::now();
        if self.view_tab == ViewTab::Servers {
            ui::sidebar::render(self, ui);
        }
        if let Some(perf) = &mut self.perf {
            perf.record_sidebar(sidebar_start.elapsed());
        }

        let central_start = Instant::now();
        match self.view_tab {
            ViewTab::Servers => ui::workspace::render(self, ui),
            ViewTab::Agents => ui::agents::render(self, ui),
            ViewTab::Dispatch => ui::dispatch::render(self, ui),
            ViewTab::Backlog => ui::backlog::render(self, ui),
        }
        let central_elapsed = central_start.elapsed();
        if let Some(perf) = &mut self.perf {
            perf.record_central(central_elapsed);
            if self.view_tab == ViewTab::Servers {
                perf.record_workspace(central_elapsed);
            }
        }

        // Reachable from any view (not just Servers, where the repo list
        // itself now lives) and rendered unconditionally so it works no
        // matter which tab triggered it.
        ui::settings::render_settings_window(self, ui);
        ui::sidebar::render_remove_confirmation(self, ui);

        // Onboarding overlay paints last so it sits on top of everything
        // else when shown. It no-ops when already dismissed.
        let onboarding_start = Instant::now();
        ui::onboarding::render(self, ui);
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
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(summary.overlay_text())
                            .monospace()
                            .color(ui.visuals().text_color()),
                    );
                });
        });
}

/// Returns (creating it if needed) the shared per-task serialization lock
/// for `key` — see `HiveApp::task_write_locks`'s doc for why every
/// `backlog` CLI writer in this app takes this same lock before touching a
/// task's file. The per-task `Arc<Mutex<()>>` entries this map accumulates
/// are never removed (N6: both this map and `HiveApp::board_move_outcomes`/
/// `board_move_started` are bounded by the number of distinct tasks ever
/// written to in this run, not by anything unbounded — acceptable, the same
/// trade-off `HiveApp::dispatch_runs`/`sizes` already make for other
/// per-task maps in this app).
fn task_write_lock(locks: &TaskWriteLocks, key: &BacklogTaskKey) -> Arc<Mutex<()>> {
    locks
        .lock()
        .unwrap()
        .entry(key.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Lock a per-task write lock, tolerating poisoning (N7, post-review
/// revision): a prior save panicking mid-edit must not permanently and
/// silently block every future write to that task. The guard protects no
/// real state (`()`), so recovering it from a poisoned lock is safe — the
/// alternative (propagating the panic here too, via a bare `.unwrap()`)
/// would strand every subsequent save for that task behind a lock nothing
/// could ever acquire again.
fn lock_task(task_lock: &Arc<Mutex<()>>) -> std::sync::MutexGuard<'_, ()> {
    task_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Shared core of a single-task `backlog` CLI save (N1/N2, post-review
/// revision — previously duplicated near-verbatim between
/// `spawn_backlog_save` and `spawn_board_move_save`): run
/// `edit_backlog_task`, reload the project cache either way, and set the
/// stale-aware status message. Returns whether the edit itself succeeded —
/// `spawn_board_move_save` needs that for its own outcome report;
/// `spawn_backlog_save` just discards it.
///
/// Used by `spawn_backlog_save` and `spawn_board_move_save` — the two
/// savers that each touch exactly one task and want that edit's outcome
/// reflected immediately (reload + status + wake the backlog worker on
/// success). `spawn_backlog_bulk_save` deliberately does *not* call this
/// per task in its own loop — see that method's own doc for why.
fn save_one_task(
    project_root: &Path,
    task_id: &str,
    patch: &BacklogTaskPatch,
    projects: &Arc<Mutex<HashMap<PathBuf, BacklogProject>>>,
    status: &Status,
    kick: &Kick,
) -> bool {
    match switchbard_core::edit_backlog_task(project_root, task_id, patch) {
        Ok(_) => {
            let reload = refresh_backlog_project_cache(projects, project_root);
            status.set(with_stale_warning(reload, format!("saved {task_id}")));
            kick.notify();
            true
        }
        Err(e) => {
            // Reload even on failure: the edit itself didn't happen, but
            // the view's cached snapshot could still be stale for an
            // unrelated reason (e.g. an external edit), and
            // `with_stale_warning` already has to say so out loud if this
            // reload itself fails — no reason to only make that check on
            // the success path. `board::resolve_pending_moves`'s
            // wall-clock-timeout fallback also reads whatever
            // `backlog_projects` currently holds, so a stale snapshot there
            // is a worse failure mode than a status line that also says the
            // reload failed.
            let reload = refresh_backlog_project_cache(projects, project_root);
            status.set(with_stale_warning(
                reload,
                format!("save {task_id} failed: {e}"),
            ));
            false
        }
    }
}

/// Re-read one project straight after a mutation so the UI reflects the edit
/// without waiting out `workers::spawn_backlog`'s poll period.
///
/// Returns the reload failure rather than swallowing it. A dropped error here
/// is invisible in the worst way: the `backlog` CLI write succeeded, so the
/// status bar says "saved", while the cache the views render from still holds
/// the pre-mutation snapshot — the user sees their edit apparently do nothing
/// and has no clue why. Callers pair this with [`with_stale_warning`].
pub(crate) fn refresh_backlog_project_cache(
    projects: &Arc<Mutex<HashMap<PathBuf, BacklogProject>>>,
    project_root: &Path,
) -> Result<(), String> {
    match load_backlog_project(project_root) {
        Ok(project) => {
            projects
                .lock()
                .unwrap()
                .insert(project_root.to_path_buf(), project);
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Clears a task's `HiveApp::refining_tasks` entry on **every** exit path,
/// including a panic inside the worker thread.
///
/// Without this the guard could outlive the run it guards: a thread that
/// panicked between the insert and the remove would leave that task's Refine
/// button disabled for the rest of the session, with no run in flight and no
/// way to clear it short of a restart. A `Drop` impl is the only construct
/// that survives unwinding, so the "cannot stack" property and the
/// "eventually re-enables" property come from the same place.
struct RefineLease {
    tasks: Arc<Mutex<BTreeSet<BacklogTaskKey>>>,
    key: BacklogTaskKey,
}

impl Drop for RefineLease {
    fn drop(&mut self) {
        // Deliberately not `.lock().unwrap()` like the rest of this file: a
        // panic inside a `Drop` that is itself running during unwinding
        // aborts the process. Recovering the set through the poison is both
        // safe (a `BTreeSet` of keys has no invariant a panic could break)
        // and strictly better than taking the whole app down over a
        // bookkeeping remove.
        let mut tasks = match self.tasks.lock() {
            Ok(tasks) => tasks,
            Err(poisoned) => poisoned.into_inner(),
        };
        tasks.remove(&self.key);
    }
}

/// The task a Refine run should read its current content from, taken from
/// the cache the views render — not from the click's own captured copy.
/// `switchbard_core::refine_task` needs the *latest* description/criteria/plan
/// to build a prompt and an additive merge against; a `BacklogTask` cloned at
/// click time could already be stale by the time the thread runs (a
/// concurrent AC toggle, a background reload), and merging against a stale
/// copy is how an "additive" merge quietly re-appends something.
fn load_refine_target(
    projects: &Arc<Mutex<HashMap<PathBuf, BacklogProject>>>,
    project_root: &Path,
    task_id: &str,
) -> Option<switchbard_core::BacklogTask> {
    projects
        .lock()
        .unwrap()
        .get(project_root)?
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
}

/// Compose a mutation's success message with the [`refresh_backlog_project_cache`]
/// outcome. The write really did succeed, so the message still says so — but a
/// failed reload means the visible snapshot is stale, and that has to be said
/// out loud rather than left for the user to infer from a view that didn't move.
fn with_stale_warning(reload: Result<(), String>, success: String) -> String {
    match reload {
        Ok(()) => success,
        Err(e) => format!("{success} — view may be stale, reload failed: {e}"),
    }
}

impl eframe::App for HiveApp {
    // egui 0.36 inverted the app loop: the trait hands us a `&mut Ui` for the
    // whole window rather than the `&Context` 0.31's `update` took, because
    // panels are now nested inside a parent `Ui` instead of claiming screen
    // space off the context. `Window`/`Area` still take a `&Context`, so the
    // handle is cloned out once here (it is an `Arc` internally — cheap) and
    // passed alongside, rather than re-borrowed from `ui` at each call site
    // where the borrow checker would fight the `&mut Ui` the panels need.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_picker();

        // Snapshot persistable UI state so we can save the config if any
        // toggle was flipped this update. `theme` and `sidebar_collapsed`
        // (TASK-27) live directly on `config.ui` (their toggles mutate in
        // place), so they're tracked the same way `ui_scale` is below
        // rather than mirrored through `save_ui_to_config`.
        let ui_before = (self.browser_choice, self.show_non_servers);
        let theme_before = self.config.ui.theme;
        let sidebar_collapsed_before = self.config.ui.sidebar_collapsed;
        let filters_before = self.config.ui.filters.clone();

        self.render_ui(ui);
        self.persist_filter_facets();

        // Capture the live zoom (top-bar stepper or ⌘+/⌘−/⌘0) so it survives a
        // restart. egui's keyboard zoom lands one frame late, which the next
        // frame's read-back picks up — invisible to the user.
        let zoom = ctx.zoom_factor();
        let zoom_changed = (zoom - self.config.ui.ui_scale).abs() > f32::EPSILON;
        if zoom_changed {
            self.config.ui.ui_scale = zoom;
        }

        let ui_after = (self.browser_choice, self.show_non_servers);
        let theme_changed = self.config.ui.theme != theme_before;
        let sidebar_collapsed_changed =
            self.config.ui.sidebar_collapsed != sidebar_collapsed_before;
        let filters_changed = self.config.ui.filters != filters_before;
        if ui_before != ui_after
            || zoom_changed
            || theme_changed
            || sidebar_collapsed_changed
            || filters_changed
        {
            self.save_ui_to_config();
        }
    }
}
