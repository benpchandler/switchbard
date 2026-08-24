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
//! Most of what it reports is **evidence**, not a verdict.
//! [`DispatchRun::looks_stalled`] is expressed against the run's own
//! configured staleness threshold (`DispatchOptions::stale_after`), not
//! against process liveness — and, since TASK-46 removed the wall-clock kill
//! that threshold used to arm, it is now advisory *only*: a stalled run is
//! still running, just flagged for a human to go check on. An empty log is
//! normal for a healthy in-flight run either way: `claude -p --output-format
//! text` emits nothing until it exits, which is the single most misleading
//! signal a user looking at these files will hit.
//!
//! ## The one place this *does* adjudicate: [`DispatchRunLiveness`]
//!
//! The Kill affordance is the exception, because the cost of being wrong is
//! not a misleading label — it is SIGKILL to a process group that has nothing
//! to do with Switchbard. "Here is a pgid, killing it is your call" is not an
//! honest offer when the app cannot tell whether that pgid is still the agent,
//! so this module probes the process table for exactly that question and
//! **fails closed**: anything short of positive identification yields
//! [`DispatchRunLiveness::Unverifiable`] and no kill handle.
//!
//! Identification is not "a process with that group id exists" — that is the
//! very thing pid recycling defeats. It is, precisely: the sidecar was minted
//! this boot (`crate::boot_time`), *and* a substring match for this run's log
//! stem (`dispatch-<task>-<stamp>`) hits the command line of some live process
//! **in that process group**.
//!
//! Two things make that sound rather than merely suggestive, and both are
//! worth stating because neither is obvious:
//!
//! - The match is *group-anchored*. It is not a search of the whole process
//!   table for a string; the candidate set is already narrowed to the recorded
//!   pgid, and within one boot a process group contains only the tree the
//!   pipeline spawned into it. So a hit means "this group is still running the
//!   thing we put there".
//! - The stem embeds the task id and the run's start unix-seconds, so it is
//!   unique per run — a *different* dispatch run that happened to land on this
//!   pgid would carry its own stem and miss.
//!
//! What this does not claim: that no unrelated process anywhere could ever
//! contain that substring in its argv. One could (a text editor with the log
//! open, a `grep` for it). It would have to be *in the recorded process
//! group*, on this boot, for that to matter — which is why the boot gate and
//! the group filter are load-bearing rather than belt-and-braces.
//!
//! Probing is a `ps` subprocess, so it runs on the worker cadence
//! (`workers::refresh_dispatch_runs`) like every other read here, never on the
//! render path — and only for runs that actually have a sidecar, which is the
//! rare case.
//!
//! This module stays **read-only**. Probing observes; it never removes a
//! sidecar or edits a task. Cleaning up a sidecar whose group is verifiably
//! gone belongs to `crate::dispatch::sweep_dead_sidecar`, called by the
//! worker, next to the format's other writers.

use crate::dispatch::{
    dispatch_branch_name, dispatch_log_dir, dispatch_log_stem, dispatch_pid_path,
    dispatch_worktree_path, read_dispatch_sidecar, DispatchSidecar,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Why a run's recorded process group cannot be vouched for. Every variant
/// leads to the same UI outcome — no Kill button — but they read very
/// differently to a user, so the reason travels with the verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarDoubt {
    /// A pre-versioning bare-pgid sidecar. It carries no boot epoch, so there
    /// is no way to tell whether its number survived a reboot.
    LegacyFormat,
    /// Written under a different boot. The pid space has been reissued
    /// wholesale since; the number now names a stranger.
    StaleBoot,
    /// The sidecar parsed and belongs to this boot, but the process-table
    /// probe itself failed (no `ps`, or it errored), so nothing was
    /// identified either way.
    ProbeFailed,
}

impl SidecarDoubt {
    /// One line a UI can show in place of the Kill button, so "no button
    /// here" reads as a decision rather than as a missing feature.
    pub fn explain(self) -> &'static str {
        match self {
            SidecarDoubt::LegacyFormat => {
                "this run predates the current sidecar format — its process can't be identified"
            }
            SidecarDoubt::StaleBoot => {
                "this run started before the last reboot — its process id no longer means anything"
            }
            SidecarDoubt::ProbeFailed => {
                "couldn't read the process table to confirm this run is still the one recorded"
            }
        }
    }
}

