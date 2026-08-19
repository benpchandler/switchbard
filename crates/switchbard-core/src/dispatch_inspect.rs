//! Read-only inspection of a dispatch run, for the Dispatch view.
//!
//! ## Why there is no run store
//!
//! Everything the UI wants to show about a run — which branch, which worktree,
//! which log, when it started, how long it has been going — is recoverable
//! from the repo root and the task id alone, because [`crate::dispatch`]
//! derives all of those from exactly those two inputs:
//!
//! - branch: [`dispatch_branch_name`]
//! - worktree: [`dispatch_worktree_path`]
//! - log/prompt: [`dispatch_log_dir`] + [`dispatch_log_stem`], and the stem
//!   carries the run's start time as a unix-seconds stamp
//!
//! So this module *rebuilds* the paths rather than reading a record the
//! pipeline wrote down. That keeps `workers::spawn_dispatch`'s documented
//! property intact — it publishes no state of its own; a task's label is its
//! state — and avoids a second store that could disagree with the labels
//! (`config` is already the repo's one "single source of truth, kept in
//! lock-step" invariant; a dispatch store would be a second one to keep
//! honest).
//!
//! It also means inspection survives a restart: Switchbard can be closed and
//! reopened mid-run and still show the elapsed time correctly, because the
//! start time lives in a filename on disk rather than in process memory.
//!
//! ## What this deliberately does not claim
//!
//! It reports **evidence**, not a verdict on liveness. There is no pid to
//! check — the pipeline blocks on its own child in a worker thread and never
//! records one — so [`DispatchRun::looks_stalled`] is expressed against the
//! run's own configured timeout rather than pretending to probe the process
//! table. An empty log is normal for a healthy in-flight run: `claude -p
//! --output-format text` emits nothing until it exits, which is the single
//! most misleading signal a user looking at these files will hit.

use crate::dispatch::{
    dispatch_branch_name, dispatch_log_dir, dispatch_log_stem, dispatch_worktree_path,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// One dispatch run, reconstructed from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchRun {
    pub task_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    /// `true` if the worktree is still on disk. A finished-and-cleaned run has
    /// its paths but no directory.
    pub worktree_exists: bool,
    /// Newest log for this task, if any run has ever started.
    pub log_path: Option<PathBuf>,
    /// Prompt actually fed to the agent, alongside the newest log.
    pub prompt_path: Option<PathBuf>,
    /// Unix seconds the run started, recovered from the log filename.
    pub started_at_unix: Option<u64>,
    /// Bytes written to the log so far. Zero is expected while running.
    pub log_bytes: u64,
    /// Unix seconds the log was last written. Because `--output-format text`
    /// flushes at exit, this is effectively *when the agent finished* — the
    /// one timestamp that distinguishes a live run from an abandoned one.
    pub log_modified_unix: Option<u64>,
}

/// How long after the agent exits the pipeline is still allowed to be working
/// before we call the run abandoned. The post-run steps are real work — commit,
/// push, then `gh pr create` behind `GH_SPACING` — so this is generous enough
/// not to libel a pipeline that is simply mid-push.
pub const RELEASE_GRACE: Duration = Duration::from_secs(120);

impl DispatchRun {
    /// How long the run has been going, or ran for, as of `now_unix`.
    /// `None` when no log exists yet (nothing has started).
    pub fn elapsed(&self, now_unix: u64) -> Option<Duration> {
        let started = self.started_at_unix?;
        Some(Duration::from_secs(now_unix.saturating_sub(started)))
    }

    /// Whether the run has outlived `timeout`. Evidence for "go look", not a
    /// claim that the process is gone — see the module doc.
    pub fn looks_stalled(&self, now_unix: u64, timeout: Duration) -> bool {
        self.elapsed(now_unix)
            .is_some_and(|elapsed| elapsed > timeout)
    }

