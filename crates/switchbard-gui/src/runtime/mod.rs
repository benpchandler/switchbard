//! Plain-data domain types used by the GUI layer, plus the worktree-expansion
//! helper that bridges configured `Repo`s to the live list of `WorktreeRef`s.
//!
//! Types mirror the user's mental model:
//! - `WorktreeMeta` = git probe results for one worktree (dirty, ahead/behind, age).
//! - `ActiveRun`    = a process Switchbard launched that's still going.
//! - `PickerState`  = the rfd file-picker hand-off.
//! - `RowState`     = the verdict for a service row in the workspace
//!   (drives state / ports / actions from a single decision).
//!
//! (`ViewMode` is gone — the GUI is now a single workspace panel with
//! per-repo swimlane cards, no tabs.)

pub mod worktree_create;
pub mod worktree_names;
pub mod worktree_rename;
pub mod worktrees;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use switchbard_core::dispatch_inspect::{DispatchRun, DispatchRunLiveness};
use switchbard_core::{
    AgentKind, AttachedProcesses, AttributedListener, BranchDeleteAssessment, CommitSummary,
    ContextKind, ContextScope, DirtyFile, DriftDetail, DriftProbe, Fact, Landed, LandingStage,
    OrderingOverlay, RemovalFacts, RemovalIntent, RemovalSafety, RemovalVerdict, Repo, TrunkDetail,
    TrunkDivergence, WorktreeRef, WorktreeStaleness,
};

/// The cross-repo triage overlay (`<hub repo>/ordering.yml`), refreshed by
/// the backlog worker alongside the project scan. `warning` is set when a
/// present-but-malformed file falls back to an empty overlay (task-10 AC #3)
/// — surfaced in the Backlog view's summary bar as a warning pill, the same
/// treatment `BacklogRepo::warnings` gets, rather than through the
/// transient `backlog_status` line (which a 30s-periodic worker write would
/// otherwise clobber mid-read).
#[derive(Debug, Clone, Default)]
pub struct OrderingState {
    pub overlay: OrderingOverlay,
    pub warning: Option<String>,
}

/// Top-level central-panel tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewTab {
    #[default]
    Servers,
    Agents,
    Backlog,
    Dispatch,
}

/// Sibling surfaces within the top-level Agents view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentsSection {
    #[default]
    Context,
    Hooks,
}

/// Identifies one task across every tracked Backlog project: the project's
/// worktree-root key (as everywhere else backlog state is keyed) plus its
/// `backlog` task id. A bare task id is only unique **within** a project —
/// the unified All-projects scope can show two "TASK-10"s side by side from
/// different repos, so selection/bulk-selection must key on the pair.
pub type BacklogTaskKey = (PathBuf, String);

/// One in-flight board drag-drop, keyed by the moved task (task-42: "Board
/// drag: optimistic move + drop feedback"). Lives on `BacklogViewState`
/// rather than folded into `HiveApp::backlog_repos` — that cache is
/// reloaded on its own cadence by `workers::spawn_backlog` and by a
/// mutation's own targeted reload (`app::refresh_backlog_repo_cache`),
/// and there is no way to tell "a worker just clobbered this" from "the drop
/// itself resolved" if an in-flight edit lived in the same map as real,
/// disk-backed data. Keeping this a separate, render-time-only overlay means
/// the cache never carries anything but real data; `board::render_column`
/// folds this on top of it for exactly one purpose — bucketing the card into
/// its destination column before the round trip through the `backlog` CLI
/// resolves.
///
/// **Post-review revision (independent audit, F1/F3):** the first version of
/// this type resolved off `BacklogRepo::loaded_at_unix` advancing past a
/// drop-time snapshot — "any reload, from any source, resolves it." That was
/// wrong: an *unrelated* reload (the periodic backlog worker's own poll,
/// woken early by `Kick::notify` right after a completely different save)
/// would resolve a still-in-flight move early, snapping the card back to
/// its stale real status mid-flight — exactly the confusing flicker this
/// feature exists to kill, and *more* likely right after a prior drop since
/// that's precisely when the worker gets woken. `generation` replaces that
/// signal: an entry now resolves only off **its own drop's own save**
/// completing (`HiveApp::board_move_outcomes`, written by
/// `spawn_board_move_save`), never off an unrelated cache reload.
#[derive(Debug, Clone)]
pub struct PendingBoardMove {
    /// The column the card was dropped on — what it renders under until
    /// this entry resolves one way or the other.
    pub target_status: String,
    /// Monotonically increasing per-drop token (`BacklogViewState::
    /// next_move_generation`), stamped at drop time and carried by
    /// `spawn_board_move_save` into its completion report
    /// (`BoardMoveOutcome`). `board::resolve_pending_moves` only ever
    /// resolves an entry against an outcome whose generation matches this
    /// one exactly — a later drop on the same task overwrites this whole
    /// `PendingBoardMove` (a fresh generation), so a *stale* outcome for a
    /// now-superseded generation is recognized and discarded rather than
    /// incorrectly resolving the newer entry.
    pub generation: u64,
    /// Wall-clock fallback: if this generation's own outcome is never
    /// reported (e.g. the save thread panics, or some future code path
    /// forgets to report), this bounds how long a card can sit in the
    /// optimistic "saving" state before the overlay gives up and clears
    /// itself, falling back to whatever's actually on disk next render. A
    /// stranded overlay entry — a card permanently claiming a status the
    /// real data never confirmed — is a worse failure mode than an
    /// occasional card that quietly reverts after a bounded wait. This is
    /// now purely a last-resort backstop, not (as in the first version) the
    /// thing standing in for a real completion signal.
    pub queued_at: Instant,
}

/// The result `HiveApp::spawn_board_move_save`'s background thread reports
/// for one drop, keyed by the moved task in `HiveApp::board_move_outcomes`.
/// `board::resolve_pending_moves` drains this map every frame and only acts
/// on an outcome whose `generation` matches the *current* `pending_moves`
/// entry for that key — see `PendingBoardMove::generation`'s doc.
#[derive(Debug, Clone, Copy)]
pub struct BoardMoveOutcome {
    pub generation: u64,
    pub success: bool,
}

