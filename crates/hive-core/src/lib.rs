pub mod scanner;
pub mod types;
pub mod attribution;
pub mod kill;
pub mod worktree;
pub mod git_probe;
pub mod workflow;
pub mod spawn;
pub mod open_url;

pub use scanner::scan_listeners;
pub use types::{AttributedListener, LocalListener, Repo, WorktreeRef};
pub use attribution::attribute;
pub use kill::{kill_pgid, KillOutcome};
pub use worktree::{enumerate_worktrees, WorktreeEntry};
pub use git_probe::{humanize_age, probe_ahead_behind, probe_dirty, probe_head_commit_time};
pub use workflow::{detect_services, DetectedService, ServiceSource};
pub use spawn::{spawn_in_session, SpawnedRun};
pub use open_url::{open_url, url_for_port, BROWSER_APP_NAMES};
