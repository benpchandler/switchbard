pub mod agent_context;
pub mod attribution;
pub mod backlog;
pub mod backlog_relations;
pub mod backlog_stats;
pub mod backlog_triage;
pub mod boot_time;
pub mod classify;
pub mod config;
pub mod discover;
pub mod dispatch;
pub mod dispatch_inspect;
pub mod dispatch_kill;
pub mod expected_port;
mod git_env;
pub mod git_probe;
pub mod instance_lock;
pub mod kill;
pub mod landing;
pub mod open_url;
pub mod refine;
pub mod removal_safety;
pub mod resolve;
pub mod scanner;
pub mod spawn;
pub mod types;
pub mod workflow;
pub mod worktree;
pub mod worktree_create;
pub mod worktree_remove;
pub mod worktree_size;

pub use agent_context::{
    agent_context_cache_path, agent_context_needs_rescan, load_agent_context_cache,
    load_agent_context_cache_from, read_context_preview, save_agent_context_cache,
    save_agent_context_cache_to, scan_agent_context, AgentContextItem, AgentContextMap, AgentHook,
    AgentHookWarning, AgentKind, ContextKind, ContextScope,
};
pub use attribution::attribute;
pub use backlog::{
    append_backlog_notes, append_task_acceptance_criteria, append_task_notes, archive_backlog_task,
    assignable_statuses, body_round_trips, check_in_goal, complete_backlog_task,
    create_backlog_task, create_goal, create_initiative_def, create_project_def,
    create_task_allocating_id, edit_backlog_task, edit_initiative_def, edit_project_def,
    expedite_task, is_backlog_repo, load_backlog_repo, missing_standard_statuses, next_task_id,
    ordered_status_vocabulary, parse_backlog_day, rank_project, rank_project_move, rank_task,
    rank_task_move, replace_task_section, roll_goals, set_backlog_acceptance_checked,
    set_backlog_dod_checked, set_backlog_final_summary, set_backlog_label, set_task_checklist_item,
    set_task_label, set_task_list_field, set_task_priority, set_task_project, set_task_status,
    set_task_title, swap_backlog_label, swap_task_label, task_file_round_trips, unexpedite_task,
    unrank_project, unrank_task, write_new_task_file, BacklogChecklistItem, BacklogRepo,
    BacklogTask, BacklogTaskPatch, BacklogTaskSource, GoalCheckIn, GoalDef, GoalMeasure, GoalWeek,
    InitiativeDef, InitiativeDefPatch, NewBacklogTask, NewGoal, NewInitiativeDef, NewProjectDef,
    ProjectDef, ProjectDefPatch, RankMove, RankPlacement, RepoRanking, TaskChecklist,
    TaskListField, TaskSection, WriteOutcome, ACTIVE_BRANCH_DAYS, BACKLOG_PRIORITIES,
    BACKLOG_STATUSES, CANONICAL_STATUS_ORDER, DEFAULT_PROJECT_STATUS, PROJECT_STATUSES,
    STANDARD_STATUSES,
};
pub use backlog_relations::{
    ancestor_depth, blocking_dependencies, blocks, children, dependency_statuses,
    effective_project, is_blocked, is_newly_unblocked, subtask_progress,
};
pub use backlog_stats::{
    compute_burndown, compute_burndown_by_project, compute_cross_repo_stats, compute_goal_statuses,
    compute_hierarchy_rollup, week_monday_of, BurndownPoint, BurndownSeries, CrossRepoStats,
    GoalPace, GoalStatus, HierarchyRollup, InitiativeRollup, ProjectRollup, RepoStats,
};
pub use backlog_triage::{
    find_hub_repo, load_ordering_overlay, parse_backlog_datetime_unix, triage_entry_from_task,
    triage_rank, OrderingOverlay, TriageDue, TriageEntry, TriagePriority,
};
pub use classify::{classify_command, classify_script_body, ServerLikelihood};
pub use discover::{auto_scan_roots, discover_repos, DiscoveredRepo};
pub use dispatch::{
    build_dispatch_prompt, claim_task_for_dispatch, dispatch_branch_name, dispatch_log_dir,
    dispatch_log_stem, dispatch_one, dispatch_pid_path, dispatch_worktree_path,
    drain_dispatch_queue, list_dispatch_queue, parse_dispatch_sidecar, read_dispatch_sidecar,
    release_as_dispatched, release_as_failed, select_batch, sweep_dead_sidecar, DispatchOptions,
    DispatchOutcome, DispatchResult, DispatchSidecar, DEFAULT_MAX_CONCURRENT, DEFAULT_MAX_TURNS,
    DEFAULT_STALE_AFTER, DISPATCHED_LABEL, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL,
    DISPATCH_IN_PROGRESS_STATUS, DISPATCH_LABEL, DISPATCH_REVIEW_STATUS, SIDECAR_VERSION,
};
pub use dispatch_kill::{kill_dispatch_run, DispatchKillOutcome, KillRefusal};
pub use expected_port::{default_port_for_service, expected_port};
pub use git_env::git_cmd;
pub use git_probe::{
    humanize_age, probe_ahead_behind, probe_dirty_files, probe_drift_detail, probe_fetch_age,
    probe_head_commit_time, probe_ignored_files, probe_recent_commits, probe_ref_drift,
    probe_ref_drift_detail, probe_remote_drift, probe_trunk_detail, probe_trunk_divergence,
    probe_worktree_staleness, staleness_from_trunk, CommitSummary, DriftDetail, DriftProbe,
    TrunkDetail, TrunkDivergence, WorktreeStaleness,
};
pub use kill::{kill_pgid, KillOutcome};
pub use landing::{probe_pr_state, probe_push_state, LandingStage, PrState, PushState};
pub use open_url::{open_url, url_for_port, BROWSER_APP_NAMES};
pub use refine::{
    build_refine_patch, build_refine_prompt, describe_refine_outcome, describe_refine_result,
    normalize_criterion, parse_refine_response, refine_log_stem, refine_task, RefineOptions,
    RefineOutcome, RefinePlan, RefineResult, RefineSuggestion, DEFAULT_REFINE_MAX_TURNS,
    DEFAULT_REFINE_TIMEOUT, REFINED_MARKER,
};
pub use removal_safety::{
    probe_facts, probe_worktree_lock, AttachedProcesses, CheckOutcome, CheckResult, Fact, Landed,
    LandedEvidence, RemovalCheck, RemovalFacts, RemovalIntent, RemovalSafety, RemovalVerdict,
};
pub use resolve::{resolve, ResolvedService};
pub use scanner::scan_listeners;
pub use spawn::{spawn_in_session, wait_for_exit, SpawnedRun, WaitOutcome};
pub use types::{AttributedListener, LocalListener, Repo, WorktreeAlias, WorktreeRef};
pub use workflow::{detect_services, DetectedService, ServiceSource};
pub use worktree::{enumerate_worktrees, WorktreeEntry};
pub use worktree_create::{create_worktree, CreateBranchMode, CreateWorktreeOptions};
pub use worktree_remove::{
    assess_branch_delete, collect_dirty_files, delete_branch, is_primary_worktree, remove_worktree,
    BranchDeleteAssessment, DirtyFile,
};
pub use worktree_size::{humanize_size, probe_worktree_size};
