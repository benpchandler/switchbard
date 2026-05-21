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

pub mod worktrees;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use switchbard_core::{AttributedListener, CommitSummary, DriftDetail};

#[derive(Debug, Clone, Default)]
pub struct WorktreeMeta {
    /// `Some(files)` after the porcelain probe completes — empty means clean.
    /// `None` while the probe hasn't returned yet (or it failed).
    pub dirty_files: Option<Vec<String>>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// Commit lists behind the ahead/behind counts (capped). Used to build the
    /// drift-cell tooltip's "showing N of M" body.
    pub drift_detail: Option<DriftDetail>,
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

impl WorktreeMeta {
    /// True if the porcelain probe finished and reported at least one file.
    pub fn is_dirty(&self) -> Option<bool> {
        self.dirty_files.as_ref().map(|v| !v.is_empty())
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
