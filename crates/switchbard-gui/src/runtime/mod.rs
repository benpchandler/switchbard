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

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use switchbard_core::{
    AgentKind, AttributedListener, BranchDeleteAssessment, CommitSummary, ContextKind,
    ContextScope, DirtyFile, DriftDetail, DriftProbe,
};

/// Top-level central-panel tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewTab {
    #[default]
    Servers,
    AgentContext,
}

/// Agent target selected in the Agent Context explorer.
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

/// UI-local selection state for the Agent Context explorer.
#[derive(Debug, Clone)]
pub struct AgentContextViewState {
    pub scope: ContextScope,
    pub kind: Option<ContextKind>,
    pub selected_id: Option<String>,
    pub agent: AgentContextAgent,
    pub global_kind: Option<ContextKind>,
    pub global_selected_id: Option<String>,
    pub global_open: bool,
    pub pinned_repo: Option<String>,
}

impl Default for AgentContextViewState {
    fn default() -> Self {
        Self {
            scope: ContextScope::Local,
            kind: None,
            selected_id: None,
            agent: AgentContextAgent::Claude,
            global_kind: None,
            global_selected_id: None,
            global_open: false,
            pinned_repo: None,
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
    pub branch_assessment: Option<BranchDeleteAssessment>,
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
    /// `HEAD` compared with the repo's local `main` branch.
    pub main_drift: Option<DriftProbe>,
    /// `HEAD` compared with the current branch's configured upstream remote.
    pub remote_drift: Option<DriftProbe>,
    /// Commit lists behind the local-main comparison, capped for tooltip use.
    pub main_drift_detail: Option<DriftDetail>,
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

/// One locally-verifiable criterion behind "safe to delete this worktree".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteSafetyCriterionKind {
    LinkedWorktree,
    FilesClear,
    NoProcesses,
}

impl DeleteSafetyCriterionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::LinkedWorktree => "linked worktree",
            Self::FilesClear => "files clear",
            Self::NoProcesses => "no processes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteSafetyCriterion {
    pub kind: DeleteSafetyCriterionKind,
    pub satisfied: bool,
    pub tooltip: String,
}

pub fn delete_safety_criteria(
    is_primary: bool,
    meta: &WorktreeMeta,
    listener_count: usize,
    active_run_count: usize,
) -> Vec<DeleteSafetyCriterion> {
    let (files_clear, files_tip) =
        files_safe_to_delete(meta.dirty_files.as_deref(), meta.ignored_files.as_ref());
    let no_processes = listener_count == 0 && active_run_count == 0;
    let process_tip = match (listener_count, active_run_count) {
        (0, 0) => "No attributed listeners or Switchbard runs".to_string(),
        (listeners, runs) => format!(
            "{listeners} attributed listener{} and {runs} Switchbard run{} still tied here",
            plural(listeners),
            plural(runs)
        ),
    };

    vec![
        DeleteSafetyCriterion {
            kind: DeleteSafetyCriterionKind::LinkedWorktree,
            satisfied: !is_primary,
            tooltip: if is_primary {
                "Primary checkout cannot be removed here".to_string()
            } else {
                "Linked worktree can be removed without dropping the repo".to_string()
            },
        },
        DeleteSafetyCriterion {
            kind: DeleteSafetyCriterionKind::FilesClear,
            satisfied: files_clear,
            tooltip: files_tip,
        },
        DeleteSafetyCriterion {
            kind: DeleteSafetyCriterionKind::NoProcesses,
            satisfied: no_processes,
            tooltip: process_tip,
        },
    ]
}

fn files_safe_to_delete(
    dirty_files: Option<&[String]>,
    ignored_files: Option<&FileListSummary>,
) -> (bool, String) {
    let Some(dirty) = dirty_files else {
        return (false, "File check pending or failed".to_string());
    };
    let ignored = ignored_files;
    if dirty.is_empty() {
        let ignored_note = ignored_summary_note(ignored, " would also be removed");
        return (
            true,
            format!("No uncommitted or untracked files{ignored_note}"),
        );
    }

    let ignored_note = ignored_summary_note(ignored, " also present");
    let review_verb = if dirty.len() == 1 { "needs" } else { "need" };
    (
        false,
        format!(
            "{} changed/untracked file{} {review_verb} review{}",
            format_count(dirty.len()),
            plural(dirty.len()),
            ignored_note
        ),
    )
}

fn ignored_summary_note(ignored: Option<&FileListSummary>, suffix: &str) -> String {
    let Some(ignored) = ignored else {
        return String::new();
    };
    if ignored.is_empty() {
        return String::new();
    }
    format!(
        "; {} ignored file{}{}",
        format_count(ignored.total),
        plural(ignored.total),
        suffix
    )
}

fn format_count(count: usize) -> String {
    let digits = count.to_string();
    let mut reversed = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            reversed.push(',');
        }
        reversed.push(ch);
    }
    reversed.chars().rev().collect()
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
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

    fn ready_probe(ahead: u32, behind: u32, base: &str) -> Option<DriftProbe> {
        Some(DriftProbe::Ready {
            base: base.to_string(),
            ahead,
            behind,
        })
    }

    #[test]
    fn delete_safety_criteria_are_green_for_linked_clean_idle_worktree() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            ignored_files: Some(FileListSummary::from_lines(vec![], 4)),
            main_drift: ready_probe(4, 3, "main"),
            remote_drift: ready_probe(2, 1, "origin/feature"),
            ..Default::default()
        };

        let criteria = delete_safety_criteria(false, &meta, 0, 0);

        assert_eq!(criteria.len(), 3);
        assert!(criteria.iter().all(|criterion| criterion.satisfied));
        assert!(criteria
            .iter()
            .any(|criterion| criterion.kind == DeleteSafetyCriterionKind::LinkedWorktree));
        assert!(criteria
            .iter()
            .any(|criterion| criterion.kind == DeleteSafetyCriterionKind::FilesClear));
    }

    #[test]
    fn delete_safety_criteria_treat_ignored_files_as_context_not_blockers() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            ignored_files: Some(FileListSummary::from_lines(vec!["!! .env".to_string()], 4)),
            ..Default::default()
        };

        let criteria = delete_safety_criteria(false, &meta, 0, 0);
        let files = criteria
            .iter()
            .find(|criterion| criterion.kind == DeleteSafetyCriterionKind::FilesClear)
            .unwrap();

        assert!(files.satisfied);
        assert!(files.tooltip.contains("No uncommitted or untracked files"));
        assert!(files.tooltip.contains("1 ignored"));
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

    #[test]
    fn delete_safety_ignored_file_message_is_count_only() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            ignored_files: Some(FileListSummary::from_lines(
                vec!["!! target/".to_string(), "!! node_modules/".to_string()],
                2,
            )),
            ..Default::default()
        };

        let criteria = delete_safety_criteria(false, &meta, 0, 0);
        let files = criteria
            .iter()
            .find(|criterion| criterion.kind == DeleteSafetyCriterionKind::FilesClear)
            .unwrap();

        assert_eq!(
            files.tooltip,
            "No uncommitted or untracked files; 2 ignored files would also be removed"
        );
    }

    #[test]
    fn delete_safety_criteria_block_on_dirty_or_untracked_files() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![
                " M src/main.rs".to_string(),
                "?? scratch.txt".to_string(),
            ]),
            ignored_files: Some(FileListSummary::from_lines(
                vec!["!! target/".to_string()],
                4,
            )),
            ..Default::default()
        };

        let criteria = delete_safety_criteria(false, &meta, 0, 0);
        let files = criteria
            .iter()
            .find(|criterion| criterion.kind == DeleteSafetyCriterionKind::FilesClear)
            .unwrap();

        assert!(!files.satisfied);
        assert!(files.tooltip.contains("2 changed/untracked"));
    }

    #[test]
    fn primary_or_busy_worktree_is_not_safe_to_delete() {
        let meta = WorktreeMeta {
            dirty_files: Some(vec![]),
            ignored_files: Some(FileListSummary::from_lines(vec![], 4)),
            ..Default::default()
        };

        let criteria = delete_safety_criteria(true, &meta, 1, 1);

        assert_eq!(criteria.len(), 3);
        assert!(
            !criteria
                .iter()
                .find(|criterion| criterion.kind == DeleteSafetyCriterionKind::LinkedWorktree)
                .unwrap()
                .satisfied
        );
        assert!(
            !criteria
                .iter()
                .find(|criterion| criterion.kind == DeleteSafetyCriterionKind::NoProcesses)
                .unwrap()
                .satisfied
        );
    }
}