/// UI-local filters and edit buffers for the Backlog project-management view.
///
/// `selected_repo` doubles as the scope switch: `None` (the default) is
/// the unified "All projects" scope — the task list merges every tracked
/// project, triage-ranked, with a repo badge per row. `Some(path)` narrows
/// to that one project, matching how the view worked before the unified
/// scope. Reusing the field rather than adding a parallel enum keeps "which
/// project(s) am I looking at" in one place, and it's exactly the field the
/// existing project picker combo box already drives.
#[derive(Debug, Clone)]
pub struct BacklogViewState {
    pub selected_repo: Option<PathBuf>,
    pub selected_task: Option<BacklogTaskKey>,
    pub bulk_selected_tasks: BTreeSet<BacklogTaskKey>,
    pub bulk_selection_anchor: Option<BacklogTaskKey>,
    pub repo_filter: String,
    pub status_filter: String,
    pub priority_filter: String,
    /// The project-name facet ("all" sentinel, same as status/priority):
    /// filters the List lens's row set without switching to the Projects
    /// lens.
    pub project_filter: String,
    pub label_filter: String,
    pub sort_key: BacklogTaskSortKey,
    pub sort_direction: BacklogTaskSortDirection,
    pub show_completed: bool,
    pub show_archived: bool,
    /// Drafts are parsed by core unconditionally; this only controls whether
    /// they're *visible* in the current filter (task-15 AC #4). Defaults to
    /// `true` so existing behavior (drafts always shown) doesn't regress —
    /// the checkbox is there so a user can filter them out, not opt in.
    pub show_drafts: bool,
    /// Which lens renders the central panel: the triage/status list (the
    /// pre-existing view), the per-status kanban board, tasks grouped by
    /// milestone, or the cross-repo statistics dashboard.
    pub lens: BacklogLens,
    pub editor: BacklogEditorState,
    pub new_task: BacklogNewTaskState,
    /// Whether the persistent task-detail rail is reduced to its edge
    /// toggle. Session-only like the current task selection: dragging the
    /// expanded rail still persists its width in egui's panel memory, while
    /// this flag owns the distinct open/closed state.
    pub detail_rail_collapsed: bool,
    /// Whether the Backlog is narrowed to tasks untouched for at least
    /// `Config::ui.stale_after_days`. Persisted with the other ordinary
    /// filters; bulk archive still requires its separate explicit confirm.
    pub stale_only: bool,
    /// Whether the bulk-archive action is primed for its confirm click.
    /// Cleared whenever the filtered set changes, so a confirm can never
    /// apply to a different set than the one its count described.
    pub bulk_archive_confirm: bool,
    /// Global free-text search overlay (Cmd+K / Ctrl+K), task-15 AC #2.
    pub search: BacklogSearchState,
    /// Set to the task the user clicked "Archive" on; the detail pane shows
    /// an inline "Archive this task?" confirmation until they confirm or
    /// cancel. Cleared whenever the detail selection changes.
    pub archive_confirm: bool,
    /// Which parent rows the List lens's sub-task tree (task-17) currently
    /// shows expanded. Session-only UI state — not persisted, same as
    /// `bulk_selected_tasks`; a collapsed-by-default tree on next launch is
    /// the expected, unsurprising behavior.
    pub expanded_parents: BTreeSet<BacklogTaskKey>,
    /// Name of the `SavedView` (task-20, `Config::ui.saved_views`) currently
    /// applied, if the filters/sort/lens still match what was saved under
    /// it. `None` means "unsaved" — the normal state while just filtering
    /// around. Session-only; not itself persisted.
    pub active_saved_view: Option<String>,
    /// Draft text for the "Save current as…" field.
    pub saved_view_name_draft: String,
    /// Mirrors `archive_confirm`'s inline-confirm pattern for the "Dispatch
    /// this task" opt-in — flagging a task hands it to an autonomous
    /// headless agent run, consequential enough to confirm the same way
    /// Archive does. Cleared whenever the detail selection changes.
    pub dispatch_confirm: bool,
    /// Mirrors `archive_confirm` for "Clean Up Old Tasks" (QA parity matrix
    /// LOW gap) — a workspace-global bulk action, not tied to any one
    /// task's selection, so unlike the per-task confirms above it's cleared
    /// by its own Confirm/Cancel buttons only, not by selection changes.
    pub cleanup_confirm: bool,
    /// task-42: render-time overlay for every in-flight Board drag-drop —
    /// see `PendingBoardMove`'s doc for why this isn't folded into
    /// `HiveApp::backlog_repos`. `board::resolve_pending_moves` is the
    /// only place entries are cleared.
    pub pending_moves: HashMap<BacklogTaskKey, PendingBoardMove>,
    /// task-42: once a `pending_moves` entry resolves as a *success* (the
    /// reloaded cache confirms the target status), the resolved key moves
    /// here with the instant it landed — `board::paint_card` reads this to
    /// paint a brief, one-shot "landing flash" on the card. Entries expire
    /// (and remove themselves) once the flash's fixed duration elapses;
    /// see `board::resolve_pending_moves`.
    pub landing_flash: HashMap<BacklogTaskKey, Instant>,
    /// task-42: the next `PendingBoardMove::generation` token —
    /// `board::apply_drop` reads-then-increments this on every drop.
    /// Session-only, single-threaded (only ever touched from the UI thread
    /// during `render_board`); the background save thread only ever
    /// receives a copy of the value stamped at drop time, never this
    /// counter itself.
    pub next_move_generation: u64,
}

impl Default for BacklogViewState {
    fn default() -> Self {
        Self {
            selected_repo: None,
            selected_task: None,
            bulk_selected_tasks: BTreeSet::new(),
            bulk_selection_anchor: None,
            repo_filter: String::new(),
            status_filter: "all".to_string(),
            priority_filter: "all".to_string(),
            project_filter: "all".to_string(),
            label_filter: "all".to_string(),
            sort_key: BacklogTaskSortKey::default(),
            sort_direction: BacklogTaskSortDirection::default(),
            show_completed: false,
            show_archived: false,
            show_drafts: true,
            lens: BacklogLens::default(),
            editor: BacklogEditorState::default(),
            new_task: BacklogNewTaskState::default(),
            detail_rail_collapsed: false,
            stale_only: false,
            bulk_archive_confirm: false,
            search: BacklogSearchState::default(),
            archive_confirm: false,
            expanded_parents: BTreeSet::new(),
            active_saved_view: None,
            saved_view_name_draft: String::new(),
            dispatch_confirm: false,
            cleanup_confirm: false,
            pending_moves: HashMap::new(),
            landing_flash: HashMap::new(),
            next_move_generation: 0,
        }
    }
}

impl BacklogViewState {
    pub fn restore_filters(ui: &switchbard_core::config::UiConfig) -> Self {
        let mut state = Self::default();
        let Some(memory) = ui.filters.get("backlog") else {
            return state;
        };
        // "repo"/"repo_query" are the current keys; "project"/"project_query"
        // are the pre-rename spellings, read as fallbacks so an existing
        // config restores once and re-persists under the new keys.
        state.repo_filter = memory
            .facets
            .get("repo_query")
            .or_else(|| memory.facets.get("project_query"))
            .cloned()
            .unwrap_or_default();
        state.selected_repo = memory
            .facets
            .get("repo")
            .or_else(|| memory.facets.get("project"))
            .map(PathBuf::from);
        state.status_filter = memory
            .facets
            .get("status")
            .cloned()
            .unwrap_or_else(|| "all".to_string());
        state.priority_filter = memory
            .facets
            .get("priority")
            .cloned()
            .unwrap_or_else(|| "all".to_string());
        state.project_filter = memory
            .facets
            .get("project_name")
            .or_else(|| memory.facets.get("milestone"))
            .cloned()
            .unwrap_or_else(|| "all".to_string());
        state.label_filter = memory
            .facets
            .get("label")
            .cloned()
            .unwrap_or_else(|| "all".to_string());
        state.show_completed = facet_bool(memory, "completed", false);
        state.show_archived = facet_bool(memory, "archived", false);
        state.show_drafts = facet_bool(memory, "drafts", true);
        state.stale_only = facet_bool(memory, "stale", false);
        state
    }

    pub fn persist_filters(&self, ui: &mut switchbard_core::config::UiConfig) {
        let memory = ui.filters.entry("backlog".to_string()).or_default();
        set_optional_facet(
            &mut memory.facets,
            "repo_query",
            (!self.repo_filter.is_empty()).then(|| self.repo_filter.clone()),
        );
        set_optional_facet(
            &mut memory.facets,
            "repo",
            self.selected_repo
                .as_ref()
                .map(|path| path.display().to_string()),
        );
        // Purge the pre-rename keys so a config only ever carries one
        // spelling. "project" held a repo *path* before the rename, which is
        // why the project-name facet uses the fresh key "project_name".
        memory.facets.remove("project_query");
        memory.facets.remove("project");
        memory.facets.remove("milestone");
        persist_non_default(&mut memory.facets, "status", &self.status_filter, "all");
        persist_non_default(&mut memory.facets, "priority", &self.priority_filter, "all");
        persist_non_default(
            &mut memory.facets,
            "project_name",
            &self.project_filter,
            "all",
        );
        persist_non_default(&mut memory.facets, "label", &self.label_filter, "all");
        persist_bool(&mut memory.facets, "completed", self.show_completed, false);
        persist_bool(&mut memory.facets, "archived", self.show_archived, false);
        persist_bool(&mut memory.facets, "drafts", self.show_drafts, true);
        persist_bool(&mut memory.facets, "stale", self.stale_only, false);
    }
}

