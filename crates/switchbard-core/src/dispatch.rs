//! Dispatch pipeline: a task explicitly flagged for dispatch → an isolated
//! git worktree → a headless `claude -p` run against that worktree → on
//! success, a pull request → the PR link appended back onto the task's
//! notes. Every mutation to a task (claiming it, recording the outcome)
//! goes through the `backlog` CLI — the repo stays the system of record,
//! this module is just the engine that drives it (unified task hub, slice 2;
//! see `docs/product-trajectory.md`).
//!
//! ## The queue
//!
//! There is no separate queue store. The queue *is* the set of tasks in a
//! repo's Backlog labeled `dispatch` — flaggable from any machine that can
//! run the `backlog` CLI, including a plain terminal with no Switchbard
//! running. [`list_dispatch_queue`] reads that straight off an already-loaded
//! [`BacklogProject`].
//!
//! The double-dispatch guard is a label swap, not a lock: [`dispatch_one`]'s
//! first move is `dispatch` → `dispatching` via [`crate::swap_backlog_label`],
//! *before* any worktree or process work starts. A queue reload never sees an
//! in-flight task as eligible again. On exit the task lands on `dispatched`
//! (success) or `dispatch-failed` (anything else) — never back on `dispatch`,
//! so a failed run doesn't silently retry-loop; a human re-flags it.
//!
//! ## The pipeline (`dispatch_one`)
//!
//! 1. Claim the task (label swap, above).
//! 2. `create_worktree` a fresh worktree at `<repo>/.worktrees/dispatch-<id>`
//!    on a new `dispatch/<id>` branch off `opts.base_branch`.
//! 3. `spawn_in_session` a headless `claude -p` fed the task's own
//!    description/AC/DoD as its prompt (see [`build_dispatch_prompt`]), and
//!    block on it via `wait_for_exit` — this is a synchronous pipeline step,
//!    not a fire-and-forget dev-server spawn, even though it reuses the same
//!    session-leader primitive.
//! 4. On a clean exit: commit any dirty worktree state (idempotent if the
//!    agent already committed), push the branch, `gh pr create`.
//! 5. Release the claim with the outcome (dispatched + PR link in the task's
//!    notes, or dispatch-failed + a note describing why).
//!
//! `claude -p` runs with `--permission-mode acceptEdits`, not
//! `--dangerously-skip-permissions`. Headless automation needs *some* way to
//! avoid hanging on a permission prompt nobody is at the keyboard to answer,
//! but `--dangerously-skip-permissions` is denied outright by Claude Code's
//! own auto-mode classifier in at least one real environment this pipeline
//! runs in (confirmed while building this module — a bare `claude -p
//! --dangerously-skip-permissions` was blocked at the classifier, while the
//! same prompt under `--permission-mode acceptEdits` wrote files and ran
//! `git commit` via Bash without hanging). `acceptEdits` auto-accepts file
//! edits and the Bash calls this pipeline's prompt asks for (add, commit,
//! test/build gate) while still being a supported, non-bypass permission
//! mode — a better fit for a worker meant to actually run than a flag that
//! may simply refuse to execute.
//!
//! ## Bounding a runaway run
//!
//! Two limits apply, and they bound different failure modes. `opts.timeout`
//! is wall-clock and external: the pipeline kills the process group once the
//! run has hung for too long. [`DispatchOptions::max_turns`] is internal and
//! passed to the agent itself as `--max-turns` — it bounds *looping*, which
//! is what a runaway agent actually does, and ends the run as an ordinary
//! exit the pipeline can report rather than a kill it can only infer.
//!
//! Neither helps a human who is watching a run go wrong right now, so
//! [`dispatch_one`] also writes a pgid sidecar next to the run's log
//! ([`dispatch_pid_path`]) between spawning the agent and blocking on it, and
//! deletes it as the run releases. A UI that reads that file can signal the
//! group directly with [`crate::kill_pgid`].
//!
//! Killing the group needs **no coordination with this module**, which is the
//! property that makes the sidecar cheap: the pipeline is already blocked in
//! `wait_for_exit` on that exact process, so the kill simply makes the wait
//! return early. The run then walks its normal exit path — non-zero exit →
//! [`DispatchResult::ClaudeFailed`] → `release_as_failed` → the task lands on
//! `dispatch-failed` with a note. One signal in; the existing label state
//! machine does all the bookkeeping.
//!
//! ## Why `drain_dispatch_queue` is serial, not parallel
//!
//! `opts.max_concurrent` caps how many tasks one drain call picks up, but it
//! processes them one at a time rather than fanning out across threads. Two
//! reasons: each task ends in a `gh pr create` call, and GitHub's abuse-rate
//! limiting is the real constraint here, not wall-clock time — spacing calls
//! out (`GH_SPACING`) and stopping the batch outright on a 403 is simpler and
//! safer to reason about than N concurrent `gh` calls tripping the same
//! limit at once. If a caller wants wall-clock concurrency later (e.g. the
//! GUI's fifth worker thread polling this on its own cadence), running
//! multiple short drain cycles back to back gets the queue-depth-limited
//! throughput without the shared-rate-limit risk.
//!
//! ## For the future GUI wiring
//!
//! [`list_dispatch_queue`], [`dispatch_one`], and [`DispatchOutcome`] are the
//! whole surface a caller needs: load a `BacklogProject`, list the queue,
//! call `dispatch_one` per task (or `drain_dispatch_queue` for the capped,
//! spaced-out version), render `DispatchOutcome`. No GUI code lives here —
//! that wiring (the Dispatch button, a background worker thread following
//! `workers.rs`'s pattern) is separate scope.