    /// The log has content, which for `--output-format text` means the agent
    /// process has finished writing (it buffers until exit).
    pub fn log_has_output(&self) -> bool {
        self.log_bytes > 0
    }

    /// The agent finished but nothing released the claim: its output has been
    /// written, [`RELEASE_GRACE`] has since elapsed, and the caller reports the
    /// task is still labeled `dispatching`.
    ///
    /// This is the app-restarted-mid-run case. `dispatch_one` blocks in a
    /// worker thread while `spawn_in_session` makes the agent a session leader,
    /// so killing Switchbard orphans the run: the agent survives, finishes, and
    /// commits, but the push / `gh pr create` / release steps die with the
    /// parent and the task sits on `dispatching` forever.
    ///
    /// Deliberately keyed on the log's *mtime* rather than on the run
    /// exceeding its timeout. A run abandoned five minutes in is just as
    /// orphaned as one abandoned an hour in — waiting out the 30-minute
    /// timeout to say so would hide the fast case for 25 minutes.
    pub fn looks_orphaned(&self, now_unix: u64, still_claimed: bool) -> bool {
        if !still_claimed || !self.log_has_output() {
            return false;
        }
        self.log_modified_unix
            .is_some_and(|finished| now_unix.saturating_sub(finished) > RELEASE_GRACE.as_secs())
    }
}

/// Rebuild what is knowable about `task_id`'s dispatch run under `repo_root`.
/// Always succeeds: a task that was never dispatched yields a `DispatchRun`
/// with no log and a non-existent worktree, which is exactly what the view
/// should render for it.
pub fn inspect_dispatch_run(repo_root: &Path, task_id: &str) -> DispatchRun {
    let worktree_path = dispatch_worktree_path(repo_root, task_id);
    let newest = newest_log_for(task_id);

    let (log_path, started_at_unix, log_bytes, log_modified_unix) = match newest {
        Some((path, started)) => {
            let meta = std::fs::metadata(&path).ok();
            let bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            (Some(path), Some(started), bytes, modified)
        }
        None => (None, None, 0, None),
    };

    let prompt_path = started_at_unix.map(|started| {
        dispatch_log_dir().join(format!("{}-prompt.md", dispatch_log_stem(task_id, started)))
    });

    DispatchRun {
        task_id: task_id.to_string(),
        branch: dispatch_branch_name(task_id),
        worktree_exists: worktree_path.is_dir(),
        worktree_path,
        log_path,
        prompt_path,
        started_at_unix,
        log_bytes,
        log_modified_unix,
    }
}

/// Newest `dispatch-<task>-<stamp>.log` for one task, with its stamp. A task
/// re-flagged after a failure has several; the most recent is the one the user
/// means. Ordering is by the stamp *in the name*, not file mtime — the log is
/// written to over the run's lifetime, so mtime tracks last write, not start.
fn newest_log_for(task_id: &str) -> Option<(PathBuf, u64)> {
    let prefix = format!("dispatch-{}-", task_id.to_ascii_lowercase());
    let entries = std::fs::read_dir(dispatch_log_dir()).ok()?;

    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            let stamp = name.strip_prefix(&prefix)?.strip_suffix(".log")?;
            // Rejects `<task>-<stamp>-prompt.md` and any sibling task whose id
            // merely starts with this one (`task-30` vs `task-307`): a real
            // stamp is all digits.
            let stamp: u64 = stamp.parse().ok()?;
            Some((path, stamp))
        })
        .max_by_key(|(_, stamp)| *stamp)
}