fn facet_bool(memory: &switchbard_core::config::FilterMemory, key: &str, default: bool) -> bool {
    memory
        .facets
        .get(key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn persist_non_default(
    facets: &mut std::collections::BTreeMap<String, String>,
    key: &str,
    value: &str,
    default: &str,
) {
    set_optional_facet(facets, key, (value != default).then(|| value.to_string()));
}

fn persist_bool(
    facets: &mut std::collections::BTreeMap<String, String>,
    key: &str,
    value: bool,
    default: bool,
) {
    set_optional_facet(facets, key, (value != default).then(|| value.to_string()));
}

/// The Backlog view's central-panel lens. `List` is the pre-existing
/// triage/status list; `Board`/`Projects`/`Statistics` are task-15/16
/// additions; `Digest`/`Portfolio` are task-21/19. `Digest` is the default —
/// task-21 makes the "what should I do today" landing screen the Backlog
/// tab's default lens (not the whole app's default *tab*, which stays
/// `ViewTab::Servers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BacklogLens {
    #[default]
    Digest,
    List,
    Board,
    Projects,
    Portfolio,
    Statistics,
}

impl BacklogLens {
    pub fn label(self) -> &'static str {
        match self {
            Self::Digest => "Digest",
            Self::List => "List",
            Self::Board => "Board",
            Self::Projects => "Projects",
            Self::Portfolio => "Portfolio",
            Self::Statistics => "Statistics",
        }
    }

    /// Stable identifier persisted in `SavedView::lens` (task-20). Not the
    /// same string as `label()` on principle — `label()` is UI copy, free to
    /// reword; this one is a serialization format that must stay stable
    /// across renames.
    pub fn as_saved_id(self) -> &'static str {
        match self {
            Self::Digest => "digest",
            Self::List => "list",
            Self::Board => "board",
            Self::Projects => "projects",
            Self::Portfolio => "portfolio",
            Self::Statistics => "statistics",
        }
    }

    /// Inverse of [`as_saved_id`][Self::as_saved_id]. Falls back to the
    /// default lens for anything unrecognized (a saved view from an older
    /// build naming a lens this version doesn't have) rather than erroring —
    /// see `SavedView`'s doc in `switchbard_core::config`.
    pub fn from_saved_id(id: &str) -> Self {
        match id {
            "list" => Self::List,
            "board" => Self::Board,
            "projects" => Self::Projects,
            // Pre-rename spelling (the lens was "Milestones" before the
            // Linear-hierarchy divergence) — old saved views land here.
            "milestones" => Self::Projects,
            "portfolio" => Self::Portfolio,
            "statistics" => Self::Statistics,
            _ => Self::Digest,
        }
    }
}

/// State for the Cmd+K / Ctrl+K global task search overlay.
#[derive(Debug, Clone, Default)]
pub struct BacklogSearchState {
    pub open: bool,
    pub query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BacklogTaskSortKey {
    /// The triage-ranked order (`switchbard_core::triage_rank`): overlay
    /// rank, then overdue/due-today/priority/age/repo. Default — this is
    /// what makes the All-projects scope a triage queue out of the box; the
    /// other sort keys remain available in either scope to override it.
    #[default]
    Triage,
    Task,
    Status,
    Priority,
    AcceptanceCriteria,
    /// QA parity matrix MEDIUM gap: the webview's All Tasks table sorts by
    /// labels/assignee/milestone; `compare_tasks` (sort.rs) had no such
    /// keys.
    Labels,
    Assignee,
    Project,
}

impl BacklogTaskSortKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Triage => "Triage",
            Self::Task => "Task",
            Self::Status => "Status",
            Self::Priority => "Priority",
            Self::AcceptanceCriteria => "AC",
            Self::Labels => "Labels",
            Self::Assignee => "Assignee",
            Self::Project => "Project",
        }
    }

    /// See `BacklogLens::as_saved_id` — same rationale (task-20 saved views).
    pub fn as_saved_id(self) -> &'static str {
        match self {
            Self::Triage => "triage",
            Self::Task => "task",
            Self::Status => "status",
            Self::Priority => "priority",
            Self::AcceptanceCriteria => "acceptance_criteria",
            Self::Labels => "labels",
            Self::Assignee => "assignee",
            Self::Project => "project",
        }
    }

    pub fn from_saved_id(id: &str) -> Self {
        match id {
            "task" => Self::Task,
            "status" => Self::Status,
            "priority" => Self::Priority,
            "acceptance_criteria" => Self::AcceptanceCriteria,
            "labels" => Self::Labels,
            "assignee" => Self::Assignee,
            "project" => Self::Project,
            // Pre-rename spelling from old saved views.
            "milestone" => Self::Project,
            _ => Self::Triage,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BacklogTaskSortDirection {
    #[default]
    Ascending,
    Descending,
}

impl BacklogTaskSortDirection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ascending => "Ascending",
            Self::Descending => "Descending",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }

    /// See `BacklogLens::as_saved_id` (task-20 saved views).
    pub fn as_saved_id(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }

    pub fn from_saved_id(id: &str) -> Self {
        match id {
            "descending" => Self::Descending,
            _ => Self::Ascending,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BacklogEditorState {
    pub loaded_key: Option<String>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub labels: String,
    pub assignees: String,
    pub dependencies: String,
    pub plan: String,
    pub project: String,
    pub note: String,
    /// `false` (default) shows the description as rendered CommonMark;
    /// `true` reveals the raw multiline editor (task-15 AC #3).
    pub description_editing: bool,
    /// Draft text for the "add a reference" inline field. Submitting appends
    /// it to the task's existing reference list and saves immediately —
    /// unlike the other editor fields, it isn't held for the batch Save.
    pub new_reference: String,
}

#[derive(Debug, Clone)]
pub struct BacklogNewTaskState {
    pub open: bool,
    /// Which project to create the task in. Seeded from `selected_repo`
    /// when it names one project; when the view is in the All-projects scope
    /// (`selected_repo` is `None`), the modal shows its own project picker
    /// and stores the choice here instead of forcing the user out of the
    /// unified scope just to file a task.
    pub target_repo: Option<PathBuf>,
    /// Set when the modal was opened via "+ Subtask" on a task's detail pane
    /// (task-17) — the parent task id, passed through as `-p` on create.
    /// `Some` also pins `target_repo` to the parent's own project; a
    /// subtask can't be filed in a different repo than its parent, since
    /// Backlog.md's `parent` field is a bare, project-scoped id.
    pub parent: Option<String>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub acceptance_criteria: String,
    /// QA parity matrix LOW gap: labels/assignee/milestone/dependencies were
    /// only settable after creation, via the detail pane's own editors. Same
    /// comma-separated draft-text shape as `BacklogEditorState`'s
    /// labels/assignees/dependencies fields (`detail_lists::split_csv`
    /// parses both).
    pub labels: String,
    pub assignees: String,
    pub project: String,
    pub dependencies: String,
}

impl Default for BacklogNewTaskState {
    fn default() -> Self {
        Self {
            open: false,
            target_repo: None,
            parent: None,
            title: String::new(),
            description: String::new(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            acceptance_criteria: String::new(),
            labels: String::new(),
            assignees: String::new(),
            project: String::new(),
            dependencies: String::new(),
        }
    }
}

/// Agent target selected in the Agents view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentContextAgent {
    Claude,
    Codex,
    All,
}

impl AgentContextAgent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::All => "All agents",
        }
    }

    pub fn agent_kind(self) -> AgentKind {
        match self {
            Self::Claude | Self::All => AgentKind::Claude,
            Self::Codex => AgentKind::Codex,
        }
    }
}

