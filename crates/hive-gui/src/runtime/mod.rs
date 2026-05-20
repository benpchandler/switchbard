//! Plain-data domain types used by the GUI layer, plus the worktree-expansion
//! helper that bridges configured `Repo`s to the live list of `WorktreeRef`s.
//!
//! Types mirror the user's mental model:
//! - `WorktreeMeta` = git probe results for one worktree (dirty, ahead/behind, age).
//! - `ActiveRun`    = a process Hive launched that's still going.
//! - `PickerState`  = the rfd file-picker hand-off.
//! - `RowState`     = the unified verdict for a Servers-view row (drives the
//!   STATE/PORTS/ACTIONS columns from a single decision).
//! - `ViewMode`     = which tab the user is on.

pub mod worktrees;

use hive_core::{AttributedListener, DriftDetail};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

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
    /// Set when the probe completes; kept for a future "stale data" badge in
    /// the UI. Currently unread.
    #[allow(dead_code)]
    pub probed_at: Option<Instant>,
}

impl WorktreeMeta {
    /// True if the porcelain probe finished and reported at least one file.
    pub fn is_dirty(&self) -> Option<bool> {
        self.dirty_files.as_ref().map(|v| !v.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Listeners,
    Worktrees,
    Servers,
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
    /// Started by Hive — we know its pgid.
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
    pub fn compute(
        expected_port: Option<u16>,
        wt_path: &std::path::Path,
        run_for_this: Option<&ActiveRun>,
        by_port: &HashMap<u16, AttributedListener>,
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