use crate::backlog::{
    append_backlog_notes, edit_backlog_task, swap_backlog_label, BacklogProject, BacklogTask,
    BacklogTaskPatch, BacklogTaskSource,
};
use crate::git_env::git_cmd;
use crate::kill::kill_pgid;
use crate::spawn::{spawn_in_session, wait_for_exit, WaitOutcome};
use crate::worktree_create::{create_worktree, CreateBranchMode, CreateWorktreeOptions};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DISPATCH_LABEL: &str = "dispatch";
pub const DISPATCHING_LABEL: &str = "dispatching";
pub const DISPATCHED_LABEL: &str = "dispatched";
pub const DISPATCH_FAILED_LABEL: &str = "dispatch-failed";

/// Status a task moves to when the pipeline claims it: an agent is actively
/// working the task in its own worktree, which is what "In Progress" means on
/// every board this app renders. The label state machine
/// (`dispatch`/`dispatching`/…) stays the authority on *pipeline* state; the
/// status exists so a task being worked by an agent is not invisible to
/// someone reading the board rather than the dispatch pill.
pub const DISPATCH_IN_PROGRESS_STATUS: &str = "In Progress";

/// Status a task moves to once the pipeline opened a PR: the agent is done and
/// a human should look. Already part of [`crate::STANDARD_STATUSES`], so this
/// introduces no new vocabulary — see that constant's doc for why the
/// standardized set is what makes this reachable in every repo.
pub const DISPATCH_REVIEW_STATUS: &str = "In Review";

/// Default cap on how many queued tasks one `drain_dispatch_queue` call picks
/// up (per the mission's "per-run concurrency cap default 2").
pub const DEFAULT_MAX_CONCURRENT: usize = 2;

/// Default [`DispatchOptions::max_turns`]. 50 is well clear of what a real
/// task-sized run takes (the runs this pipeline was built against land in the
/// low tens) while still being a hard stop on a loop that would otherwise only
/// end at the 30-minute timeout.
pub const DEFAULT_MAX_TURNS: u32 = 50;

const KILL_GRACE: Duration = Duration::from_secs(10);
const GH_SPACING: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct DispatchOptions {
    /// Branch new dispatch worktrees are created from.
    pub base_branch: String,
    /// The `claude` binary to invoke — a bare name resolves via `$PATH`.
    pub claude_binary: String,
    /// Git remote the dispatch branch is pushed to.
    pub remote: String,
    /// How long to let one `claude -p` run before killing it as stuck.
    pub timeout: Duration,
    /// See [`DEFAULT_MAX_CONCURRENT`] and the module doc's "why serial" note.
    pub max_concurrent: usize,
    /// Hard cap on agent turns for one headless run, passed straight through
    /// as `claude -p --max-turns`.
    ///
    /// A runaway agent is a *turn-count* problem before it is a wall-clock
    /// problem: the failure mode this bounds is a loop — re-reading the same
    /// file, re-running the same failing test, re-deciding the same fork —
    /// which burns tokens at full rate and produces nothing. `opts.timeout`
    /// catches that too, but only after the full 30 minutes, and only by
    /// killing the process from outside, leaving a log that ends mid-thought
    /// with no statement of why. `--max-turns` bounds it upstream, inside the
    /// agent, where the run ends as an ordinary exit the pipeline can report.
    /// The two limits are complementary, not redundant: turns bound looping,
    /// the timeout bounds hanging.
    pub max_turns: u32,
}

impl Default for DispatchOptions {
    fn default() -> Self {
        Self {
            base_branch: "main".to_string(),
            claude_binary: "claude".to_string(),
            remote: "origin".to_string(),
            timeout: Duration::from_secs(30 * 60),
            max_concurrent: DEFAULT_MAX_CONCURRENT,
            max_turns: DEFAULT_MAX_TURNS,
        }
    }
}

/// The end state of one task's dispatch pipeline. Every variant other than
/// `PrOpened` results in the task landing on `dispatch-failed` with an
/// explanatory note — see the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchResult {
    PrOpened {
        url: String,
    },
    /// `claude -p` exited cleanly but left nothing to commit or push.
    NoChanges,
    /// `None` means the run was killed for exceeding `opts.timeout`.
    ClaudeFailed {
        exit_code: Option<i32>,
    },
    GitFailed {
        stage: &'static str,
        message: String,
    },
    GhFailed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchOutcome {
    pub task_id: String,
    pub worktree_path: PathBuf,
    pub branch: String,
    pub log_path: PathBuf,
    pub result: DispatchResult,
}