/// UI-local selection state for the Agents view.
#[derive(Debug, Clone)]
pub struct AgentContextViewState {
    pub section: AgentsSection,
    pub scope: ContextScope,
    pub kind: Option<ContextKind>,
    pub selected_id: Option<String>,
    pub agent: AgentContextAgent,
    pub global_kind: Option<ContextKind>,
    pub global_selected_id: Option<String>,
    pub global_open: bool,
    pub pinned_repo: Option<String>,
    /// Hook-only facets. `None` means all values; options are derived from the
    /// detected registrations so stale or custom event names remain visible.
    pub hook_scope: Option<ContextScope>,
    pub hook_event: Option<String>,
    pub hook_type: Option<String>,
}

impl Default for AgentContextViewState {
    fn default() -> Self {
        Self {
            section: AgentsSection::Context,
            scope: ContextScope::Local,
            kind: None,
            selected_id: None,
            agent: AgentContextAgent::Claude,
            global_kind: None,
            global_selected_id: None,
            global_open: false,
            pinned_repo: None,
            hook_scope: None,
            hook_event: None,
            hook_type: None,
        }
    }
}

impl AgentContextViewState {
    /// Restore durable filter choices while leaving selection, expansion, and
    /// navigation state at their safe session defaults.
    pub fn restore_filters(ui: &switchbard_core::config::UiConfig) -> Self {
        let mut state = Self::default();
        let shared = ui.filters.get("agents");
        state.agent = match shared
            .and_then(|memory| memory.facets.get("agent"))
            .map(String::as_str)
        {
            Some("codex") => AgentContextAgent::Codex,
            Some("all") => AgentContextAgent::All,
            _ => AgentContextAgent::Claude,
        };
        if let Some(memory) = ui.filters.get("agents.context") {
            state.scope = match memory.facets.get("scope").map(String::as_str) {
                Some("directory") => ContextScope::Directory,
                _ => ContextScope::Local,
            };
            state.kind = memory
                .facets
                .get("type")
                .and_then(|value| match value.as_str() {
                    "instruction" => Some(ContextKind::Instruction),
                    "command" => Some(ContextKind::Command),
                    "skill" => Some(ContextKind::Skill),
                    "config" => Some(ContextKind::Config),
                    "doc" => Some(ContextKind::Doc),
                    _ => None,
                });
        }
        if let Some(memory) = ui.filters.get("agents.hooks") {
            state.hook_scope = memory
                .facets
                .get("scope")
                .and_then(|value| match value.as_str() {
                    "global" => Some(ContextScope::Global),
                    "local" => Some(ContextScope::Local),
                    "directory" => Some(ContextScope::Directory),
                    _ => None,
                });
            state.hook_event = memory.facets.get("event").cloned();
            state.hook_type = memory.facets.get("handler").cloned();
        }
        state
    }

    pub fn persist_filters(&self, ui: &mut switchbard_core::config::UiConfig) {
        let shared = ui.filters.entry("agents".to_string()).or_default();
        shared.facets.insert(
            "agent".to_string(),
            match self.agent {
                AgentContextAgent::Claude => "claude",
                AgentContextAgent::Codex => "codex",
                AgentContextAgent::All => "all",
            }
            .to_string(),
        );

        let context = ui.filters.entry("agents.context".to_string()).or_default();
        context.facets.insert(
            "scope".to_string(),
            match self.scope {
                ContextScope::Directory => "directory",
                ContextScope::Global | ContextScope::Local => "local",
            }
            .to_string(),
        );
        match self.kind {
            Some(kind) => {
                context.facets.insert(
                    "type".to_string(),
                    match kind {
                        ContextKind::Instruction => "instruction",
                        ContextKind::Command => "command",
                        ContextKind::Skill => "skill",
                        ContextKind::Config => "config",
                        ContextKind::Doc => "doc",
                    }
                    .to_string(),
                );
            }
            None => {
                context.facets.remove("type");
            }
        }

        let hooks = ui.filters.entry("agents.hooks".to_string()).or_default();
        set_optional_facet(
            &mut hooks.facets,
            "scope",
            self.hook_scope.map(|scope| match scope {
                ContextScope::Global => "global".to_string(),
                ContextScope::Local => "local".to_string(),
                ContextScope::Directory => "directory".to_string(),
            }),
        );
        set_optional_facet(&mut hooks.facets, "event", self.hook_event.clone());
        set_optional_facet(&mut hooks.facets, "handler", self.hook_type.clone());
    }
}

fn set_optional_facet(
    facets: &mut std::collections::BTreeMap<String, String>,
    key: &str,
    value: Option<String>,
) {
    match value {
        Some(value) => {
            facets.insert(key.to_string(), value);
        }
        None => {
            facets.remove(key);
        }
    }
}

/// Active-run summary shown in the remove-worktree dialog. Stripped down from
/// `ActiveRun` because the dialog only needs the user-visible name + the pgid
/// it'll signal.
#[derive(Debug, Clone)]
pub struct ActiveRunSummary {
    pub service_name: String,
    pub pgid: i32,
}

/// State for the modal that confirms `git worktree remove`. Held in an
/// `Arc<Mutex<Option<…>>>` on `HiveApp` so the worker thread can flip
/// `busy`/`error` while the UI renders.
#[derive(Debug, Clone)]
pub struct ConfirmRemoveWorktree {
    pub repo_path: PathBuf,
    pub worktree_path: PathBuf,
    pub branch: Option<String>,
    pub dirty_files: Vec<DirtyFile>,
    pub active_runs: Vec<ActiveRunSummary>,
    /// Local git facts about deleting `branch`, computed when the dialog opens.
    /// `None` when the worktree has no branch (detached HEAD) — no deletion
    /// option is offered in that case.
    ///
    /// Narrower than it looks: the only question this still answers for the
    /// dialog is [`BranchDeleteAssessment::is_blocked`] - would git accept
    /// `branch -d` at all. Whether the work *landed* comes from
    /// `removal_facts` like everywhere else, so the dialog cannot disagree
    /// with the row badge that sent the user here.
    pub branch_assessment: Option<BranchDeleteAssessment>,
    /// The shared safety facts, probed synchronously at open time. Evaluated
    /// per-frame at the intent the checkbox currently implies, so ticking
    /// "also delete the branch" re-reads the same rule set rather than
    /// switching to a parallel one.
    pub removal_facts: RemovalFacts,
    /// The "also delete the branch" checkbox. Defaults off; only meaningful
    /// when `branch_assessment` is present and not blocked.
    pub delete_branch: bool,
    /// True while the worker thread is killing services + running git.
    /// Disables both buttons and shows a spinner.
    pub busy: bool,
    /// If the removal attempt failed, the git stderr (or kill error) lands
    /// here so the dialog can show it inline without closing.
    pub error: Option<String>,
}

impl ConfirmRemoveWorktree {
    /// Whether the dialog should offer a branch-delete checkbox: there's a
    /// branch, and git wouldn't refuse outright (not checked out elsewhere).
    pub fn can_offer_branch_delete(&self) -> bool {
        self.branch_assessment
            .as_ref()
            .is_some_and(|a| !a.is_blocked())
    }

    /// Whether the confirmed action will actually delete the branch.
    pub fn will_delete_branch(&self) -> bool {
        self.delete_branch && self.can_offer_branch_delete()
    }
}

/// One worktree considered for the bulk-remove sweep (TASK-41), classified at
/// dialog-open time by re-running the same primitives the single-row Remove
/// dialog uses (`collect_dirty_files` + `assess_branch_delete`) — never a
/// parallel "is this safe" check.
#[derive(Debug, Clone)]
pub struct BulkRemoveCandidate {
    pub repo_path: PathBuf,
    pub worktree_path: PathBuf,
    pub display_name: String,
    pub branch: Option<String>,
    pub branch_assessment: Option<BranchDeleteAssessment>,
    /// Why this candidate isn't in the removable set — `None` for a
    /// clean+merged worktree with no other blockers. Non-`None` routes it
    /// into the "needs review" list instead, and it is never force-removed.
    pub review_reason: Option<String>,
}

