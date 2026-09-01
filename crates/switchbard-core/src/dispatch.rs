//! Dispatch pipeline: a task explicitly flagged for dispatch → an isolated
//! git worktree → a headless `claude -p` run against that worktree → on
//! success, a pull request → the PR link appended back onto the task's
//! notes. Every mutation to a task (claiming it, recording the outcome)
//! goes through `crate::backlog`'s native write layer — the repo stays the
//! system of record, this module is just the engine that drives it (unified
//! task hub, slice 2, as amended by the *Backlog format fork* entry; see
//! `docs/product-trajectory.md`).
//!
//! ## The queue
//!
//! There is no separate queue store. The queue *is* the set of tasks in a
//! repo's Backlog labeled `dispatch` — flaggable from a plain terminal with
//! no Switchbard running (via anything that can set a task label; the
//! format fork's `switchbard task` CLI is the planned first-class way).
//! [`list_dispatch_queue`] reads that straight off an already-loaded
//! [`BacklogRepo`].
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
//! LED-580 post-mortem (2026-08-20): a genuinely productive 30-minute run —
//! 29 files of real work — was hard-killed the instant it crossed
//! `opts.timeout` (as it was then), stranding everything it had not yet
//! committed. Owner decision: drop the strict wall clock entirely. There is
//! no code path in this module (or anywhere else) that kills a run for
//! taking too long. [`run_claude_headless`]'s wait on the agent is, for all
//! practical purposes, unbounded — see [`NO_WALL_CLOCK_LIMIT`].
//!
//! What still bounds a runaway agent, now that the timeout doesn't:
//!
//! - [`DispatchOptions::max_turns`], passed to the agent itself as
//!   `--max-turns`. A runaway agent is a *turn-count* problem before it is a
//!   wall-clock one — re-reading the same file, re-running the same failing
//!   test, re-deciding the same fork — and this bounds it upstream, inside
//!   the agent, ending the run as an ordinary exit the pipeline can report
//!   rather than a kill it can only infer.
//! - The identity-gated Kill button, for a human watching a run go wrong
//!   *right now*. Manual, not automatic — see below.
//! - [`DispatchOptions::stale_after`], which is **not a bound at all**. It is
//!   an advisory threshold: a run past it keeps running exactly as before,
//!   just flagged needs-attention in the top-bar chip and the Dispatch view
//!   (`crate::dispatch_inspect::DispatchRun::looks_stalled`) so a human knows
//!   to go check on it. `stale_after` carries the same 30-minute default the
//!   removed wall-clock kill used — that number was never wrong as a "go
//!   look" signal, only as a kill trigger.
//!
//! [`dispatch_one`] still writes a [`DispatchSidecar`] next to the run's log
//! ([`dispatch_pid_path`]) between spawning the agent and blocking on it, and
//! deletes it as the run releases. That sidecar is what makes the Kill button
//! possible: a UI that reads it can signal the group directly with
//! [`crate::kill_pgid`].
//!
//! That file is a loaded gun, so it is **self-authenticating**. A pid is only
//! meaningful within one boot and only supervised by the process that spawned
//! it, so the sidecar records the boot epoch and the supervisor's pid
//! alongside the pgid, and a reader that cannot match both refuses to hand
//! out a kill handle at all. The failure this prevents is concrete: force-quit
//! Switchbard mid-run, restart, and a bare pgid on disk would arm a Kill
//! button aimed at whatever process group has since inherited that number.
//! Every ambiguity — missing file, unknown version, legacy bare-pgid format,
//! different boot — collapses to the same answer, *unverifiable*, and the
//! same obligation: offer nothing. See
//! [`crate::dispatch_inspect::DispatchRunLiveness`] for the verdict a UI
//! actually consumes.
//!
//! Killing the group needs **no coordination with this module**, which is the
//! property that makes the sidecar cheap: the pipeline is already blocked in
//! `wait_for_exit` on that exact process, so the kill simply makes the wait
//! return early. The run then walks its normal exit path — non-zero (or
//! signal) exit → [`DispatchResult::ClaudeFailed`] → `release_as_failed` →
//! the task lands on `dispatch-failed` with a note. One signal in; the
//! existing label state machine does all the bookkeeping.
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
//! Consequence, accepted as of TASK-46: since a run's own wait is no longer
//! wall-clock bounded (see "Bounding a runaway run" above), a single long
//! run also delays every other task queued behind it in the same drain call
//! — there is no wall clock left to cap how long that delay can be. The
//! caller that actually processes a live queue serially, one drain call at a
//! time on its own worker thread, is the GUI's `workers::spawn_dispatch`;
//! see that function's doc for the same tradeoff at that layer.
//!
//! ## For the future GUI wiring
//!
//! [`list_dispatch_queue`], [`dispatch_one`], and [`DispatchOutcome`] are the
//! whole surface a caller needs: load a `BacklogRepo`, list the queue,
//! call `dispatch_one` per task (or `drain_dispatch_queue` for the capped,
//! spaced-out version), render `DispatchOutcome`. No GUI code lives here —
//! that wiring (the Dispatch button, a background worker thread following
//! `workers.rs`'s pattern) is separate scope.