/// Tasks in `project` currently eligible for dispatch: active-source tasks
/// labeled exactly `dispatch`, sorted by id for a deterministic drain order.
/// A task mid-flight (`dispatching`) or already finished (`dispatched` /
/// `dispatch-failed`) never reappears here.
pub fn list_dispatch_queue(project: &BacklogProject) -> Vec<BacklogTask> {
    let mut queue: Vec<BacklogTask> = project
        .tasks
        .iter()
        .filter(|task| task.source == BacklogTaskSource::Active)
        .filter(|task| task.labels.iter().any(|label| label == DISPATCH_LABEL))
        .cloned()
        .collect();
    queue.sort_by(|a, b| a.id.cmp(&b.id));
    queue
}

/// Cap how many queued tasks one drain cycle picks up. Pure so the cap
/// invariant is unit-testable without touching disk or the backlog CLI.
pub fn select_batch(queue: &[BacklogTask], max_concurrent: usize) -> Vec<BacklogTask> {
    let batch: Vec<BacklogTask> = queue.iter().take(max_concurrent).cloned().collect();
    debug_assert!(
        batch.len() <= max_concurrent,
        "select_batch must respect its cap"
    );
    debug_assert!(
        batch.len() <= queue.len(),
        "select_batch cannot invent tasks the queue doesn't have"
    );
    batch
}

/// Branch name for a task's dispatch worktree. Namespaced under `dispatch/`
/// so these branches are trivially greppable/prunable as a group.
pub fn dispatch_branch_name(task_id: &str) -> String {
    format!("dispatch/{}", task_id.to_ascii_lowercase())
}

/// Where a task's dispatch worktree lives. Public because it is also the
/// *only* definition of that convention — `crate::dispatch_inspect` rebuilds
/// the path to find an existing run rather than keeping a parallel copy of the
/// rule (see that module's doc on why the pipeline needs no run store).
pub fn dispatch_worktree_path(repo_root: &Path, task_id: &str) -> PathBuf {
    repo_root
        .join(".worktrees")
        .join(format!("dispatch-{}", task_id.to_ascii_lowercase()))
}

/// Directory holding every dispatch run's log and prompt file. Shared with the
/// service-log directory on purpose: one place a user has to look for "what
/// did switchbard spawn".
pub fn dispatch_log_dir() -> PathBuf {
    std::env::temp_dir().join("switchbard-logs")
}

/// Filename stem shared by a run's log (`<stem>.log`) and prompt
/// (`<stem>-prompt.md`). The embedded `unix_now()` seconds stamp is what lets
/// `dispatch_inspect` recover a run's start time — and therefore its elapsed
/// time — without the pipeline persisting anything.
pub fn dispatch_log_stem(task_id: &str, started_at_unix: u64) -> String {
    format!(
        "dispatch-{}-{}",
        task_id.to_ascii_lowercase(),
        started_at_unix
    )
}

/// Where a run's pgid sidecar lives: `<stem>.pid` alongside that run's log
/// and prompt.
///
/// The sidecar is the one thing about a run that genuinely *cannot* be rebuilt
/// from the repo root and the task id — a process group id is assigned by the
/// kernel at spawn. It is still run-scoped evidence on disk rather than a run
/// store, in exactly the sense [`crate::dispatch_inspect`]'s doc means: it is
/// named by the same stem convention as the log, it is written by the run and
/// deleted when that run releases, and nothing reads it as authority on
/// pipeline state (the task's label remains the state machine). It exists so a
/// human watching a run has a hand on the plug.
pub fn dispatch_pid_path(task_id: &str, started_at_unix: u64) -> PathBuf {
    dispatch_log_dir().join(format!(
        "{}.pid",
        dispatch_log_stem(task_id, started_at_unix)
    ))
}

/// Parse a pgid out of a sidecar written by [`write_pid_sidecar`]. `None` for
/// a missing, empty, or unparseable file — a caller that cannot read a pgid
/// simply has no kill affordance to offer, which is the position every caller
/// was in before sidecars existed. Non-positive values are rejected because
/// `kill_pgid` negates its argument to address a group: a 0 or negative pgid
/// would widen the signal past the one run it is meant for.
pub fn read_pid_sidecar(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|pgid| *pgid > 0)
}

/// Record the run's process group so a UI can signal it. Best-effort by
/// design: failing to write the sidecar costs the user a Kill button, which
/// must never be worth aborting a run that is otherwise fine.
fn write_pid_sidecar(path: &Path, pgid: i32) {
    let _ = std::fs::write(path, format!("{pgid}\n"));
}