impl BulkRemoveCandidate {
    pub fn is_removable(&self) -> bool {
        self.review_reason.is_none()
    }
}

/// State for the bulk "Remove N worktrees" confirmation modal (TASK-41).
/// Deliberately lighter-weight than `ConfirmRemoveWorktree`: there's no
/// worker-visible `busy`/`error` in-place mutation here because, like
/// `HiveApp::spawn_backlog_bulk_save`/`spawn_backlog_cleanup`, Confirm closes
/// the dialog immediately and reports the outcome via `config_status` —
/// appropriate for a batch action where "which of the N failed" is better
/// read as a summary line than re-litigated in a lingering modal.
#[derive(Debug, Clone)]
pub struct ConfirmBulkRemoveWorktrees {
    /// Clean + merged: what Confirm actually removes.
    pub removable: Vec<BulkRemoveCandidate>,
    /// Dirty and/or unmerged (or has active runs/listeners — see
    /// `worktree_actions::open_bulk_remove_worktree_confirm`'s doc): shown for
    /// visibility only, never touched by Confirm.
    pub needs_review: Vec<BulkRemoveCandidate>,
    /// "Also delete branch" — defaults ON here (unlike the single-row
    /// dialog): every `removable` candidate is already known merged, so
    /// deleting its branch is always a plain, non-force `git branch -d`.
    pub delete_branches: bool,
}

#[derive(Debug, Clone, Default)]
pub struct WorktreeMeta {
    /// `Some(files)` after the porcelain probe completes — empty means clean.
    /// `None` while the probe hasn't returned yet (or it failed).
    pub dirty_files: Option<Vec<String>>,
    /// Summary of ignored local artifacts (`!! path` porcelain rows). These
    /// are not dirty from Git's perspective, but deleting the worktree would
    /// still delete them from disk. Store only a preview so dependency/build
    /// directories do not make every UI frame clone thousands of strings.
    pub ignored_files: Option<FileListSummary>,
    /// What a branch delete here would discard, measured against the repo's
    /// trunk the same way `removal_safety`'s `WorkLanded` check measures it.
    ///
    /// Replaces a `DriftProbe` against the local `main`. Two things were wrong
    /// with that: the base (the removal checks compare against
    /// `default_branch()`, usually `origin/main`, so a stale local trunk made
    /// the row read `+16` next to `remove ok`), and the method (ancestry
    /// counts rebase-merged commits as ahead, so the row offered a list of
    /// "at risk" commits that were already upstream).
    pub trunk: Option<TrunkDivergence>,
    /// `HEAD` compared with the current branch's configured upstream remote.
    /// Still a `DriftProbe`: for "am I in sync with my upstream", ancestry is
    /// the right question and patch-equivalence is not.
    pub remote_drift: Option<DriftProbe>,
    /// Commit lists behind the trunk comparison, capped for tooltip use.
    pub trunk_detail: Option<TrunkDetail>,
    /// Commit lists behind the remote-upstream comparison, capped for tooltip use.
    pub remote_drift_detail: Option<DriftDetail>,
    pub head_commit_unix: Option<u64>,
    /// Unix seconds of the last `git fetch` against this repo. None when the
    /// repo has never been fetched (fresh clone of nothing).
    pub fetch_unix: Option<u64>,
    /// Newest-first list of recent commits on the current branch (capped).
    /// Powers the ACTIVITY column (velocity badge + commit-subject hover).
    pub recent_commits: Option<Vec<CommitSummary>>,
    /// Set when the probe completes; kept for a future "stale data" badge in
    /// the UI. Currently unread.
    #[allow(dead_code)]
    pub probed_at: Option<Instant>,
    /// Merged/NoUpstream/Live classification (TASK-41), computed by the same
    /// git-probe worker tick as `trunk`/`remote_drift`. `None` while the
    /// probe hasn't returned yet.
    pub staleness: Option<WorktreeStaleness>,
    /// Whether git holds a lock on this worktree, and why. A locked worktree
    /// is one `git worktree remove` refuses outright, so the row's badge has
    /// to know before it can promise a removal will work. Defaults to
    /// [`Fact::Pending`].
    pub lock: Fact<Option<String>>,
}

/// Cached on-disk size for one worktree (TASK-41). Lives in its own
/// `Arc<Mutex<HashMap<..>>>` on `HiveApp` (not folded into `WorktreeMeta`)
/// because it's refreshed on a much slower, independently-paced cadence — see
/// `workers::spawn_size`'s doc for why `du` can't share the git-probe
/// worker's tick.
#[derive(Debug, Clone, Copy)]
pub struct WorktreeSizeEntry {
    /// `None` when the `du` call itself failed (missing dir, permission
    /// error) — distinct from "not probed yet" (`spawn_size` simply hasn't
    /// gotten to this worktree, and the entry doesn't exist in the map yet).
    pub bytes: Option<u64>,
    pub computed_at: Instant,
}

/// A worktree's landing stage plus when it was computed, so the worker can
/// age entries out the way `WorktreeSizeEntry` does.
///
/// Its own map rather than a `WorktreeMeta` field, for the same reason sizes
/// are: the git-probe worker rewrites `meta` wholesale every tick and would
/// wipe a field it doesn't own. It also has a genuinely different cadence —
/// the PR half is a network call.
#[derive(Debug, Clone)]
pub struct LandingEntry {
    pub stage: LandingStage,
    pub computed_at: Instant,
}

/// Is `w` the primary checkout of its repo? A cheap "same path" check — good
/// enough for counts/badges/filters, which can tolerate the rare race where
/// a repo's primary path briefly doesn't match (e.g. mid-rename). Callers
/// about to take a destructive action instead use the stronger,
/// filesystem-canonicalizing `switchbard_core::is_primary_worktree`.
///
/// Shared by the git-probe worker's retired-worktree-count cache
/// (`workers::spawn_probe`) and the Workspace staleness view
/// (`ui::workspace::staleness`) so "is this worktree primary" has exactly
/// one answer instead of two copies that could drift apart.
pub fn worktree_is_primary(w: &WorktreeRef, repos: &[Repo]) -> bool {
    repos
        .iter()
        .any(|r| r.name == w.repo_name && r.path == w.path)
}

/// Is this worktree one the user could retire right now?
///
/// Powers the top-bar "N retired worktrees" nudge and the "Select all
/// merged+clean" bulk-select action, so what the nudge counts is exactly what
/// the button selects.
///
/// **This is not a separate definition of "safe to remove".** It used to be:
/// non-primary, merged, and not dirty — two of the five checks
/// `removal_safety` actually applies. So "Select all merged+clean" would
/// happily select a worktree whose own badge read `remove blocked`, because
/// the badge also knows about locks and about processes still running there.
/// A bulk-select that hands you rows the app then refuses to remove is worse
/// than no bulk-select.
///
/// It now evaluates the same `RemovalSafety` every other surface does, and
/// only a `Safe` verdict counts. An unprobed worktree yields `Checking`, so it
/// is still never counted — for the right reason this time, rather than as a
/// side effect of two `Option`s being `None`.
pub fn is_retired_worktree(
    w: &WorktreeRef,
    repos: &[Repo],
    meta: Option<&WorktreeMeta>,
    attached: AttachedProcesses,
) -> bool {
    if worktree_is_primary(w, repos) {
        return false;
    }
    let Some(meta) = meta else {
        return false;
    };
    // `is_primary` is `false` here, not recomputed: the primary case already
    // returned above, and it uses the cheap path-equality check the rest of
    // the render path uses. The canonicalizing check stays where it belongs —
    // on the paths that actually remove things.
    let facts = removal_facts(false, meta, attached);
    RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).verdict()
        == RemovalVerdict::Safe
}