/// What can be *proved* about a run's process group right now.
///
/// The ordering of concerns is deliberate: a kill handle is only produced by
/// [`DispatchRunLiveness::Alive`], and that variant is only constructed after
/// positive identification. There is no "probably" state, because a UI given
/// one would render a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchRunLiveness {
    /// No sidecar on disk. The normal, quiet state of every run that ever
    /// released cleanly, and of every run that has not started.
    NoSidecar,
    /// A sidecar exists but cannot be authenticated. Fail closed.
    Unverifiable(SidecarDoubt),
    /// The sidecar authenticated and the group is positively **gone**. This
    /// is not a non-answer: combined with a task still labelled `dispatching`
    /// it is proof that a run died without anything releasing it, which is
    /// what turns a permanently-"in flight" row into one asking for
    /// attention.
    Gone,
    /// The group is alive and still running *this* run's agent.
    ///
    /// `supervised` is the other half a UI needs: `true` means the Switchbard
    /// process that spawned the agent is this one, still blocked in
    /// `wait_for_exit`, so when the run ends the pipeline is there to release
    /// the task. `false` means the agent outlived its supervisor — nothing is
    /// blocked on it any more, so killing it releases nothing (there was
    /// never a deadline to lose either way — see TASK-46).
    Alive { pgid: i32, supervised: bool },
}

impl DispatchRunLiveness {
    /// The pgid a Kill affordance may signal, or `None`. The single gate — a
    /// caller that goes around this to read a pgid from anywhere else is
    /// reintroducing the bug this type exists to close.
    pub fn killable_pgid(&self) -> Option<i32> {
        match self {
            DispatchRunLiveness::Alive { pgid, .. } => Some(*pgid),
            _ => None,
        }
    }

    /// Whether a live agent still has a pipeline behind it. `false` for every
    /// non-`Alive` verdict, so a caller can ask this without first matching.
    pub fn is_supervised(&self) -> bool {
        matches!(
            self,
            DispatchRunLiveness::Alive {
                supervised: true,
                ..
            }
        )
    }

    /// Positive proof that the run's process group is gone.
    pub fn is_gone(&self) -> bool {
        matches!(self, DispatchRunLiveness::Gone)
    }

    /// Why there is no kill handle, when the reason is worth showing.
    pub fn doubt(&self) -> Option<SidecarDoubt> {
        match self {
            DispatchRunLiveness::Unverifiable(doubt) => Some(*doubt),
            _ => None,
        }
    }
}

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
    /// What could be proved about the run's process group at the last worker
    /// refresh. The *only* source of a kill handle — see the module doc's
    /// "the one place this does adjudicate" section.
    pub liveness: DispatchRunLiveness,
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

    /// Whether the run has outlived `stale_after`. Evidence for "go look", not
    /// a claim that the process is gone, and — since TASK-46 removed the
    /// wall-clock kill this threshold used to arm — never a reason anything
    /// will kill the run either. See the module doc.
    pub fn looks_stalled(&self, now_unix: u64, stale_after: Duration) -> bool {
        self.elapsed(now_unix)
            .is_some_and(|elapsed| elapsed > stale_after)
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
    /// exceeding `stale_after`. A run abandoned five minutes in is just as
    /// orphaned as one abandoned an hour in — waiting out the 30-minute
    /// default threshold to say so would hide the fast case for 25 minutes.
    pub fn looks_orphaned(&self, now_unix: u64, still_claimed: bool) -> bool {
        if !still_claimed || !self.log_has_output() {
            return false;
        }
        self.log_modified_unix
            .is_some_and(|finished| now_unix.saturating_sub(finished) > RELEASE_GRACE.as_secs())
    }
}

impl DispatchRun {
    /// Claimed, but nothing is running. The single predicate both the Dispatch
    /// view's sectioning and the top-bar chip's counts consult, so the list and
    /// the ambient badge can never disagree about which runs are stuck.
    ///
    /// Two independent kinds of evidence, because they catch different deaths:
    ///
    /// - [`Self::looks_orphaned`] — the agent *finished* (its output is
    ///   flushed) and nothing released the claim. The app-exited-after-the-run
    ///   case.
    /// - [`DispatchRunLiveness::Gone`] — the group is positively dead with no
    ///   output at all. The app-was-force-quit-*during*-the-run case, which
    ///   `looks_orphaned` cannot see: an empty log is indistinguishable from a
    ///   healthy in-flight run by file evidence alone, so before the liveness
    ///   probe existed such a task sat under "In flight" forever.
    pub fn is_abandoned(&self, now_unix: u64, still_claimed: bool) -> bool {
        still_claimed && (self.looks_orphaned(now_unix, true) || self.liveness.is_gone())
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
    // Only the *newest* run's sidecar is consulted: an older run's sidecar
    // still on disk describes a group nobody should be offered a button for.
    let liveness = match started_at_unix {
        Some(started) => probe_liveness(task_id, started),
        None => DispatchRunLiveness::NoSidecar,
    };

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
        liveness,
    }
}