use crate::backlog::{
    append_backlog_notes, edit_backlog_task, set_backlog_label, swap_backlog_label, BacklogRepo,
    BacklogTask, BacklogTaskPatch, BacklogTaskSource,
};
use crate::git_env::git_cmd;
// Only exercised by this module's own tests (the sidecar-kill mechanism
// proof) — TASK-46 removed this module's production use of a kill. Gated
// so a non-test build doesn't warn on an import nothing outside `mod tests`
// reaches.
#[cfg(test)]
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
/// low tens) while still being a hard stop on a loop that would otherwise
/// have no automatic bound at all now that there is no wall-clock kill (see
/// the module doc's "Bounding a runaway run" section).
pub const DEFAULT_MAX_TURNS: u32 = 50;

/// Default [`DispatchOptions::stale_after`]. The same 30-minute number
/// `opts.timeout` used to carry before TASK-46 (LED-580) removed the
/// wall-clock kill it armed — that threshold was never wrong as a "go check
/// on this" signal, only as a kill trigger, so it survives unchanged even
/// though its consequence doesn't.
pub const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(30 * 60);

/// Grace period between SIGTERM and SIGKILL for a **manual** kill. TASK-46
/// removed this module's own use of a kill — there is no more wall-clock
/// kill to grace — so the only production caller reaching for this shape of
/// grace period today is the identity-gated Kill button
/// (`crate::dispatch_kill::kill_dispatch_run`), which takes its own grace
/// duration from its caller rather than importing this constant. What still
/// exercises this exact value is `killing_a_runs_process_group_via_its_
/// sidecar_unblocks_the_pipelines_wait` below — the same sidecar-signalling
/// mechanism a manual kill uses, proven against a real process group.
#[cfg(test)]
const KILL_GRACE: Duration = Duration::from_secs(10);
const GH_SPACING: Duration = Duration::from_secs(2);

/// `wait_for_exit` requires a finite deadline, but TASK-46 removed the
/// wall-clock kill this pipeline used to enforce at that deadline — so this
/// exists purely to satisfy the function's signature, not as a real limit.
/// No dispatch run should ever plausibly reach it; picking a merely *very
/// large* duration rather than `Duration::MAX` matters because
/// `wait_for_exit` adds it to `Instant::now()`, and `Duration::MAX` would
/// overflow that addition and panic.
const NO_WALL_CLOCK_LIMIT: Duration = Duration::from_secs(60 * 60 * 24 * 365 * 100);

#[derive(Debug, Clone)]
pub struct DispatchOptions {
    /// Branch new dispatch worktrees are created from.
    pub base_branch: String,
    /// The `claude` binary to invoke — a bare name resolves via `$PATH`.
    pub claude_binary: String,
    /// Git remote the dispatch branch is pushed to.
    pub remote: String,
    /// How long a run can go before it is flagged as needing attention.
    /// **Advisory only** — see [`DEFAULT_STALE_AFTER`] and the module doc's
    /// "Bounding a runaway run" section. TASK-46 (LED-580, 2026-08-20)
    /// removed the wall-clock kill this field used to arm: a genuinely
    /// productive 30-minute run doing legitimate work across 29 files was
    /// hard-killed at this exact threshold and stranded its uncommitted
    /// work. Crossing `stale_after` today changes nothing about the run
    /// itself; `crate::dispatch_inspect::DispatchRun::looks_stalled` just
    /// flips the chip and the Dispatch view to needs-attention so a human
    /// knows to go look, while the run keeps going.
    pub stale_after: Duration,
    /// See [`DEFAULT_MAX_CONCURRENT`] and the module doc's "why serial" note.
    pub max_concurrent: usize,
    /// Hard cap on agent turns for one headless run, passed straight through
    /// as `claude -p --max-turns`.
    ///
    /// A runaway agent is a *turn-count* problem before it is a wall-clock
    /// one: the failure mode this bounds is a loop — re-reading the same
    /// file, re-running the same failing test, re-deciding the same fork —
    /// which burns tokens at full rate and produces nothing. Since TASK-46
    /// removed the wall-clock kill, this is the *only* automatic bound left
    /// on a runaway run — everything else past this point is either advisory
    /// (`stale_after`) or requires a human to click Kill. `--max-turns` ends
    /// the run as an ordinary exit the pipeline can report, from inside the
    /// agent, rather than a kill inferred from outside.
    pub max_turns: u32,
}

