pub mod attribution;
pub mod classify;
pub mod config;
pub mod expected_port;
pub mod git_probe;
pub mod kill;
pub mod open_url;
pub mod scanner;
pub mod spawn;
pub mod types;
pub mod workflow;
pub mod worktree;

pub use attribution::attribute;
pub use classify::{classify_command, classify_script_body, ServerLikelihood};
pub use expected_port::expected_port;
pub use git_probe::{
    humanize_age, probe_ahead_behind, probe_dirty_files, probe_drift_detail, probe_fetch_age,
    probe_head_commit_time, CommitSummary, DriftDetail,
};
pub use kill::{kill_pgid, KillOutcome};
pub use open_url::{open_url, url_for_port, BROWSER_APP_NAMES};
pub use scanner::scan_listeners;
pub use spawn::{spawn_in_session, SpawnedRun};
pub use types::{AttributedListener, LocalListener, Repo, WorktreeRef};
pub use workflow::{detect_services, DetectedService, ServiceSource};
pub use worktree::{enumerate_worktrees, WorktreeEntry};