/// Everything holding on to `worktree` in one place, from the three
/// collections that know.
///
/// The one derivation, so the git-probe worker's "N retired" count and
/// `HiveApp::attached_processes` cannot disagree about what counts as busy.
/// Takes borrowed collections rather than `&HiveApp` because the worker holds
/// its own channel handles and never sees the app.
pub fn attached_processes_for(
    worktree: &Path,
    listener_count: usize,
    active_runs: &HashMap<i32, ActiveRun>,
    dispatch_runs: &HashMap<BacklogTaskKey, DispatchRun>,
) -> AttachedProcesses {
    AttachedProcesses {
        listeners: listener_count,
        switchbard_runs: active_runs
            .values()
            .filter(|r| r.worktree_path == worktree)
            .count(),
        dispatch_runs: dispatch_runs
            .values()
            .filter(|r| r.worktree_path == worktree)
            .filter(|r| dispatch_run_holds_worktree(&r.liveness))
            .count(),
    }
}

/// How much an agent has been committing lately. The thresholds are tuned for
/// the "bazillion agents" workflow — Burst means "rapid-fire commits right
/// now", Active means "still working", Slow means "yesterday-ish", Idle means
/// "nothing recent worth surfacing".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLevel {
    /// No commits in the activity window.
    Idle,
    /// Commits today but none in the last hour. Probably between bursts.
    Slow,
    /// At least one commit in the last hour.
    Active,
    /// 3+ commits in the last 30 minutes. The agent is hammering away.
    Burst,
}

/// Concrete activity reading for one worktree: the level + the count of
/// commits in the recent window + the timestamp of the newest commit.
#[derive(Debug, Clone, Copy)]
pub struct Activity {
    pub level: ActivityLevel,
    /// Commits within the activity window (24h).
    pub count_24h: usize,
    /// Commits within the last hour.
    pub count_1h: usize,
    /// Newest commit's unix time, if any.
    pub newest_unix: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileListSummary {
    pub total: usize,
    pub preview: Vec<String>,
}

impl FileListSummary {
    pub fn from_lines(lines: Vec<String>, preview_limit: usize) -> Self {
        let total = lines.len();
        let preview = lines.into_iter().take(preview_limit).collect();
        Self { total, preview }
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
}

impl WorktreeMeta {
    /// True if the porcelain probe finished and reported at least one file.
    pub fn is_dirty(&self) -> Option<bool> {
        self.dirty_files.as_ref().map(|v| !v.is_empty())
    }

    /// True if the ignored-file probe finished and reported at least one local
    /// ignored artifact.
    pub fn has_ignored_files(&self) -> Option<bool> {
        self.ignored_files.as_ref().map(|v| !v.is_empty())
    }