impl Default for DispatchOptions {
    fn default() -> Self {
        Self {
            base_branch: "main".to_string(),
            claude_binary: "claude".to_string(),
            remote: "origin".to_string(),
            stale_after: DEFAULT_STALE_AFTER,
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
    /// `claude -p` exited non-zero, or the run was killed from the Dispatch
    /// view (`crate::dispatch_kill::kill_dispatch_run`) — a signal death
    /// decodes to `-1` (see `spawn::decode_exit_status`). Both surface here
    /// identically: either way the run didn't produce a PR, and this module
    /// has no way (and no need) to tell a deliberate kill apart from an
    /// ordinary failure once it's over.
    ClaudeFailed {
        exit_code: i32,
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
pub fn list_dispatch_queue(project: &BacklogRepo) -> Vec<BacklogTask> {
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
/// invariant is unit-testable without touching disk.
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

/// Current sidecar format version. Bump when a field's meaning changes; a
/// reader that does not recognise the version treats the file as
/// unverifiable rather than guessing, which is always safe because the only
/// thing a sidecar unlocks is a destructive action.
pub const SIDECAR_VERSION: u32 = 2;

/// What one run's sidecar claims about the process it spawned.
///
/// Every field exists to answer "is the group named here still *this run*?" —
/// none of them is decoration:
///
/// - `pgid` — what to signal.
/// - `supervisor_pid` — the Switchbard process that spawned the agent and is
///   blocked in `wait_for_exit` on it. When this is not the *current* process,
///   there is no pipeline behind the run: nothing will release the task when
///   the agent exits (and, since TASK-46, there is no wall-clock timeout left
///   to fire either way). A UI that keeps promising a release nobody will
///   perform is lying (see [`crate::dispatch_inspect::DispatchRunLiveness`]).
/// - `agent_started_unix` — the run's own start stamp, so a sidecar can be
///   matched against the log it belongs to rather than merely sitting near it.
/// - `boot_epoch_unix` — the boot this pgid was minted under. macOS wraps pids
///   at 99999; across a reboot the same number names a stranger. Without this
///   field a sidecar that outlives a restart is an armed weapon pointed at an
///   arbitrary process group. See [`crate::boot_time`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchSidecar {
    pub pgid: i32,
    pub supervisor_pid: u32,
    pub agent_started_unix: u64,
    pub boot_epoch_unix: u64,
}

impl DispatchSidecar {
    /// Whether the process that spawned this run is *this* process — i.e.
    /// whether there is still a pipeline thread blocked on the agent to
    /// enforce the timeout and release the task afterwards.
    pub fn supervised_by_this_process(&self) -> bool {
        self.supervisor_pid == std::process::id()
    }

    /// Whether the recorded pgid can still mean anything on this machine. A
    /// sidecar from a previous boot names a pid that has since been reissued;
    /// no amount of further probing makes it safe to signal.
    pub fn minted_this_boot(&self) -> bool {
        crate::boot_time::boot_epoch_unix().is_some_and(|boot| boot == self.boot_epoch_unix)
    }

    fn render(&self) -> String {
        format!(
            "v={}\npgid={}\nsupervisor={}\nstarted={}\nboot={}\n",
            SIDECAR_VERSION,
            self.pgid,
            self.supervisor_pid,
            self.agent_started_unix,
            self.boot_epoch_unix,
        )
    }
}

/// Parse a sidecar written by [`write_dispatch_sidecar`].
///
/// `None` for a missing file, a malformed one, an unrecognised version, or a
/// **legacy** bare-pgid sidecar (the pre-versioning format, which carried no
/// boot epoch and therefore cannot be authenticated). Every one of those is
/// the same answer to the caller — *I cannot vouch for this pgid* — and the
/// caller's obligation is identical: offer no kill affordance. Distinguishing
/// them would only let a caller talk itself into signalling anyway.
///
/// Non-positive pgids are rejected because `kill_pgid` negates its argument
/// to address a group: a 0 or negative pgid would widen the signal past the
/// one run it is meant for.
pub fn read_dispatch_sidecar(path: &Path) -> Option<DispatchSidecar> {
    parse_dispatch_sidecar(&std::fs::read_to_string(path).ok()?)
}

/// The pure half of [`read_dispatch_sidecar`], so every rejection rule is
/// testable without touching disk.
pub fn parse_dispatch_sidecar(text: &str) -> Option<DispatchSidecar> {
    let mut version = None;
    let mut pgid = None;
    let mut supervisor = None;
    let mut started = None;
    let mut boot = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "v" => version = value.parse::<u32>().ok(),
            "pgid" => pgid = value.parse::<i32>().ok(),
            "supervisor" => supervisor = value.parse::<u32>().ok(),
            "started" => started = value.parse::<u64>().ok(),
            "boot" => boot = value.parse::<u64>().ok(),
            _ => {}
        }
    }
    if version? != SIDECAR_VERSION {
        return None;
    }
    let sidecar = DispatchSidecar {
        pgid: pgid?,
        supervisor_pid: supervisor?,
        agent_started_unix: started?,
        boot_epoch_unix: boot?,
    };
    // A zero boot epoch is what a platform that wouldn't answer `boot_time`
    // leaves behind; it can never equal a real epoch, but rejecting it here
    // keeps "unverifiable" a property of the file rather than of a comparison
    // somewhere downstream.
    (sidecar.pgid > 0 && sidecar.supervisor_pid > 0 && sidecar.boot_epoch_unix > 0)
        .then_some(sidecar)
}

/// Record everything needed to later authenticate the run's process group.
/// Best-effort by design: failing to write the sidecar costs the user a Kill
/// button, which must never be worth aborting a run that is otherwise fine.
fn write_dispatch_sidecar(path: &Path, sidecar: &DispatchSidecar) {
    let _ = std::fs::write(path, sidecar.render());
}

/// Drop the kill handle. Called as the run releases its claim — see
/// [`dispatch_one`] for why that is the right boundary.
fn remove_pid_sidecar(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Delete a sidecar whose process group is known to be gone.
///
/// Public because the *pipeline* is not always the one that gets to clean up:
/// a Switchbard killed mid-run never reaches its release boundary, and the
/// file it left behind is exactly the stale handle this whole format exists to
/// neutralise. `workers::refresh_dispatch_runs` calls this once a run's group
/// has been positively verified dead **and** its task is no longer claimed —
/// see that call site for why a *claimed* task keeps its dead sidecar (it is
/// the only evidence that turns a permanently-"in flight" row into one that
/// asks for attention).
pub fn sweep_dead_sidecar(task_id: &str, started_at_unix: u64) {
    remove_pid_sidecar(&dispatch_pid_path(task_id, started_at_unix));
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

/// Step 1 of the pipeline: take exclusive ownership of a task.
///
/// Two moves, in this order and for different reasons:
///
/// 1. `dispatch` → `dispatching`, atomically. This is the double-dispatch
///    guard — a queue reload between here and the spawn must never see the
///    task as eligible again — so its failure aborts the claim.
/// 2. Strip any **previous** attempt's terminal labels. The state machine
///    reads as a priority ladder (`dispatched` > `dispatch-failed` >
///    `dispatching` > `dispatch`, see `ui::backlog::dispatch_ui::
///    dispatch_state`), so a task re-flagged after a failure would otherwise
///    carry both `dispatch-failed` and `dispatching` and report as Failed for
///    the entire length of its new run — a live agent rendered as a red alarm
///    that nothing can clear. The ladder is the right shape; carrying a
///    finished run's verdict into the next run's claim is the bug.
///
/// Step 2 is best-effort and non-fatal for the same reason as
/// [`set_dispatch_status`]: by then the guard has already succeeded, and a
/// failure costs a stale pill rather than correctness. Aborting there would
/// strand a task mid-claim, which is strictly worse.
///
/// Public because it is the one step of the pipeline that is meaningfully
/// testable against a real Backlog project without spawning an agent — see
/// `tests/backlog_mutations.rs`.
pub fn claim_task_for_dispatch(repo_root: &Path, task_id: &str) -> Result<()> {
    swap_backlog_label(repo_root, task_id, DISPATCH_LABEL, DISPATCHING_LABEL)
        .with_context(|| format!("failed to claim {task_id} for dispatch"))?;
    for stale in [DISPATCH_FAILED_LABEL, DISPATCHED_LABEL] {
        let _ = set_backlog_label(repo_root, task_id, stale, false);
    }
    Ok(())
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
    claim_task_for_dispatch(repo_root, &task.id)?;

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
    // exited on its own or was killed from the Dispatch view (TASK-46
    // removed the third option, a wall-clock kill). Drop the kill handle
    // here, at the release boundary, so a sidecar on disk never names a
    // process group that is not this run's live one. (The remaining
    // commit/push/PR tail is Switchbard's own work, not the agent's; there
    // is deliberately nothing to signal for it.)
    remove_pid_sidecar(&paths.pid_path);
    let exit = match exit {
        Ok(exit) => exit,
        Err(e) => {
            release_as_failed(repo_root, &task.id, &e.to_string(), &prior_status);
            return Err(e);
        }
    };

    let result = match exit {
        0 => finish_pipeline(task, &paths.worktree_path, &paths.branch, opts),
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
    project: &BacklogRepo,
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
    /// The run's own start stamp — the same one embedded in every path above.
    /// Carried explicitly so the sidecar can record it as a field rather than
    /// re-deriving it by parsing one of its own filenames back.
    started_at_unix: u64,
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
    let started_at_unix = unix_now();
    let stem = dispatch_log_stem(&task.id, started_at_unix);
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
        started_at_unix,
    })
}

/// Spawn the headless `claude -p` run and block until it exits. No wall-clock
/// kill (TASK-46, LED-580): the wait is, for all practical purposes,
/// unbounded — see [`NO_WALL_CLOCK_LIMIT`] — so this returns only once the
/// agent itself exits, carrying its real exit code. From here on, the only
/// things that can end a run early are [`DispatchOptions::max_turns`]
/// (enforced by the agent, not this function) and a manual kill through
/// `crate::dispatch_kill::kill_dispatch_run`, which ends this same blocking
/// wait by signalling the process group directly — see the module doc's
/// "Bounding a runaway run" section.
fn run_claude_headless(paths: &DispatchPaths, opts: &DispatchOptions) -> Result<i32> {
    let command = build_claude_command(&paths.prompt_path, opts);
    let run = spawn_in_session(&command, &paths.worktree_path, &paths.log_path)
        .context("failed to spawn claude")?;
    // Written *after* the spawn (the kernel assigns the group) and before the
    // blocking wait, so the sidecar exists for essentially the whole window in
    // which there is something to kill.
    write_dispatch_sidecar(
        &paths.pid_path,
        &DispatchSidecar {
            pgid: run.pgid,
            // This process is the supervisor precisely because it is about to
            // block on the agent below. If Switchbard dies, that fact dies
            // with it — and the recorded pid is how a later Switchbard finds
            // out rather than assuming it inherited the duty.
            supervisor_pid: std::process::id(),
            agent_started_unix: paths.started_at_unix,
            boot_epoch_unix: crate::boot_time::boot_epoch_unix().unwrap_or(0),
        },
    );
    match wait_for_exit(run.pid, NO_WALL_CLOCK_LIMIT).context("failed waiting on claude")? {
        WaitOutcome::Exited(code) => Ok(code),
        WaitOutcome::TimedOut => {
            // Reachable only if `NO_WALL_CLOCK_LIMIT` itself elapses — see
            // its doc. That is not a real outcome for this pipeline; treating
            // it as the internal-invariant violation it would be is more
            // honest than silently killing the run to force an answer, which
            // is exactly the wall-clock-kill behavior TASK-46 removed.
            unreachable!(
                "wait_for_exit reported TimedOut against a {}-year deadline — this should be unreachable",
                NO_WALL_CLOCK_LIMIT.as_secs() / (60 * 60 * 24 * 365)
            )
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
///
/// Public since TASK-88: the `switchbard-task queue release` verb is the
/// orchestrator's way to hand a claim back, and it must walk the exact same
/// ladder this pipeline walks — two release implementations would be two
/// claim vocabularies.
pub fn release_as_failed(repo_root: &Path, task_id: &str, reason: &str, prior_status: &str) {
    // Best-effort: if the label swap itself fails there is nothing more we
    // can do here beyond leaving the task on `dispatching` for a human to
    // notice. Never panic a background worker over a bookkeeping write.
    let _ = swap_backlog_label(repo_root, task_id, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL);
    set_dispatch_status(repo_root, task_id, prior_status);
    let _ = append_backlog_notes(repo_root, task_id, &format!("Dispatch failed: {reason}"));
}

/// Public since TASK-88 — see [`release_as_failed`]'s note.
pub fn release_as_dispatched(repo_root: &Path, task_id: &str, pr_url: &str) -> Result<()> {
    swap_backlog_label(repo_root, task_id, DISPATCHING_LABEL, DISPATCHED_LABEL)?;
    set_dispatch_status(repo_root, task_id, DISPATCH_REVIEW_STATUS);
    append_backlog_notes(repo_root, task_id, &format!("Dispatch PR: {pr_url}"))?;
    Ok(())
}

fn describe_result(result: &DispatchResult) -> String {
    match result {
        DispatchResult::PrOpened { url } => format!("PR opened: {url}"),
        DispatchResult::NoChanges => "claude made no changes".to_string(),
        DispatchResult::ClaudeFailed { exit_code } => format!("claude exited with {exit_code}"),
        DispatchResult::GitFailed { stage, message } => format!("git {stage} failed: {message}"),
        DispatchResult::GhFailed { message } => format!("gh pr create failed: {message}"),
    }
}

/// Quote a path for interpolation into a `/bin/sh -c` string (the one place
/// this module builds a shell command instead of passing argv directly, to
/// pipe the prompt file into `claude -p`'s stdin). `pub(crate)` because
/// `crate::refine` pipes its prompt into the same binary the same way —
/// one shell-quoting rule, not two.
pub(crate) fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::{load_backlog_repo, BacklogChecklistItem};
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
            project: None,
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
        let project = BacklogRepo {
            root: PathBuf::from("/repo"),
            tasks: vec![
                task("TASK-2", &["dispatch"]),
                task("TASK-1", &["dispatch", "hub"]),
                task("TASK-3", &["dispatching"]),
                task("TASK-4", &[]),
            ],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: crate::backlog::RepoRanking::default(),
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
        let project = BacklogRepo {
            root: PathBuf::from("/repo"),
            tasks: vec![done],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: crate::backlog::RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![],
        };

        assert!(list_dispatch_queue(&project).is_empty());
    }

    #[test]
    fn list_dispatch_queue_filters_real_fixture_files_on_disk() {
        // Exercises the module against real markdown parsed by
        // `load_backlog_repo` (not hand-built structs), per the mission's
        // "unit-test queue/guard logic against fixture repos under $TMPDIR".
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog/tasks")).unwrap();
        write_fixture_task(dir.path(), "TASK-1", &["dispatch"]);
        write_fixture_task(dir.path(), "TASK-2", &["dispatching"]);
        write_fixture_task(dir.path(), "TASK-3", &[]);
        write_fixture_task(dir.path(), "TASK-4", &["dispatch", "hub"]);

        let project = load_backlog_repo(dir.path()).unwrap();
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

    /// TASK-46, AC #1: no code path kills a dispatch run on wall-clock time.
    ///
    /// Proven directly against `run_claude_headless` rather than against
    /// `stale_after`'s value alone: a stand-in "claude" binary outlives an
    /// absurdly short `stale_after` and the run still comes back with its
    /// real exit code. If any code path still enforced `stale_after` as a
    /// deadline, this test would see the process killed (a signal exit,
    /// decoding to `-1`) instead of the `7` the stand-in actually exits with.
    #[test]
    fn run_claude_headless_ignores_stale_after_and_returns_the_real_exit_code() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let fake_claude = dir.path().join("fake-claude.sh");
        fs::write(
            &fake_claude,
            "#!/bin/sh\ncat >/dev/null\nsleep 0.3\nexit 7\n",
        )
        .unwrap();
        let mut perms = fs::metadata(&fake_claude).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_claude, perms).unwrap();

        let prompt_path = dir.path().join("prompt.md");
        fs::write(&prompt_path, "hi").unwrap();

        let opts = DispatchOptions {
            claude_binary: fake_claude.display().to_string(),
            // Far shorter than the 300ms the stand-in sleeps for. A real
            // wall-clock enforcement of this value would kill the run long
            // before it reaches `exit 7`.
            stale_after: Duration::from_millis(1),
            ..Default::default()
        };
        let paths = DispatchPaths {
            worktree_path: dir.path().to_path_buf(),
            branch: "dispatch/task-x".to_string(),
            log_path: dir.path().join("run.log"),
            prompt_path,
            pid_path: dir.path().join("run.pid"),
            started_at_unix: unix_now(),
        };

        let exit = run_claude_headless(&paths, &opts).expect("run completes");

        assert_eq!(
            exit, 7,
            "stale_after must not cut the run off — it should run to its own exit code"
        );
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

    fn sample_sidecar() -> DispatchSidecar {
        DispatchSidecar {
            pgid: 4242,
            supervisor_pid: 99,
            agent_started_unix: 1_700_000_000,
            boot_epoch_unix: 1_600_000_000,
        }
    }

    /// The whole sidecar lifecycle in one place: written at spawn, readable
    /// while the run is live, gone once the run releases.
    #[test]
    fn dispatch_sidecar_round_trips_and_is_removed_on_release() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dispatch-task-11-1700000000.pid");

        assert_eq!(read_dispatch_sidecar(&path), None, "nothing written yet");

        let written = sample_sidecar();
        write_dispatch_sidecar(&path, &written);
        assert!(path.exists());
        assert_eq!(read_dispatch_sidecar(&path), Some(written));

        remove_pid_sidecar(&path);
        assert!(!path.exists());
        assert_eq!(read_dispatch_sidecar(&path), None);
    }

    /// Every field has to survive the round trip, not just the pgid: dropping
    /// the boot epoch or the supervisor pid on the floor would leave a
    /// sidecar that *parses* but can no longer be authenticated, which is the
    /// exact failure this format was introduced to remove.
    #[test]
    fn every_authenticating_field_survives_the_round_trip() {
        let written = sample_sidecar();

        let read = parse_dispatch_sidecar(&written.render()).expect("round trip");

        assert_eq!(read.pgid, 4242);
        assert_eq!(read.supervisor_pid, 99);
        assert_eq!(read.agent_started_unix, 1_700_000_000);
        assert_eq!(read.boot_epoch_unix, 1_600_000_000);
    }

    /// A pre-versioning bare-pgid file is the dangerous legacy case: it looks
    /// like a perfectly usable number and carries nothing to check it
    /// against. It must never parse.
    #[test]
    fn a_legacy_bare_pgid_sidecar_never_parses() {
        assert_eq!(parse_dispatch_sidecar("4242\n"), None);
        assert_eq!(parse_dispatch_sidecar("4242"), None);
    }

    /// An unknown version is a file written by a future Switchbard whose
    /// field meanings we cannot assume. Refuse it rather than guess.
    #[test]
    fn an_unknown_sidecar_version_never_parses() {
        let future = sample_sidecar().render().replace("v=2", "v=99");

        assert_eq!(parse_dispatch_sidecar(&future), None);
    }

    /// Any missing field breaks authentication, so any missing field must
    /// break parsing — checked one at a time so a future refactor that drops
    /// one silently can't slip through.
    #[test]
    fn a_sidecar_missing_any_field_never_parses() {
        for dropped in ["v=", "pgid=", "supervisor=", "started=", "boot="] {
            let text: String = sample_sidecar()
                .render()
                .lines()
                .filter(|line| !line.starts_with(dropped))
                .map(|line| format!("{line}\n"))
                .collect();
            assert_eq!(
                parse_dispatch_sidecar(&text),
                None,
                "a sidecar without {dropped} must not parse"
            );
        }
    }

    /// `kill_pgid` negates its argument to address a process group, so a 0 or
    /// negative pgid would widen the signal far past the one run it names.
    /// A zero boot epoch is what a platform that wouldn't answer leaves
    /// behind, and can never authenticate.
    #[test]
    fn a_sidecar_with_an_unusable_field_never_parses() {
        for (field, bad) in [
            ("pgid=4242", "pgid=0"),
            ("pgid=4242", "pgid=-1"),
            ("supervisor=99", "supervisor=0"),
            ("boot=1600000000", "boot=0"),
        ] {
            let text = sample_sidecar().render().replace(field, bad);
            assert_eq!(parse_dispatch_sidecar(&text), None, "{bad} must not parse");
        }
    }

    #[test]
    fn a_malformed_sidecar_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        for (name, contents) in [
            ("empty.pid", ""),
            ("blank.pid", "   \n"),
            ("words.pid", "not-a-pid\n"),
            ("halfway.pid", "v=2\npgid=notanumber\n"),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, contents).unwrap();
            assert_eq!(read_dispatch_sidecar(&path), None, "{name} must not parse");
        }
    }

    /// The writer emits trailing newlines and the reader has to tolerate
    /// them (plus any stray padding); a regression here would silently
    /// disable every Kill button.
    #[test]
    fn the_sidecar_reader_tolerates_the_writers_whitespace() {
        let padded = "v = 2\n pgid = 91234 \nsupervisor= 7\nstarted =1\nboot= 2\n\n";

        let read = parse_dispatch_sidecar(padded).expect("whitespace must not defeat the parser");

        assert_eq!(read.pgid, 91234);
        assert_eq!(read.supervisor_pid, 7);
    }

    /// The supervisor check is what tells a restarted Switchbard that it is
    /// *not* the process blocked on this agent — the whole basis of the
    /// unsupervised-run treatment in the Dispatch view.
    #[test]
    fn supervision_is_decided_by_the_recorded_pid_not_by_hope() {
        let mine = DispatchSidecar {
            supervisor_pid: std::process::id(),
            ..sample_sidecar()
        };
        let theirs = DispatchSidecar {
            supervisor_pid: std::process::id().wrapping_add(1).max(1),
            ..sample_sidecar()
        };

        assert!(mine.supervised_by_this_process());
        assert!(!theirs.supervised_by_this_process());
    }

    /// A sidecar from another boot names a recycled pid. This is the guard
    /// that stands between a force-quit-and-restart and a SIGKILL aimed at a
    /// stranger.
    #[test]
    fn a_sidecar_from_another_boot_is_never_minted_this_boot() {
        let stale = DispatchSidecar {
            boot_epoch_unix: 1,
            ..sample_sidecar()
        };

        assert!(!stale.minted_this_boot());

        if let Some(boot) = crate::boot_time::boot_epoch_unix() {
            let current = DispatchSidecar {
                boot_epoch_unix: boot,
                ..sample_sidecar()
            };
            assert!(current.minted_this_boot());
        }
    }

    /// Removing a sidecar that is already gone is the normal case on any
    /// early-exit path — it must be silent, not an error a caller has to
    /// thread through the release bookkeeping.
    #[test]
    fn removing_a_missing_pid_sidecar_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();

        remove_pid_sidecar(&dir.path().join("never-written.pid"));
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
    /// **The reaper thread is load-bearing, not scaffolding.** It reproduces
    /// the production topology — the pipeline thread sits in `wait_for_exit`
    /// while the UI thread kills — and without it this test fails with a
    /// misleading EPERM: a killed child that nobody `waitpid`s becomes a
    /// zombie, macOS answers `kill(-pgid, 0)` on a zombie group with EPERM,
    /// and `kill.rs` reads EPERM as "still alive" and escalates into a second
    /// EPERM. In production a `wait_for_exit` is *always* concurrently
    /// reaping, so that state is unreachable there. See `kill_pgid`'s own doc.
    #[test]
    fn killing_a_runs_process_group_via_its_sidecar_unblocks_the_pipelines_wait() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("run.log");
        let pid_path = dir.path().join("run.pid");

        // `sleep 600` stands in for the headless agent: it outlives any
        // plausible test runtime, so the only way this test finishes is if
        // the kill actually lands.
        let run = spawn_in_session("exec sleep 600", dir.path(), &log_path).unwrap();
        write_dispatch_sidecar(
            &pid_path,
            &DispatchSidecar {
                pgid: run.pgid,
                supervisor_pid: std::process::id(),
                agent_started_unix: unix_now(),
                boot_epoch_unix: crate::boot_time::boot_epoch_unix().unwrap_or(0),
            },
        );

        let sidecar = read_dispatch_sidecar(&pid_path).expect("sidecar must name the group");
        assert_eq!(sidecar.pgid, run.pgid);
        assert!(sidecar.supervised_by_this_process());
        assert!(sidecar.minted_this_boot());

        // Stand in for the pipeline thread blocked in `dispatch_one`.
        let pid = run.pid;
        let pipeline = std::thread::spawn(move || wait_for_exit(pid, Duration::from_secs(10)));

        let outcome = kill_pgid(sidecar.pgid, KILL_GRACE).unwrap();
        assert!(
            matches!(
                outcome,
                crate::kill::KillOutcome::Terminated | crate::kill::KillOutcome::Killed
            ),
            "the group must die on one signal: {outcome:?}"
        );

        let waited = pipeline.join().unwrap().unwrap();
        assert!(
            matches!(waited, WaitOutcome::Exited(_)),
            "the pipeline's blocked wait must return on the kill, not time out: {waited:?}"
        );

        remove_pid_sidecar(&pid_path);
        assert_eq!(read_dispatch_sidecar(&pid_path), None);
    }

    #[test]
    fn describe_result_is_human_readable_for_every_failure_variant() {
        assert!(describe_result(&DispatchResult::NoChanges).contains("no changes"));
        assert!(describe_result(&DispatchResult::ClaudeFailed { exit_code: 1 }).contains('1'));
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