/// Decide what can be proved about one run's process group.
///
/// Reads as a sequence of things that must *all* hold before a kill handle
/// comes out the far end; any one failing short-circuits to a verdict that
/// offers nothing. The order is cheapest-and-most-decisive first: a missing
/// file needs no parse, an unparseable one needs no boot comparison, and a
/// stale-boot sidecar needs no `ps` at all.
///
/// Public because it is the authenticated path, and there must be exactly
/// one. [`crate::dispatch_kill::kill_dispatch_run`] re-runs *this* function
/// immediately before it signals rather than trusting the verdict cached on a
/// `DispatchRun`, which can be minutes old — see that module's doc. A second,
/// bespoke "is it still there?" check written next to the kill would be a
/// second place for the identity rules to drift.
pub fn probe_liveness(task_id: &str, started_at_unix: u64) -> DispatchRunLiveness {
    let path = dispatch_pid_path(task_id, started_at_unix);
    if !path.exists() {
        return DispatchRunLiveness::NoSidecar;
    }
    // Present but unreadable as a current-format sidecar. The dominant real
    // case is the pre-versioning bare-pgid file, which is also the dangerous
    // one — it looks like a perfectly good pgid and carries no way to check.
    let Some(sidecar) = read_dispatch_sidecar(&path) else {
        return DispatchRunLiveness::Unverifiable(SidecarDoubt::LegacyFormat);
    };
    if !sidecar.minted_this_boot() {
        return DispatchRunLiveness::Unverifiable(SidecarDoubt::StaleBoot);
    }
    match group_still_running_this_run(&sidecar, task_id) {
        Some(true) => DispatchRunLiveness::Alive {
            pgid: sidecar.pgid,
            supervised: sidecar.supervised_by_this_process(),
        },
        Some(false) => DispatchRunLiveness::Gone,
        None => DispatchRunLiveness::Unverifiable(SidecarDoubt::ProbeFailed),
    }
}

/// Is some live process in `sidecar.pgid` still *this* run's agent?
///
/// `None` means the process table could not be read at all — a different
/// answer from "no", and it must stay different: treating a failed probe as
/// "gone" would silently reclassify healthy runs as abandoned.
fn group_still_running_this_run(sidecar: &DispatchSidecar, task_id: &str) -> Option<bool> {
    let commands = process_group_commands(sidecar.pgid)?;
    // The run's own prompt-file stem — `dispatch-<task>-<stamp>` — appears in
    // the `sh -c "cat <prompt> | claude …"` command line the pipeline spawns,
    // and the shell stays alive as the group leader for the whole pipeline.
    // Matching on it rather than on "claude" is what makes this proof of
    // *identity* and not merely of *occupancy*: the stamp makes the string
    // unique to this run, so a recycled pgid running some other agent — even
    // another Switchbard dispatch — cannot pass.
    let fingerprint = dispatch_log_stem(task_id, sidecar.agent_started_unix);
    Some(commands.iter().any(|cmd| cmd.contains(&fingerprint)))
}

/// Command lines of every live process whose process group is `pgid`.
///
/// `ps -e -o pgid=,command=` rather than `ps -g <pgid>`: the `-g` flag means
/// process *group* on BSD/macOS but selects by session or effective group name
/// on Linux's procps, so filtering the full listing ourselves is the only
/// form that means the same thing on both platforms this app ships to.
///
/// `Some(vec![])` (a readable table with no members) is the "group is gone"
/// answer; `None` is reserved for the probe failing outright.
fn process_group_commands(pgid: i32) -> Option<Vec<String>> {
    let out = std::process::Command::new("ps")
        .args(["-e", "-o", "pgid=,command="])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_process_group_commands(
        &String::from_utf8_lossy(&out.stdout),
        pgid,
    ))
}