/// Wall-clock seconds since the epoch, for passing to [`DispatchRun::elapsed`].
/// Taken as a parameter there rather than read internally so the arithmetic
/// stays a pure, testable function.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with(started: Option<u64>, log_bytes: u64) -> DispatchRun {
        DispatchRun {
            task_id: "task-307".to_string(),
            branch: "dispatch/task-307".to_string(),
            worktree_path: PathBuf::from("/repo/.worktrees/dispatch-task-307"),
            worktree_exists: true,
            log_path: started.map(|_| PathBuf::from("/tmp/x.log")),
            prompt_path: None,
            started_at_unix: started,
            log_bytes,
            log_modified_unix: (log_bytes > 0).then_some(started.unwrap_or(0)),
        }
    }

    #[test]
    fn elapsed_is_none_before_a_run_has_started() {
        assert_eq!(run_with(None, 0).elapsed(1_000), None);
    }

    #[test]
    fn elapsed_counts_from_the_stamp_in_the_log_name() {
        let run = run_with(Some(1_000), 0);
        assert_eq!(run.elapsed(1_420), Some(Duration::from_secs(420)));
    }

    /// Clock skew (or a stamp from the future) must not underflow the
    /// subtraction into a nonsense multi-century elapsed time.
    #[test]
    fn elapsed_saturates_rather_than_underflowing() {
        let run = run_with(Some(2_000), 0);
        assert_eq!(run.elapsed(1_000), Some(Duration::ZERO));
    }

    #[test]
    fn stalled_only_once_past_the_timeout() {
        let run = run_with(Some(0), 0);
        let timeout = Duration::from_secs(30 * 60);
        assert!(!run.looks_stalled(29 * 60, timeout));
        assert!(run.looks_stalled(31 * 60, timeout));
    }

    /// A never-started run is not stalled, however long ago "now" is.
    #[test]
    fn a_run_that_never_started_is_not_stalled() {
        assert!(!run_with(None, 0).looks_stalled(u64::MAX, Duration::from_secs(1)));
    }

    /// The empty-log-is-normal invariant the module doc calls out: an
    /// in-flight run has no output, and that must not read as failure.
    #[test]
    fn an_empty_log_is_not_treated_as_output() {
        assert!(!run_with(Some(0), 0).log_has_output());
        assert!(run_with(Some(0), 12).log_has_output());
    }

    /// A healthy in-flight run has written nothing yet, so it is never
    /// orphaned however long it has been going. This is the guard that keeps
    /// a slow-but-alive agent out of the recovery bucket.
    #[test]
    fn a_running_agent_with_no_output_is_never_orphaned() {
        let run = run_with(Some(0), 0);
        assert!(!run.looks_orphaned(u64::MAX / 2, true));
    }

    /// Right after the agent exits, the pipeline is legitimately still doing
    /// commit/push/`gh pr create` — don't call that abandoned.
    #[test]
    fn a_just_finished_run_is_within_the_release_grace() {
        let run = run_with(Some(0), 42);
        let finished = run.log_modified_unix.expect("log mtime");
        assert!(!run.looks_orphaned(finished + 5, true));
        assert!(!run.looks_orphaned(finished + RELEASE_GRACE.as_secs(), true));
    }

    /// The TASK-307 case: agent finished, grace elapsed, task still claimed.
    #[test]
    fn a_finished_run_still_claimed_past_the_grace_is_orphaned() {
        let run = run_with(Some(0), 42);
        let finished = run.log_modified_unix.expect("log mtime");
        assert!(run.looks_orphaned(finished + RELEASE_GRACE.as_secs() + 1, true));
    }

    /// Once the pipeline released the claim the same evidence is just a
    /// normal finished run — orphaned is about the claim, not the log.
    #[test]
    fn a_released_run_is_not_orphaned_however_old() {
        let run = run_with(Some(0), 42);
        assert!(!run.looks_orphaned(u64::MAX / 2, false));
    }

    #[test]
    fn inspecting_a_never_dispatched_task_is_empty_but_still_names_its_paths() {
        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), "TASK-999");
        assert_eq!(run.branch, "dispatch/task-999");
        assert!(!run.worktree_exists);
        assert_eq!(run.log_path, None);
        assert_eq!(run.started_at_unix, None);
        assert!(run.worktree_path.ends_with(".worktrees/dispatch-task-999"));
    }
}