    /// Bucket recent-commit data into an ActivityLevel. Returns `None` until
    /// the probe has at least returned (even if the result is empty).
    pub fn activity(&self) -> Option<Activity> {
        let commits = self.recent_commits.as_ref()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cutoff_24h = now.saturating_sub(86_400);
        let cutoff_1h = now.saturating_sub(3600);
        let cutoff_30m = now.saturating_sub(1800);

        let count_24h = commits
            .iter()
            .filter(|c| c.committed_unix >= cutoff_24h)
            .count();
        let count_1h = commits
            .iter()
            .filter(|c| c.committed_unix >= cutoff_1h)
            .count();
        let count_30m = commits
            .iter()
            .filter(|c| c.committed_unix >= cutoff_30m)
            .count();
        let newest_unix = commits.iter().map(|c| c.committed_unix).max();

        let level = if count_30m >= 3 {
            ActivityLevel::Burst
        } else if count_1h >= 1 {
            ActivityLevel::Active
        } else if count_24h >= 1 {
            ActivityLevel::Slow
        } else {
            ActivityLevel::Idle
        };
        Some(Activity {
            level,
            count_24h,
            count_1h,
            newest_unix,
        })
    }
}

/// Whether a dispatch agent run still has a claim on its worktree.
///
/// The one rule, because two places count these: the Workspace row (from its
/// per-frame snapshot) and `HiveApp::attached_processes` (from the live
/// locks). They read different sources for good reason; they must not apply
/// different rules to what they read.
///
/// Fails closed on [`DispatchRunLiveness::Unverifiable`]. A sidecar that
/// cannot be authenticated is not proof the agent is gone, and treating
/// "can't tell" as "finished" is exactly how a live agent's worktree gets
/// swept out from under it.
pub fn dispatch_run_holds_worktree(liveness: &DispatchRunLiveness) -> bool {
    match liveness {
        DispatchRunLiveness::Alive { .. } | DispatchRunLiveness::Unverifiable(_) => true,
        DispatchRunLiveness::NoSidecar | DispatchRunLiveness::Gone => false,
    }
}

/// Adapt this worktree's *cached* probe state into the shared
/// [`RemovalFacts`], so the Workspace row's badge is the exact same rule set
/// the two confirm dialogs run - see `switchbard_core::removal_safety`.
///
/// The row is the one caller that legitimately has no answer yet: its facts
/// come from background workers on a cadence, not from a synchronous git
/// call. Every `None` here therefore maps to [`Fact::Pending`], which renders
/// as "checking" rather than as a blocker.
///
/// Known limitation, deliberate and bounded: [`WorktreeMeta`]'s fields use
/// `None` for *both* "not probed yet" and "the probe failed", a convention
/// that predates this module. A persistently failing probe therefore reads as
/// perpetually checking on the row instead of as blocked. That is a cosmetic
/// degradation and never a false green - and the surfaces that actually
/// remove things do not come through here at all. They call
/// `removal_safety::probe_facts`, which distinguishes the two properly.
pub fn removal_facts(
    is_primary: bool,
    meta: &WorktreeMeta,
    attached: AttachedProcesses,
) -> RemovalFacts {
    RemovalFacts {
        is_primary,
        lock: meta.lock.clone(),
        dirty_files: match &meta.dirty_files {
            Some(files) => Fact::Known(files.len()),
            None => Fact::Pending,
        },
        ignored_files: meta.ignored_files.as_ref().map(|i| i.total),
        landed: landed_from_staleness(meta),
        // Listener and run counts are always available: they come from an
        // in-memory scan the caller has already done, not from a probe that
        // can be outstanding.
        attached: Fact::Known(attached),
    }
}

/// Reuse the Merged/NoUpstream/Live badge's own probe as the row's "did the work
/// land" fact, rather than issuing a second git call per row per frame.
///
/// The two cannot disagree: `probe_worktree_staleness` and
/// `assess_branch_delete` both go through `worktree_remove::commits_ahead`.
/// The badge only records *whether* the ahead-count was zero, so a non-merged
/// worktree reports [`Landed::No`] with no count attached: enough to fail the
/// check honestly, without printing a number nothing measured. The dialogs,
/// which have room for it, call `probe_facts` and get the real count.
fn landed_from_staleness(meta: &WorktreeMeta) -> Fact<Landed> {
    match &meta.staleness {
        None => Fact::Pending,
        Some(WorktreeStaleness::Unknown) => {
            Fact::Unavailable("Couldn't work out whether this branch landed".to_string())
        }
        Some(WorktreeStaleness::Merged { base, evidence }) => Fact::Known(Landed::Yes {
            base: base.clone(),
            evidence: *evidence,
        }),
        // NoUpstream and Live are both "probed, and not contained in the base".
        // Neither carries a commit count - the badge only ever recorded
        // whether the count was zero - so both report an unmeasured `No`
        // rather than inventing a number the row could print.
        Some(WorktreeStaleness::NoUpstream | WorktreeStaleness::Live) => Fact::Known(Landed::No {
            commits: None,
            base: None,
        }),
    }
}

#[derive(Debug, Clone)]
pub struct ActiveRun {
    pub worktree_path: PathBuf,
    pub service_name: String,
    // Surfaced via tooltip / future expanded-row detail; keep for UI v0.4.
    #[allow(dead_code)]
    pub command: String,
    pub pid: u32,
    pub pgid: i32,
    pub started_at: Instant,
    // Used by a forthcoming "Open log" action.
    #[allow(dead_code)]
    pub log_path: PathBuf,
}

/// State of the "Add repo…" file picker. Lives in an Arc<Mutex<>> so the
/// worker thread that calls into `rfd` can hand the result back to the UI
/// without blocking egui's main loop.
#[derive(Debug, Clone)]
pub enum PickerState {
    Idle,
    InFlight,
    Picked(PathBuf),
}

/// Per-row verdict in the Servers view. Computed from the service command +
/// the current scanner snapshot before rendering, so STATE/PORTS/ACTIONS all
/// branch on the same fact.
#[derive(Debug, Clone)]
pub enum RowState {
    /// Started by Switchbard — we know its pgid.
    Running {
        pid: u32,
        pgid: i32,
        started_at: Instant,
    },
    /// Bound on this worktree's expected port but not by us. User probably
    /// started it from a terminal.
    ExternalLive { port: u16, pid: u32 },
    /// Another process is bound to this command's expected port. Starting it
    /// would EADDRINUSE.
    Blocked {
        port: u16,
        pid: u32,
        holder_label: String,
    },
    /// Nothing detected — Start is the only sensible action.
    Idle,
}

impl RowState {
    /// Build the per-row state from the raw inputs. Single source of truth
    /// for the Servers view: the table renderer must not re-derive any of
    /// these classifications from scratch.
    ///
    /// `containerized` flips the semantics for container-defined services
    /// (docker-compose entries): the listener on the expected port is held
    /// by the container runtime (Docker / OrbStack / etc.), not by any
    /// worktree-attributed process — so "held by anything ≠ blocked, it
    /// means the service is up." For non-containerized rows, a held port
    /// owned by a different worktree is still Blocked (you'd EADDRINUSE).
    pub fn compute(
        expected_port: Option<u16>,
        wt_path: &std::path::Path,
        run_for_this: Option<&ActiveRun>,
        by_port: &HashMap<u16, AttributedListener>,
        containerized: bool,
    ) -> Self {
        if let Some(run) = run_for_this {
            return RowState::Running {
                pid: run.pid,
                pgid: run.pgid,
                started_at: run.started_at,
            };
        }
        let Some(port) = expected_port else {
            return RowState::Idle;
        };
        let Some(al) = by_port.get(&port) else {
            return RowState::Idle;
        };
        if containerized {
            // For compose-defined services, the host-side port forwarder is
            // owned by the container runtime — no worktree attribution. If
            // *anything* is on the port, the service is running.
            return RowState::ExternalLive {
                port,
                pid: al.listener.pid,
            };
        }
        let same_worktree = al.worktree_path.as_deref() == Some(wt_path);
        if same_worktree {
            RowState::ExternalLive {
                port,
                pid: al.listener.pid,
            }
        } else {
            let holder_label = match (&al.repo_name, &al.worktree_branch) {
                (Some(repo), Some(b)) => format!("{repo}/{b}"),
                (Some(repo), None) => repo.clone(),
                _ => al
                    .listener
                    .cwd
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "unattributed".to_string()),
            };
            RowState::Blocked {
                port,
                pid: al.listener.pid,
                holder_label,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::DriftProbe;

    #[test]
    fn agent_filter_facets_round_trip_through_generic_memory() {
        let mut ui = switchbard_core::config::UiConfig::default();
        let state = AgentContextViewState {
            agent: AgentContextAgent::All,
            scope: ContextScope::Directory,
            kind: Some(ContextKind::Skill),
            hook_scope: Some(ContextScope::Global),
            hook_event: Some("PostToolUse".to_string()),
            hook_type: Some("command".to_string()),
            ..AgentContextViewState::default()
        };

        state.persist_filters(&mut ui);
        let restored = AgentContextViewState::restore_filters(&ui);

        assert_eq!(restored.agent, AgentContextAgent::All);
        assert_eq!(restored.scope, ContextScope::Directory);
        assert_eq!(restored.kind, Some(ContextKind::Skill));
        assert_eq!(restored.hook_scope, Some(ContextScope::Global));
        assert_eq!(restored.hook_event.as_deref(), Some("PostToolUse"));
        assert_eq!(restored.hook_type.as_deref(), Some("command"));
    }

    #[test]
    fn backlog_filter_facets_round_trip_without_transient_actions() {
        let mut ui = switchbard_core::config::UiConfig::default();
        let state = BacklogViewState {
            selected_repo: Some(PathBuf::from("/tmp/demo")),
            status_filter: "In Progress".to_string(),
            priority_filter: "high".to_string(),
            show_completed: true,
            stale_only: true,
            bulk_archive_confirm: true,
            ..BacklogViewState::default()
        };

        state.persist_filters(&mut ui);
        let restored = BacklogViewState::restore_filters(&ui);

        assert_eq!(restored.selected_repo, Some(PathBuf::from("/tmp/demo")));
        assert_eq!(restored.status_filter, "In Progress");
        assert_eq!(restored.priority_filter, "high");
        assert!(restored.show_completed);
        assert!(restored.stale_only);
        assert!(!restored.bulk_archive_confirm);
        assert!(restored.bulk_selected_tasks.is_empty());
    }

    #[test]
    fn backlog_repo_facets_restore_from_pre_rename_keys_and_persist_purges_them() {
        // A config written before the repo-vocabulary rename stored the repo
        // scope under "project"/"project_query".
        let mut ui = switchbard_core::config::UiConfig::default();
        let memory = ui.filters.entry("backlog".to_string()).or_default();
        memory
            .facets
            .insert("project".to_string(), "/tmp/legacy".to_string());
        memory
            .facets
            .insert("project_query".to_string(), "budg".to_string());

        let restored = BacklogViewState::restore_filters(&ui);
        assert_eq!(restored.selected_repo, Some(PathBuf::from("/tmp/legacy")));
        assert_eq!(restored.repo_filter, "budg");

        // Re-persisting writes the new keys and drops the legacy spellings.
        restored.persist_filters(&mut ui);
        let memory = ui.filters.get("backlog").expect("backlog memory");
        assert_eq!(
            memory.facets.get("repo").map(String::as_str),
            Some("/tmp/legacy")
        );
        assert_eq!(
            memory.facets.get("repo_query").map(String::as_str),
            Some("budg")
        );
        assert!(!memory.facets.contains_key("project"));
        assert!(!memory.facets.contains_key("project_query"));

        // New keys win when both spellings are present.
        let mut both = switchbard_core::config::UiConfig::default();
        let memory = both.filters.entry("backlog".to_string()).or_default();
        memory
            .facets
            .insert("project".to_string(), "/tmp/old".to_string());
        memory
            .facets
            .insert("repo".to_string(), "/tmp/new".to_string());
        let restored = BacklogViewState::restore_filters(&both);
        assert_eq!(restored.selected_repo, Some(PathBuf::from("/tmp/new")));
    }

    fn ready_probe(ahead: u32, behind: u32, base: &str) -> Option<DriftProbe> {
        Some(DriftProbe::Ready {
            base: base.to_string(),
            ahead,
            behind,
        })
    }

    #[test]
    fn ignored_file_summary_keeps_total_and_bounded_preview() {
        let summary = FileListSummary::from_lines(
            vec![
                "!! target/".to_string(),
                "!! node_modules/".to_string(),
                "!! dist/".to_string(),
            ],
            2,
        );

        assert_eq!(summary.total, 3);
        assert_eq!(summary.preview, vec!["!! target/", "!! node_modules/"]);
    }

    /// A worktree whose cached probes all came back clean must evaluate to
    /// the same verdict the sweep would reach - that agreement is the whole
    /// reason this adapter exists.
    #[test]
    fn cached_probes_for_a_clean_merged_idle_worktree_evaluate_to_safe() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            ignored_files: Some(FileListSummary::from_lines(vec![], 4)),
            // 0 unlanded, matching the Merged staleness below. The old
            // fixture said "4 ahead" here while claiming Merged - a state the
            // app can no longer produce, now that both come from the same
            // patch-equivalence probe against the same base.
            trunk: Some(TrunkDivergence {
                base: "main".into(),
                unlanded: 0,
                ancestry_ahead: 0,
                behind: 3,
            }),
            remote_drift: ready_probe(2, 1, "origin/feature"),
            staleness: Some(WorktreeStaleness::Merged {
                base: "main".into(),
                evidence: switchbard_core::LandedEvidence::Ancestry,
            }),
            lock: Fact::Known(None),
            ..Default::default()
        };

        let facts = removal_facts(false, &meta, AttachedProcesses::default());
        let safety = switchbard_core::RemovalSafety::evaluate(
            &facts,
            switchbard_core::RemovalIntent::WorktreeAndBranch,
        );
        assert_eq!(
            safety.verdict(),
            switchbard_core::RemovalVerdict::Safe,
            "{}",
            safety.tooltip()
        );
    }

