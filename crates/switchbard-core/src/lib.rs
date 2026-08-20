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
pub mod open_url;
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
    save_agent_context_cache_to, scan_agent_context, AgentContextItem, AgentContextMap, AgentKind,
    ContextKind, ContextScope,
};
pub use attribution::attribute;
pub use backlog::{
    append_backlog_notes, archive_backlog_task, backlog_cli_path, complete_backlog_task,
    create_backlog_task, edit_backlog_task, is_backlog_project, load_backlog_project,
    ordered_status_vocabulary, parse_backlog_day, parse_created_task_id,
    set_backlog_acceptance_checked, set_backlog_dod_checked, set_backlog_label, swap_backlog_label,
    BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskPatch, BacklogTaskSource,
    NewBacklogTask, BACKLOG_PRIORITIES, BACKLOG_STATUSES, CANONICAL_STATUS_ORDER,
    STANDARD_STATUSES,
};
pub use backlog_relations::{
    blocking_dependencies, blocks, children, dependency_statuses, is_blocked, is_newly_unblocked,
    subtask_progress,
};
pub use backlog_stats::{
    compute_burndown, compute_burndown_by_milestone, compute_cross_repo_stats, BurndownPoint,
    BurndownSeries, CrossRepoStats, RepoStats,
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
    select_batch, sweep_dead_sidecar, DispatchOptions, DispatchOutcome, DispatchResult,
    DispatchSidecar, DEFAULT_MAX_CONCURRENT, DEFAULT_MAX_TURNS, DISPATCHED_LABEL,
    DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_IN_PROGRESS_STATUS, DISPATCH_LABEL,
    DISPATCH_REVIEW_STATUS, SIDECAR_VERSION,
};
pub use dispatch_kill::{kill_dispatch_run, DispatchKillOutcome, KillRefusal};
pub use expected_port::{default_port_for_service, expected_port};
pub use git_env::git_cmd;
pub use git_probe::{
    humanize_age, probe_ahead_behind, probe_dirty_files, probe_drift_detail, probe_fetch_age,
    probe_head_commit_time, probe_ignored_files, probe_main_drift, probe_recent_commits,
    probe_ref_drift, probe_ref_drift_detail, probe_remote_drift, probe_worktree_staleness,
    CommitSummary, DriftDetail, DriftProbe, WorktreeStaleness,
};
pub use kill::{kill_pgid, KillOutcome};
pub use open_url::{open_url, url_for_port, BROWSER_APP_NAMES};
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