/// The pure half of [`process_group_commands`]: split `ps` output into
/// `(pgid, command)` and keep the commands in `pgid`'s group.
///
/// Public within the crate so the parsing rules — leading whitespace, a
/// command containing spaces, a non-numeric first column from an unexpected
/// `ps` — are unit-testable without a live process table.
pub(crate) fn parse_process_group_commands(text: &str, pgid: i32) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (group, command) = line.split_once(char::is_whitespace)?;
            (group.parse::<i32>().ok()? == pgid).then(|| command.trim().to_string())
        })
        .filter(|command| !command.is_empty())
        .collect()
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
            liveness: DispatchRunLiveness::NoSidecar,
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
    fn stalled_only_once_past_the_stale_after_threshold() {
        let run = run_with(Some(0), 0);
        let stale_after = Duration::from_secs(30 * 60);
        assert!(!run.looks_stalled(29 * 60, stale_after));
        assert!(run.looks_stalled(31 * 60, stale_after));
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
        assert_eq!(run.liveness, DispatchRunLiveness::NoSidecar);
        assert!(run.worktree_path.ends_with(".worktrees/dispatch-task-999"));
    }

    /// A run whose sidecar names a group that is genuinely still running this
    /// agent: verified alive, supervised by this process, and killable.
    ///
    /// Uses a real child in its own session, because the identity check reads
    /// the actual process table — a fabricated pgid would prove nothing about
    /// the rule it is meant to pin.
    #[test]
    fn a_live_supervised_run_is_verified_alive_and_killable() {
        let fixture = LiveRunFixture::spawn();

        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), &fixture.task_id);

        assert_eq!(
            run.liveness,
            DispatchRunLiveness::Alive {
                pgid: fixture.pgid,
                supervised: true,
            }
        );
        assert_eq!(run.liveness.killable_pgid(), Some(fixture.pgid));
        assert!(run.liveness.is_supervised());
        assert!(!run.is_abandoned(now_unix(), true));
    }

    /// F1, the blocking finding: force-quit Switchbard mid-run and the
    /// sidecar is never removed. On restart the task is still labelled
    /// `dispatching` and its log is empty, so `looks_orphaned` is false — the
    /// row used to sit under "In flight" forever with a Kill button aimed at
    /// a pgid the OS has since handed to someone else.
    ///
    /// Now: the group is positively verified gone, so no kill handle is
    /// produced and the run classifies as abandoned (which is what routes it
    /// to the attention section and the danger chip).
    #[test]
    fn a_run_whose_group_died_offers_no_kill_and_reads_as_abandoned() {
        let fixture = LiveRunFixture::spawn();
        fixture.kill_agent();

        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), &fixture.task_id);

        assert_eq!(run.liveness, DispatchRunLiveness::Gone);
        assert_eq!(run.liveness.killable_pgid(), None, "must not arm a kill");
        assert!(!run.liveness.is_supervised());
        assert_eq!(
            run.log_bytes, 0,
            "precondition: an empty log, as in the wild"
        );
        assert!(
            !run.looks_orphaned(now_unix(), true),
            "precondition: file evidence alone cannot see this death"
        );
        assert!(
            run.is_abandoned(now_unix(), true),
            "a claimed task whose group is gone needs attention, not an in-flight row"
        );
    }

    /// A released task's dead sidecar is still not a kill handle, and with no
    /// claim outstanding it is not abandoned either — abandonment is about
    /// the claim, exactly as `looks_orphaned` already had it.
    #[test]
    fn a_dead_group_on_an_unclaimed_task_is_not_abandoned() {
        let fixture = LiveRunFixture::spawn();
        fixture.kill_agent();

        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), &fixture.task_id);

        assert!(!run.is_abandoned(now_unix(), false));
    }

    /// The dangerous legacy case, pinned deliberately: a pre-versioning
    /// bare-pgid sidecar carries no boot epoch, so nothing can authenticate
    /// it. It must never arm a kill, however live the number looks.
    #[test]
    fn a_legacy_bare_pgid_sidecar_never_arms_a_kill() {
        let fixture = LiveRunFixture::spawn();
        // Same live group, written in the old format.
        std::fs::write(&fixture.pid_path, format!("{}\n", fixture.pgid)).unwrap();

        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), &fixture.task_id);

        assert_eq!(
            run.liveness,
            DispatchRunLiveness::Unverifiable(SidecarDoubt::LegacyFormat)
        );
        assert_eq!(run.liveness.killable_pgid(), None);
        assert_eq!(run.liveness.doubt(), Some(SidecarDoubt::LegacyFormat));
    }

    /// A sidecar minted under a previous boot names a pid the kernel has
    /// since reissued. Even though the group in this fixture *is* alive, the
    /// boot mismatch alone must veto the kill handle — the probe is never
    /// even reached.
    #[test]
    fn a_sidecar_from_another_boot_never_arms_a_kill() {
        let fixture = LiveRunFixture::spawn();
        fixture.rewrite_sidecar(DispatchSidecar {
            boot_epoch_unix: 1,
            ..fixture.sidecar()
        });

        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), &fixture.task_id);

        assert_eq!(
            run.liveness,
            DispatchRunLiveness::Unverifiable(SidecarDoubt::StaleBoot)
        );
        assert_eq!(run.liveness.killable_pgid(), None);
    }

    /// Occupancy is not identity. A sidecar pointing at a live group that is
    /// *not* this run — the pid-recycling case, simulated here by pointing at
    /// the test harness's own group — must not arm a kill, or Switchbard
    /// would offer to SIGKILL itself.
    #[test]
    fn a_recycled_pgid_running_something_else_never_arms_a_kill() {
        let fixture = LiveRunFixture::spawn();
        // Our own process group is unquestionably alive and unquestionably
        // not this run's agent.
        let ours = unsafe { libc::getpgrp() };
        fixture.rewrite_sidecar(DispatchSidecar {
            pgid: ours,
            ..fixture.sidecar()
        });

        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), &fixture.task_id);

        assert_eq!(
            run.liveness,
            DispatchRunLiveness::Gone,
            "a live but unrelated group is 'not this run', not a kill target"
        );
        assert_eq!(run.liveness.killable_pgid(), None);
    }

    /// A run whose supervisor is gone: the agent may well still be running,
    /// but no `wait_for_exit` is watching it, so nothing will release the
    /// task when it ends. The verdict has to say so — the view keys its
    /// unsupervised-notice treatment on exactly this bit.
    #[test]
    fn a_live_run_whose_supervisor_died_is_alive_but_unsupervised() {
        let fixture = LiveRunFixture::spawn();
        fixture.rewrite_sidecar(DispatchSidecar {
            // pid 1 is launchd/init: always alive, never this process.
            supervisor_pid: 1,
            ..fixture.sidecar()
        });

        let run = inspect_dispatch_run(Path::new("/nonexistent/repo"), &fixture.task_id);

        assert_eq!(
            run.liveness,
            DispatchRunLiveness::Alive {
                pgid: fixture.pgid,
                supervised: false,
            }
        );
        assert_eq!(
            run.liveness.killable_pgid(),
            Some(fixture.pgid),
            "an identified agent is still killable — it just releases nothing"
        );
        assert!(!run.liveness.is_supervised());
    }

    #[test]
    fn every_doubt_explains_itself_in_one_line() {
        for doubt in [
            SidecarDoubt::LegacyFormat,
            SidecarDoubt::StaleBoot,
            SidecarDoubt::ProbeFailed,
        ] {
            let text = doubt.explain();
            assert!(!text.is_empty());
            assert!(!text.contains('\n'), "must fit one UI line: {text:?}");
        }
    }

    // ── ps parsing ────────────────────────────────────────────────────────

    /// Real `ps -e -o pgid=,command=` output: right-aligned numeric column,
    /// commands containing spaces and pipes.
    #[test]
    fn process_group_commands_are_filtered_by_the_leading_pgid_column() {
        let sample = concat!(
            "    1 /sbin/launchd\n",
            "  501 /bin/sh -c cat '/tmp/switchbard-logs/dispatch-task-43-17-prompt.md' | claude -p\n",
            "  501 sleep 600\n",
            " 9999 /usr/bin/unrelated --flag\n",
        );

        let mine = parse_process_group_commands(sample, 501);

        assert_eq!(mine.len(), 2);
        assert!(mine[0].contains("dispatch-task-43-17-prompt.md"));
        assert!(mine[0].contains('|'), "the full command line is preserved");
        assert_eq!(mine[1], "sleep 600");
    }

    #[test]
    fn process_group_commands_is_empty_for_a_group_with_no_members() {
        let sample = "    1 /sbin/launchd\n  501 sleep 600\n";

        assert!(parse_process_group_commands(sample, 4242).is_empty());
    }

    /// A header row or any other non-numeric first column must be skipped
    /// rather than panicking or being misattributed to some group.
    #[test]
    fn process_group_commands_ignores_unparseable_rows() {
        let sample = " PGID COMMAND\n  501 sleep 600\nnot-a-row\n  501\n";

        let mine = parse_process_group_commands(sample, 501);

        assert_eq!(mine, vec!["sleep 600".to_string()]);
    }

    // ── fixtures ──────────────────────────────────────────────────────────

    /// A real dispatch run on disk: a live child in its own session, a log
    /// named by the pipeline's own stem convention, and a current-format
    /// sidecar. Everything is torn down on drop, including the child, so a
    /// failing assertion cannot leak a `sleep` into the developer's machine.
    struct LiveRunFixture {
        task_id: String,
        started: u64,
        pgid: i32,
        pid: u32,
        log_path: PathBuf,
        pid_path: PathBuf,
    }

    impl LiveRunFixture {
        fn spawn() -> Self {
            let task_id = unique_task_id("live");
            let started = now_unix();
            let log_dir = dispatch_log_dir();
            std::fs::create_dir_all(&log_dir).unwrap();
            let stem = dispatch_log_stem(&task_id, started);
            let log_path = log_dir.join(format!("{stem}.log"));
            let prompt_path = log_dir.join(format!("{stem}-prompt.md"));
            std::fs::write(&prompt_path, "fixture prompt").unwrap();

            // Mirrors the shape `dispatch::run_claude_headless` spawns: a
            // shell whose command line carries the prompt path, which is what
            // the identity check fingerprints. `cat … | sleep` keeps the
            // shell alive as group leader exactly like the real pipeline.
            let command = format!("cat '{}' | sleep 600", prompt_path.display());
            let run = crate::spawn::spawn_in_session(&command, &log_dir, &log_path).unwrap();
            // Give the shell a moment to exist in the process table.
            std::thread::sleep(Duration::from_millis(150));

            let fixture = Self {
                pid_path: crate::dispatch::dispatch_pid_path(&task_id, started),
                task_id,
                started,
                pgid: run.pgid,
                pid: run.pid,
                log_path,
            };
            fixture.rewrite_sidecar(fixture.sidecar());
            fixture
        }

        fn sidecar(&self) -> DispatchSidecar {
            DispatchSidecar {
                pgid: self.pgid,
                supervisor_pid: std::process::id(),
                agent_started_unix: self.started,
                boot_epoch_unix: crate::boot_time::boot_epoch_unix().unwrap_or(0),
            }
        }

        fn rewrite_sidecar(&self, sidecar: DispatchSidecar) {
            std::fs::write(
                &self.pid_path,
                format!(
                    "v={}\npgid={}\nsupervisor={}\nstarted={}\nboot={}\n",
                    crate::dispatch::SIDECAR_VERSION,
                    sidecar.pgid,
                    sidecar.supervisor_pid,
                    sidecar.agent_started_unix,
                    sidecar.boot_epoch_unix,
                ),
            )
            .unwrap();
        }

        /// Kill the agent and reap it, so the group is genuinely gone rather
        /// than a zombie — see `kill_pgid`'s doc on why an unreaped group
        /// still answers liveness probes.
        fn kill_agent(&self) {
            let pid = self.pid;
            let reaper = std::thread::spawn(move || {
                crate::spawn::wait_for_exit(pid, Duration::from_secs(5))
            });
            let _ = crate::kill::kill_pgid(self.pgid, Duration::from_secs(2));
            let _ = reaper.join();
        }
    }

    impl Drop for LiveRunFixture {
        fn drop(&mut self) {
            self.kill_agent();
            let _ = std::fs::remove_file(&self.log_path);
            let _ = std::fs::remove_file(&self.pid_path);
            let _ = std::fs::remove_file(dispatch_log_dir().join(format!(
                "{}-prompt.md",
                dispatch_log_stem(&self.task_id, self.started)
            )));
        }
    }

    /// `dispatch_log_dir()` is a real shared directory (`$TMPDIR/
    /// switchbard-logs`), also used by a running Switchbard — every test
    /// writing there needs an id no other test, run, or app can collide with.
    fn unique_task_id(tag: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        format!(
            "TASK-inspecttest-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }
}