    /// `WorktreeMeta`'s `None` means "no answer yet", and the adapter must
    /// carry that through as `Pending`. A half-probed row that evaluated to
    /// `Safe` would be a green light nothing verified.
    #[test]
    fn an_unprobed_worktree_defers_rather_than_passing() {
        let facts = removal_facts(
            false,
            &WorktreeMeta::default(),
            AttachedProcesses::default(),
        );
        assert_eq!(facts.dirty_files, Fact::Pending);
        assert_eq!(facts.landed, Fact::Pending);
        assert_eq!(facts.lock, Fact::Pending);
        let safety = switchbard_core::RemovalSafety::evaluate(
            &facts,
            switchbard_core::RemovalIntent::WorktreeAndBranch,
        );
        assert_eq!(safety.verdict(), switchbard_core::RemovalVerdict::Checking);
    }

    /// The bug the row badge shipped with: merged-ness was not one of its
    /// checks, so an unlanded worktree read "remove ok" on the row while the
    /// sweep routed it to "needs review" in the same frame.
    #[test]
    fn an_unlanded_worktree_no_longer_reads_as_removable() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            staleness: Some(WorktreeStaleness::Live),
            lock: Fact::Known(None),
            ..Default::default()
        };

        let facts = removal_facts(false, &meta, AttachedProcesses::default());
        let safety = switchbard_core::RemovalSafety::evaluate(
            &facts,
            switchbard_core::RemovalIntent::WorktreeAndBranch,
        );
        assert_eq!(safety.verdict(), switchbard_core::RemovalVerdict::Blocked);
        assert!(safety
            .blocking_reason()
            .unwrap()
            .contains("Not fully merged"));
    }

    /// The badge never measured a commit count, so the adapter must not
    /// invent one for the row to print.
    #[test]
    fn an_unlanded_worktree_reports_no_commit_count_it_never_measured() {
        let meta = WorktreeMeta {
            staleness: Some(WorktreeStaleness::NoUpstream),
            ..Default::default()
        };
        assert_eq!(
            removal_facts(false, &meta, AttachedProcesses::default()).landed,
            Fact::Known(Landed::No {
                commits: None,
                base: None
            })
        );
    }

    /// A failed staleness probe must block, not silently pass and not be
    /// mistaken for "still loading".
    #[test]
    fn an_unclassifiable_worktree_blocks_rather_than_passing() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            staleness: Some(WorktreeStaleness::Unknown),
            lock: Fact::Known(None),
            ..Default::default()
        };

        let facts = removal_facts(false, &meta, AttachedProcesses::default());
        assert!(matches!(facts.landed, Fact::Unavailable(_)));
        let safety = switchbard_core::RemovalSafety::evaluate(
            &facts,
            switchbard_core::RemovalIntent::WorktreeAndBranch,
        );
        assert_eq!(safety.verdict(), switchbard_core::RemovalVerdict::Blocked);
    }

    #[test]
    fn ignored_files_reach_the_shared_rules_as_a_count_only() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            ignored_files: Some(FileListSummary::from_lines(
                vec!["!! target/".to_string(), "!! node_modules/".to_string()],
                2,
            )),
            staleness: Some(WorktreeStaleness::Merged {
                base: "main".into(),
                evidence: switchbard_core::LandedEvidence::Ancestry,
            }),
            lock: Fact::Known(None),
            ..Default::default()
        };

        let facts = removal_facts(false, &meta, AttachedProcesses::default());
        assert_eq!(facts.ignored_files, Some(2));
        let safety = switchbard_core::RemovalSafety::evaluate(
            &facts,
            switchbard_core::RemovalIntent::WorktreeAndBranch,
        );
        assert_eq!(safety.verdict(), switchbard_core::RemovalVerdict::Safe);
        assert!(safety
            .tooltip()
            .contains("2 ignored files would also be deleted"));
    }

    /// A dispatched agent holds its worktree; an unauthenticatable sidecar is
    /// not proof it let go.
    #[test]
    fn a_dispatch_run_holds_its_worktree_unless_proven_gone() {
        use switchbard_core::dispatch_inspect::{DispatchRunLiveness, SidecarDoubt};

        assert!(dispatch_run_holds_worktree(&DispatchRunLiveness::Alive {
            pgid: 42,
            supervised: true
        }));
        assert!(dispatch_run_holds_worktree(&DispatchRunLiveness::Alive {
            pgid: 42,
            supervised: false
        }));
        assert!(
            dispatch_run_holds_worktree(&DispatchRunLiveness::Unverifiable(
                SidecarDoubt::ProbeFailed
            )),
            "a sidecar we can't authenticate is not proof the agent is gone"
        );
        assert!(!dispatch_run_holds_worktree(&DispatchRunLiveness::Gone));
        assert!(!dispatch_run_holds_worktree(
            &DispatchRunLiveness::NoSidecar
        ));
    }

    #[test]
    fn pre_rename_saved_ids_land_on_the_new_lens_and_sort_key() {
        assert_eq!(
            BacklogLens::from_saved_id("projects"),
            BacklogLens::Projects
        );
        assert_eq!(
            BacklogLens::from_saved_id("milestones"),
            BacklogLens::Projects,
            "a saved view from before the Linear-hierarchy rename restores"
        );
        assert_eq!(
            BacklogTaskSortKey::from_saved_id("project"),
            BacklogTaskSortKey::Project
        );
        assert_eq!(
            BacklogTaskSortKey::from_saved_id("milestone"),
            BacklogTaskSortKey::Project
        );
    }

    #[test]
    fn project_name_facet_restores_from_the_legacy_milestone_key_and_purges_it() {
        let mut ui = switchbard_core::config::UiConfig::default();
        let memory = ui.filters.entry("backlog".to_string()).or_default();
        memory
            .facets
            .insert("milestone".to_string(), "Lucella cutover".to_string());

        let restored = BacklogViewState::restore_filters(&ui);
        assert_eq!(restored.project_filter, "Lucella cutover");

        restored.persist_filters(&mut ui);
        let memory = ui.filters.get("backlog").expect("backlog memory");
        assert_eq!(
            memory.facets.get("project_name").map(String::as_str),
            Some("Lucella cutover")
        );
        assert!(!memory.facets.contains_key("milestone"));
    }
}