/// Drop the kill handle. Called as the run releases its claim — see
/// [`dispatch_one`] for why that is the right boundary.
fn remove_pid_sidecar(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// The `/bin/sh -c` line for one headless run. Pure so the flag set — in
/// particular `--max-turns`, the only bound that acts *inside* the agent — is
/// unit-testable without spawning a process.
fn build_claude_command(prompt_path: &Path, opts: &DispatchOptions) -> String {
    format!(
        "cat {} | {} -p --permission-mode acceptEdits --max-turns {} --output-format text",
        shell_quote(prompt_path),
        opts.claude_binary,
        opts.max_turns,
    )
}

/// The prompt handed to the headless `claude -p` run: the task's own
/// description/AC/DoD verbatim, plus the operating contract (implement and
/// commit; don't push or open a PR — the pipeline owns that). Pure so wording
/// changes are unit-testable without spawning a process.
pub fn build_dispatch_prompt(task: &BacklogTask) -> String {
    let mut prompt = format!(
        "You are implementing Backlog task {} — \"{}\" — in this git worktree.\n\n",
        task.id, task.title
    );
    if !task.description.trim().is_empty() {
        prompt.push_str("## Description\n\n");
        prompt.push_str(task.description.trim());
        prompt.push_str("\n\n");
    }
    push_checklist_section(
        &mut prompt,
        "Acceptance Criteria",
        &task.acceptance_criteria,
    );
    push_checklist_section(&mut prompt, "Definition of Done", &task.definition_of_done);
    prompt.push_str(
        "## Operating contract\n\n\
         - Implement the task fully in this worktree.\n\
         - Run this repo's test/build gate before finishing, if one exists.\n\
         - Commit your changes with `git add -A && git commit` and a descriptive \
           message; do not push and do not open a pull request yourself — the \
           dispatch pipeline handles push and PR creation after you exit.\n\
         - If you cannot complete the task, leave the worktree uncommitted \
           rather than committing partial or broken work.\n",
    );
    prompt
}

fn push_checklist_section(
    prompt: &mut String,
    heading: &str,
    items: &[crate::backlog::BacklogChecklistItem],
) {
    if items.is_empty() {
        return;
    }
    prompt.push_str(&format!("## {heading}\n\n"));
    for item in items {
        prompt.push_str(&format!("- {}\n", item.text));
    }
    prompt.push('\n');
}

/// Run the full pipeline for one task: claim → worktree → headless `claude
/// -p` → commit/push → `gh pr create` → notes. Blocking; safe to call from a
/// background worker thread the same way `switchbard-core`'s other probes
/// are called from the GUI's `workers.rs`.
///
/// Returns `Err` only for setup failures the queue can't recover from (claim
/// failed, worktree/prompt-file setup failed) — the task's label is left
/// untouched in that case so it's retried on the next queue load. Everything
/// downstream of a successful claim reports through `DispatchOutcome::result`
/// instead, so a caller draining a whole batch can keep going after one
/// task's pipeline fails partway through.
pub fn dispatch_one(
    repo_root: &Path,
    task: &BacklogTask,
    opts: &DispatchOptions,
) -> Result<DispatchOutcome> {
    swap_backlog_label(repo_root, &task.id, DISPATCH_LABEL, DISPATCHING_LABEL)
        .with_context(|| format!("failed to claim {} for dispatch", task.id))?;

    // Captured *before* the pipeline moves the task, so a failure can put it
    // back where the user left it instead of stranding it on "In Progress"
    // with nothing actually progressing.
    let prior_status = task.status.clone();
    set_dispatch_status(repo_root, &task.id, DISPATCH_IN_PROGRESS_STATUS);

    let paths = match prepare_dispatch(repo_root, task, opts) {
        Ok(paths) => paths,
        Err(e) => {
            release_as_failed(repo_root, &task.id, &e.to_string(), &prior_status);
            return Err(e);
        }
    };

    let exit = run_claude_headless(&paths, opts);
    // The agent is gone either way by the time that returns — whether it
    // exited on its own, hit `opts.timeout`, or was killed from the Dispatch
    // view. Drop the kill handle here, at the release boundary, so a sidecar
    // on disk never names a process group that is not this run's live one.
    // (The remaining commit/push/PR tail is Switchbard's own work, not the
    // agent's; there is deliberately nothing to signal for it.)
    remove_pid_sidecar(&paths.pid_path);
    let exit = match exit {
        Ok(exit) => exit,
        Err(e) => {
            release_as_failed(repo_root, &task.id, &e.to_string(), &prior_status);
            return Err(e);
        }
    };

    let result = match exit {
        Some(0) => finish_pipeline(task, &paths.worktree_path, &paths.branch, opts),
        other => DispatchResult::ClaudeFailed { exit_code: other },
    };

    match &result {
        DispatchResult::PrOpened { url } => {
            let _ = release_as_dispatched(repo_root, &task.id, url);
        }
        other => release_as_failed(repo_root, &task.id, &describe_result(other), &prior_status),
    }

    Ok(DispatchOutcome {
        task_id: task.id.clone(),
        worktree_path: paths.worktree_path,
        branch: paths.branch,
        log_path: paths.log_path,
        result,
    })
}

/// Drain up to `opts.max_concurrent` queued tasks, one at a time — see the
/// module doc's "why serial" note. Stops early if a `gh` call comes back rate
/// limited (HTTP 403) so the rest of the batch doesn't hammer an
/// already-throttled token.
pub fn drain_dispatch_queue(
    repo_root: &Path,
    project: &BacklogProject,
    opts: &DispatchOptions,
) -> Vec<DispatchOutcome> {
    let batch = select_batch(&list_dispatch_queue(project), opts.max_concurrent);
    let mut outcomes = Vec::with_capacity(batch.len());
    for task in &batch {
        let Ok(outcome) = dispatch_one(repo_root, task, opts) else {
            // Setup failure: nothing recorded on the task (see dispatch_one's
            // doc), so it stays queued for the next drain. Move on.
            continue;
        };
        let rate_limited = matches!(&outcome.result, DispatchResult::GhFailed { message } if message.contains("403"));
        outcomes.push(outcome);
        if rate_limited {
            break;
        }
        std::thread::sleep(GH_SPACING);
    }
    outcomes
}

struct DispatchPaths {
    worktree_path: PathBuf,
    branch: String,
    log_path: PathBuf,
    prompt_path: PathBuf,
    pid_path: PathBuf,
}

fn prepare_dispatch(
    repo_root: &Path,
    task: &BacklogTask,
    opts: &DispatchOptions,
) -> Result<DispatchPaths> {
    let branch = dispatch_branch_name(&task.id);
    let worktree_path = dispatch_worktree_path(repo_root, &task.id);
    create_worktree(CreateWorktreeOptions {
        repo_path: repo_root.to_path_buf(),
        worktree_path: worktree_path.clone(),
        branch_mode: CreateBranchMode::NewBranch {
            branch: branch.clone(),
            base: opts.base_branch.clone(),
        },
    })
    .context("dispatch worktree create failed")?;

    let log_dir = dispatch_log_dir();
    std::fs::create_dir_all(&log_dir).context("failed to create switchbard-logs dir")?;
    let stem = dispatch_log_stem(&task.id, unix_now());
    let log_path = log_dir.join(format!("{stem}.log"));
    let prompt_path = log_dir.join(format!("{stem}-prompt.md"));
    let pid_path = log_dir.join(format!("{stem}.pid"));
    std::fs::write(&prompt_path, build_dispatch_prompt(task))
        .context("failed writing dispatch prompt")?;

    debug_assert!(
        worktree_path.starts_with(repo_root),
        "dispatch worktrees must live under the repo"
    );
    Ok(DispatchPaths {
        worktree_path,
        branch,
        log_path,
        prompt_path,
        pid_path,
    })
}

/// Spawn the headless `claude -p` run and block until it exits or
/// `opts.timeout` elapses. `Ok(None)` means it was killed for running too
/// long; `Ok(Some(code))` is its real exit code.
fn run_claude_headless(paths: &DispatchPaths, opts: &DispatchOptions) -> Result<Option<i32>> {
    let command = build_claude_command(&paths.prompt_path, opts);
    let run = spawn_in_session(&command, &paths.worktree_path, &paths.log_path)
        .context("failed to spawn claude")?;
    // Written *after* the spawn (the kernel assigns the group) and before the
    // blocking wait, so the sidecar exists for essentially the whole window in
    // which there is something to kill.
    write_pid_sidecar(&paths.pid_path, run.pgid);
    match wait_for_exit(run.pid, opts.timeout).context("failed waiting on claude")? {
        WaitOutcome::Exited(code) => Ok(Some(code)),
        WaitOutcome::TimedOut => {
            let _ = kill_pgid(run.pgid, KILL_GRACE);
            let _ = wait_for_exit(run.pid, KILL_GRACE);
            Ok(None)
        }
    }
}

fn finish_pipeline(
    task: &BacklogTask,
    worktree_path: &Path,
    branch: &str,
    opts: &DispatchOptions,
) -> DispatchResult {
    if let Err(message) = commit_if_dirty(worktree_path, task) {
        return DispatchResult::GitFailed {
            stage: "commit",
            message,
        };
    }
    match commits_ahead(worktree_path, &opts.base_branch) {
        Ok(0) => return DispatchResult::NoChanges,
        Ok(_) => {}
        Err(message) => {
            return DispatchResult::GitFailed {
                stage: "rev-list",
                message,
            }
        }
    }
    if let Err(message) = push_branch(worktree_path, &opts.remote, branch) {
        return DispatchResult::GitFailed {
            stage: "push",
            message,
        };
    }
    match open_pull_request(worktree_path, task, branch, &opts.base_branch) {
        Ok(url) => DispatchResult::PrOpened { url },
        Err(message) => DispatchResult::GhFailed { message },
    }
}

fn commit_if_dirty(worktree_path: &Path, task: &BacklogTask) -> std::result::Result<(), String> {
    let status = run_git_capture(worktree_path, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Ok(());
    }
    run_git_capture(worktree_path, &["add", "-A"])?;
    let message = format!("{}: {}", task.id, task.title);
    run_git_capture(worktree_path, &["commit", "-m", &message])?;
    Ok(())
}

fn commits_ahead(worktree_path: &Path, base_branch: &str) -> std::result::Result<u32, String> {
    let range = format!("{base_branch}..HEAD");
    let out = run_git_capture(worktree_path, &["rev-list", "--count", &range])?;
    out.trim()
        .parse::<u32>()
        .map_err(|e| format!("unparseable rev-list count {out:?}: {e}"))
}

fn push_branch(
    worktree_path: &Path,
    remote: &str,
    branch: &str,
) -> std::result::Result<(), String> {
    run_git_capture(worktree_path, &["push", "-u", remote, branch]).map(|_| ())
}

fn run_git_capture(worktree_path: &Path, args: &[&str]) -> std::result::Result<String, String> {
    let output = git_cmd()
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git {args:?}: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn open_pull_request(
    worktree_path: &Path,
    task: &BacklogTask,
    branch: &str,
    base_branch: &str,
) -> std::result::Result<String, String> {
    let title = format!("{}: {}", task.id, task.title);
    let body = format!(
        "Automated dispatch run for {}.\n\n{}",
        task.id, task.description
    );
    let output = Command::new("gh")
        .current_dir(worktree_path)
        .args([
            "pr",
            "create",
            "--base",
            base_branch,
            "--head",
            branch,
            "--title",
            &title,
            "--body",
            &body,
        ])
        .output()
        .map_err(|e| format!("failed to run gh pr create: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Move a task's status as part of the dispatch lifecycle.
///
/// Best-effort and deliberately non-fatal: the label state machine, not the
/// status, is what guards against double dispatch, so a failed status write
/// must never abort a run or leave the task un-released. The visible cost of
/// a failure here is a task whose board column lags its dispatch pill — bad,
/// but strictly better than a half-claimed task.
fn set_dispatch_status(repo_root: &Path, task_id: &str, status: &str) {
    let patch = BacklogTaskPatch {
        status: Some(status.to_string()),
        ..Default::default()
    };
    let _ = edit_backlog_task(repo_root, task_id, &patch);
}

/// `prior_status` is the status the task carried before [`dispatch_one`]
/// claimed it; the pipeline moved it to `In Progress`, so the pipeline puts it
/// back. Restoring rather than picking a fixed "failed" status keeps this a
/// true inverse — a task that was in `Icebox` when someone flagged it returns
/// to `Icebox`, not to whatever this module considers a sensible default.
fn release_as_failed(repo_root: &Path, task_id: &str, reason: &str, prior_status: &str) {
    // Best-effort: if the label swap itself fails there is nothing more we
    // can do here beyond leaving the task on `dispatching` for a human to
    // notice. Never panic a background worker over a bookkeeping write.
    let _ = swap_backlog_label(repo_root, task_id, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL);
    set_dispatch_status(repo_root, task_id, prior_status);
    let _ = append_backlog_notes(repo_root, task_id, &format!("Dispatch failed: {reason}"));
}

fn release_as_dispatched(repo_root: &Path, task_id: &str, pr_url: &str) -> Result<()> {
    swap_backlog_label(repo_root, task_id, DISPATCHING_LABEL, DISPATCHED_LABEL)?;
    set_dispatch_status(repo_root, task_id, DISPATCH_REVIEW_STATUS);
    append_backlog_notes(repo_root, task_id, &format!("Dispatch PR: {pr_url}"))?;
    Ok(())
}

fn describe_result(result: &DispatchResult) -> String {
    match result {
        DispatchResult::PrOpened { url } => format!("PR opened: {url}"),
        DispatchResult::NoChanges => "claude made no changes".to_string(),
        DispatchResult::ClaudeFailed { exit_code } => format!("claude exited with {exit_code:?}"),
        DispatchResult::GitFailed { stage, message } => format!("git {stage} failed: {message}"),
        DispatchResult::GhFailed { message } => format!("gh pr create failed: {message}"),
    }
}

/// Quote a path for interpolation into a `/bin/sh -c` string (the one place
/// this module builds a shell command instead of passing argv directly, to
/// pipe the prompt file into `claude -p`'s stdin).
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::{load_backlog_project, BacklogChecklistItem};
    use std::fs;

    fn task(id: &str, labels: &[&str]) -> BacklogTask {
        BacklogTask {
            id: id.to_string(),
            title: "Example".to_string(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: labels.iter().map(|l| l.to_string()).collect(),
            dependencies: vec![],
            references: vec![],
            milestone: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: "Do the thing.".to_string(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from(format!("/repo/backlog/tasks/{id}.md")),
        }
    }

    #[test]
    fn list_dispatch_queue_only_returns_dispatch_labeled_active_tasks() {
        let project = BacklogProject {
            root: PathBuf::from("/repo"),
            cli_path: None,
            tasks: vec![
                task("TASK-2", &["dispatch"]),
                task("TASK-1", &["dispatch", "hub"]),
                task("TASK-3", &["dispatching"]),
                task("TASK-4", &[]),
            ],
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        };

        let queue = list_dispatch_queue(&project);
        let ids: Vec<&str> = queue.iter().map(|t| t.id.as_str()).collect();

        assert_eq!(
            ids,
            vec!["TASK-1", "TASK-2"],
            "sorted, dispatch-only, dispatching excluded"
        );
    }

    #[test]
    fn list_dispatch_queue_ignores_non_active_sources() {
        let mut done = task("TASK-9", &["dispatch"]);
        done.source = BacklogTaskSource::Completed;
        let project = BacklogProject {
            root: PathBuf::from("/repo"),
            cli_path: None,
            tasks: vec![done],
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![],
        };

        assert!(list_dispatch_queue(&project).is_empty());
    }

    #[test]
    fn list_dispatch_queue_filters_real_fixture_files_on_disk() {
        // Exercises the module against real markdown parsed by
        // `load_backlog_project` (not hand-built structs), per the mission's
        // "unit-test queue/guard logic against fixture repos under $TMPDIR".
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog/tasks")).unwrap();
        write_fixture_task(dir.path(), "TASK-1", &["dispatch"]);
        write_fixture_task(dir.path(), "TASK-2", &["dispatching"]);
        write_fixture_task(dir.path(), "TASK-3", &[]);
        write_fixture_task(dir.path(), "TASK-4", &["dispatch", "hub"]);

        let project = load_backlog_project(dir.path()).unwrap();
        let ids: Vec<String> = list_dispatch_queue(&project)
            .into_iter()
            .map(|t| t.id)
            .collect();

        assert_eq!(ids, vec!["TASK-1".to_string(), "TASK-4".to_string()]);
    }

    fn write_fixture_task(root: &Path, id: &str, labels: &[&str]) {
        let labels_line = if labels.is_empty() {
            "labels: []".to_string()
        } else {
            let items: Vec<String> = labels.iter().map(|l| format!("  - {l}")).collect();
            format!("labels:\n{}", items.join("\n"))
        };
        let text = format!(
            "---\nid: {id}\ntitle: Fixture task\nstatus: To Do\n{labels_line}\n---\n\n## Description\n\nFixture.\n"
        );
        fs::write(root.join("backlog/tasks").join(format!("{id}.md")), text).unwrap();
    }

    #[test]
    fn select_batch_caps_at_max_concurrent() {
        let queue = vec![
            task("TASK-1", &[]),
            task("TASK-2", &[]),
            task("TASK-3", &[]),
        ];

        let batch = select_batch(&queue, 2);

        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].id, "TASK-1");
        assert_eq!(batch[1].id, "TASK-2");
    }

    #[test]
    fn select_batch_never_invents_tasks_past_the_queue_length() {
        let queue = vec![task("TASK-1", &[])];

        let batch = select_batch(&queue, 5);

        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn dispatch_branch_name_is_namespaced_and_lowercased() {
        assert_eq!(dispatch_branch_name("TASK-11"), "dispatch/task-11");
    }

    #[test]
    fn build_dispatch_prompt_includes_task_content_and_the_operating_contract() {
        let mut t = task("TASK-11", &["dispatch"]);
        t.acceptance_criteria = vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "It works".to_string(),
        }];
        t.definition_of_done = vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "CI is green".to_string(),
        }];

        let prompt = build_dispatch_prompt(&t);

        assert!(prompt.contains("TASK-11"));
        assert!(prompt.contains("Do the thing."));
        assert!(prompt.contains("It works"));
        assert!(prompt.contains("CI is green"));
        assert!(prompt.contains("do not push"));
    }

    #[test]
    fn build_dispatch_prompt_omits_empty_checklist_sections() {
        let t = task("TASK-11", &[]);

        let prompt = build_dispatch_prompt(&t);

        assert!(!prompt.contains("Acceptance Criteria"));
        assert!(!prompt.contains("Definition of Done"));
    }

    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        let path = PathBuf::from("/tmp/it's-a-test/prompt.md");

        let quoted = shell_quote(&path);

        assert_eq!(quoted, "'/tmp/it'\\''s-a-test/prompt.md'");
    }

    #[test]
    fn claude_command_carries_the_turn_cap_from_options() {
        let opts = DispatchOptions {
            max_turns: 7,
            ..Default::default()
        };

        let command = build_claude_command(Path::new("/tmp/prompt.md"), &opts);

        assert!(
            command.contains("--max-turns 7"),
            "turn cap must reach the agent: {command}"
        );
        // The permission-mode rationale in this module's doc is load-bearing;
        // assert the flag survives any future edit to the command line.
        assert!(command.contains("--permission-mode acceptEdits"));
        assert!(command.contains("'/tmp/prompt.md'"));
    }

    #[test]
    fn the_default_turn_cap_is_the_documented_one() {
        let command = build_claude_command(Path::new("/tmp/p.md"), &DispatchOptions::default());

        assert_eq!(DispatchOptions::default().max_turns, DEFAULT_MAX_TURNS);
        assert!(command.contains(&format!("--max-turns {DEFAULT_MAX_TURNS}")));
    }

    #[test]
    fn pid_sidecar_sits_beside_the_run_it_names() {
        let pid = dispatch_pid_path("TASK-11", 1_700_000_000);
        let log = dispatch_log_dir().join(format!(
            "{}.log",
            dispatch_log_stem("TASK-11", 1_700_000_000)
        ));

        assert_eq!(pid.parent(), log.parent());
        assert_eq!(
            pid.file_name().unwrap().to_str().unwrap(),
            "dispatch-task-11-1700000000.pid"
        );
    }

    /// The whole sidecar lifecycle in one place: written at spawn, readable
    /// as a pgid while the run is live, gone once the run releases.
    #[test]
    fn pid_sidecar_round_trips_and_is_removed_on_release() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dispatch-task-11-1700000000.pid");

        assert_eq!(read_pid_sidecar(&path), None, "nothing written yet");

        write_pid_sidecar(&path, 4242);
        assert!(path.exists());
        assert_eq!(read_pid_sidecar(&path), Some(4242));

        remove_pid_sidecar(&path);
        assert!(!path.exists());
        assert_eq!(read_pid_sidecar(&path), None);
    }

    /// Removing a sidecar that is already gone is the normal case on any
    /// early-exit path — it must be silent, not an error a caller has to
    /// thread through the release bookkeeping.
    #[test]
    fn removing_a_missing_pid_sidecar_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();

        remove_pid_sidecar(&dir.path().join("never-written.pid"));
    }

    /// A truncated or hand-mangled sidecar must not hand a caller a pgid it
    /// would then signal. Non-positive values are rejected specifically
    /// because `kill_pgid` negates its argument to address the group.
    #[test]
    fn a_malformed_pid_sidecar_yields_no_pgid() {
        let dir = tempfile::tempdir().unwrap();
        for (name, contents) in [
            ("empty.pid", ""),
            ("blank.pid", "   \n"),
            ("words.pid", "not-a-pid\n"),
            ("zero.pid", "0\n"),
            ("negative.pid", "-1\n"),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, contents).unwrap();
            assert_eq!(read_pid_sidecar(&path), None, "{name} must not parse");
        }
    }

    /// Trailing whitespace/newline is what `write_pid_sidecar` itself emits,
    /// so the reader has to tolerate it — a regression here would silently
    /// disable every Kill button.
    #[test]
    fn pid_sidecar_reader_tolerates_the_writers_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nl.pid");
        fs::write(&path, "  91234  \n").unwrap();

        assert_eq!(read_pid_sidecar(&path), Some(91234));
    }

    /// The Kill button's entire mechanism, against a real process group:
    /// spawn a run the way the pipeline does, record its pgid in a sidecar,
    /// then — reading *only* the sidecar, exactly as the UI does — signal the
    /// group and watch the pipeline's own blocked `wait_for_exit` return.
    ///
    /// This is the claim that makes the feature need no coordination with the
    /// worker thread: the kill is one signal, and the wait coming back early
    /// is what routes the run into `ClaudeFailed` → `release_as_failed`. If
    /// this ever regresses, a killed run would sit out the full timeout with
    /// its task still claimed.
    ///
    /// `#[ignore]` for the reason `spawn::tests::spawn_then_kill_works`
    /// already documents and carries the same attribute for: signalling a
    /// freshly-`setsid`'d group from under the macOS test harness returns
    /// EPERM, while the same code path works against real long-lived groups
    /// in the running app. Re-confirmed while writing this (2026-08-19):
    /// SIGTERM is accepted and does nothing, the SIGKILL escalation comes
    /// back `Operation not permitted`, with and without the shell `exec` +
    /// settle delay that test uses. Run it deliberately —
    /// `cargo test -p switchbard-core -- --ignored killing_a_runs` — when
    /// changing the kill path; the pure sidecar round-trip above is what
    /// guards the format on every CI run.
    #[test]
    #[ignore]
    fn killing_a_runs_process_group_via_its_sidecar_unblocks_the_pipelines_wait() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("run.log");
        let pid_path = dir.path().join("run.pid");

        // `sleep 600` stands in for the headless agent: it outlives any
        // plausible test runtime, so the only way this test finishes is if
        // the kill actually lands.
        let run = spawn_in_session("exec sleep 600", dir.path(), &log_path).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        write_pid_sidecar(&pid_path, run.pgid);

        let from_sidecar = read_pid_sidecar(&pid_path).expect("sidecar must name the group");
        assert_eq!(from_sidecar, run.pgid);

        let outcome = kill_pgid(from_sidecar, Duration::from_secs(2)).unwrap();
        assert!(
            matches!(outcome, crate::kill::KillOutcome::Terminated),
            "SIGTERM should be enough for the group: {outcome:?}"
        );

        let waited = wait_for_exit(run.pid, Duration::from_secs(10)).unwrap();
        assert!(
            matches!(waited, WaitOutcome::Exited(_)),
            "the pipeline's blocked wait must return on the kill, not time out: {waited:?}"
        );

        remove_pid_sidecar(&pid_path);
        assert_eq!(read_pid_sidecar(&pid_path), None);
    }

    #[test]
    fn describe_result_is_human_readable_for_every_failure_variant() {
        assert!(describe_result(&DispatchResult::NoChanges).contains("no changes"));
        assert!(
            describe_result(&DispatchResult::ClaudeFailed { exit_code: Some(1) }).contains('1')
        );
        assert!(describe_result(&DispatchResult::GitFailed {
            stage: "push",
            message: "boom".to_string()
        })
        .contains("push"));
        assert!(describe_result(&DispatchResult::GhFailed {
            message: "403".to_string()
        })
        .contains("403"));
    }
}
