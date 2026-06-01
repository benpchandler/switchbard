pub mod agent_context;
pub mod attribution;
pub mod classify;
pub mod config;
pub mod discover;
pub mod expected_port;
pub mod git_probe;
pub mod kill;
pub mod open_url;
pub mod resolve;
pub mod scanner;
pub mod spawn;
pub mod types;
pub mod workflow;
pub mod worktree;
pub mod worktree_remove;

pub use agent_context::{
    agent_context_cache_path, agent_context_needs_rescan, load_agent_context_cache,
    load_agent_context_cache_from, read_context_preview, save_agent_context_cache,
    save_agent_context_cache_to, scan_agent_context, AgentContextItem, AgentContextMap, AgentKind,
    ContextKind, ContextScope,
};
pub use attribution::attribute;
pub use classify::{classify_command, classify_script_body, ServerLikelihood};
pub use discover::{auto_scan_roots, discover_repos, DiscoveredRepo};
pub use expected_port::{default_port_for_service, expected_port};
pub use git_probe::{
    humanize_age, probe_ahead_behind, probe_dirty_files, probe_drift_detail, probe_fetch_age,
    probe_head_commit_time, probe_main_drift, probe_recent_commits, probe_ref_drift,
    probe_ref_drift_detail, probe_remote_drift, CommitSummary, DriftDetail, DriftProbe,
};
pub use kill::{kill_pgid, KillOutcome};
pub use open_url::{open_url, url_for_port, BROWSER_APP_NAMES};
pub use resolve::{resolve, ResolvedService};
pub use scanner::scan_listeners;
pub use spawn::{spawn_in_session, SpawnedRun};
pub use types::{AttributedListener, LocalListener, Repo, WorktreeRef};
pub use workflow::{detect_services, DetectedService, ServiceSource};
pub use worktree::{enumerate_worktrees, WorktreeEntry};
pub use worktree_remove::{collect_dirty_files, is_primary_worktree, remove_worktree, DirtyFile};
