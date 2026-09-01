//! The `queue` verb family - the agent-facing protocol surface the
//! orchestrator drives (trajectory: *Task Queue orchestration*, TASK-88;
//! designed against `~/.claude/standards/agent-facing-design.md`).
//!
//! The queue is the dispatch-labeled task set in the repo's stack-rank
//! computed order (no separate store - `crate` reads it straight off
//! `load_backlog_repo`'s already-sorted tasks). The protocol's spine:
//!
//! - **Claim is the acknowledgment.** `queue claim` performs the same
//!   `dispatch` -> `dispatching` label swap `dispatch_one` starts with,
//!   *before* any work happens - a killed reader loses nothing, and a
//!   reload never re-offers an in-flight task.
//! - **stdout carries only the payload.** `list` is TSV rows; `claim`
//!   prints the task's prior status (the one datum `release --outcome
//!   failed` needs to restore); `prompt` prints the exact agent prompt;
//!   the mutating verbs print `Edited <ID>` / `no changes`.
//! - **Every error names the next step** on one stderr line, and every
//!   release walks the exact ladder the Rust pipeline walks
//!   (`release_as_dispatched` / `release_as_failed`) - one claim
//!   vocabulary, not two.

use anyhow::{anyhow, bail, Result};
use clap::Subcommand;
use std::path::Path;
use switchbard_core::{
    BacklogRepo, BacklogTask, BacklogTaskPatch, DISPATCHED_LABEL, DISPATCHING_LABEL,
    DISPATCH_FAILED_LABEL, DISPATCH_IN_PROGRESS_STATUS, DISPATCH_LABEL,
};

#[derive(Subcommand)]
pub enum QueueCmd {
    /// The dispatch queue in stack-rank order: one TSV row per task -
    /// id, state (queued|claimed|dispatched|failed), priority, project,
    /// title. Default shows the live queue (queued + claimed)
    List {
        /// Include finished states (dispatched / failed)
        #[arg(long)]
        all: bool,
    },
    /// Label a task for dispatch - it joins the queue at its rank position;
    /// prints `Edited <ID>` or `no changes`
    Send { id: String },
    /// Remove a task from the queue. Refuses a claimed (in-flight) task -
    /// release or kill the run first
    Withdraw { id: String },
    /// Acknowledge the handoff: swaps dispatch -> dispatching and moves the
    /// task to In Progress. stdout is the task's PRIOR status alone - keep
    /// it, `release --outcome failed --prior-status <it>` restores it
    Claim { id: String },
    /// Hand the claim back with the outcome; prints `Edited <ID>`
    Release {
        id: String,
        /// dispatched (requires --pr) or failed (requires --note)
        #[arg(long, value_parser = ["dispatched", "failed"])]
        outcome: String,
        /// The opened PR's URL (dispatched outcome)
        #[arg(long, value_name = "URL")]
        pr: Option<String>,
        /// The failure reason, appended to the task's notes (failed outcome)
        #[arg(long, value_name = "TEXT")]
        note: Option<String>,
        /// Status to restore on failure - `claim` printed it (default: To Do)
        #[arg(long, value_name = "STATUS")]
        prior_status: Option<String>,
    },
    /// Print the exact headless-agent prompt for a task (the same
    /// build_dispatch_prompt output the Rust pipeline feeds `claude -p`)
    Prompt { id: String },
}

/// A task's place on the dispatch label ladder, as `queue list` reports it.
fn queue_state(task: &BacklogTask) -> Option<&'static str> {
    let has = |label: &str| task.labels.iter().any(|l| l == label);
    if has(DISPATCHING_LABEL) {
        Some("claimed")
    } else if has(DISPATCH_LABEL) {
        Some("queued")
    } else if has(DISPATCHED_LABEL) {
        Some("dispatched")
    } else if has(DISPATCH_FAILED_LABEL) {
        Some("failed")
    } else {
        None
    }
}

pub fn run_queue(root: &Path, cmd: &QueueCmd) -> Result<()> {
    match cmd {
        QueueCmd::List { all } => list(root, *all),
        QueueCmd::Send { id } => set_queued(root, id, true),
        QueueCmd::Withdraw { id } => set_queued(root, id, false),
        QueueCmd::Claim { id } => claim(root, id),
        QueueCmd::Release {
            id,
            outcome,
            pr,
            note,
            prior_status,
        } => release(
            root,
            id,
            outcome,
            pr.as_deref(),
            note.as_deref(),
            prior_status.as_deref(),
        ),
        QueueCmd::Prompt { id } => {
            let repo = load(root)?;
            let task = resolve(&repo, id)?;
            print!("{}", switchbard_core::build_dispatch_prompt(task));
            Ok(())
        }
    }
}

fn load(root: &Path) -> Result<BacklogRepo> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    for warning in &repo.warnings {
        eprintln!("sb: warning: {warning}");
    }
    Ok(repo)
}

fn resolve<'r>(repo: &'r BacklogRepo, id: &str) -> Result<&'r BacklogTask> {
    crate::find_task(&repo.tasks, id).ok_or_else(|| {
        anyhow!(
            "no task {id} in {} — try `sb list --all`",
            repo.root.display()
        )
    })
}

/// `repo.tasks` already carries the stack-rank computed order (applied in
/// `load_backlog_repo`), so filtering preserves exactly the tee-up order.
fn list(root: &Path, all: bool) -> Result<()> {
    let repo = load(root)?;
    for task in &repo.tasks {
        let Some(state) = queue_state(task) else {
            continue;
        };
        if !all && !matches!(state, "queued" | "claimed") {
            continue;
        }
        println!(
            "{}\t{}\t{}\t{}\t{}",
            task.id,
            state,
            task.priority,
            task.project.as_deref().unwrap_or(""),
            task.title
        );
    }
    Ok(())
}

fn set_queued(root: &Path, id: &str, queued: bool) -> Result<()> {
    let repo = load(root)?;
    let task = resolve(&repo, id)?;
    if queued && queue_state(task) == Some("claimed") {
        bail!(
            "{} is claimed (in flight) - it is already being worked; `queue release` ends the run",
            task.id
        );
    }
    if !queued && queue_state(task) == Some("claimed") {
        bail!(
            "{} is claimed (in flight) - release it first (`queue release {} --outcome failed --note <why>`) or kill the run from the Dispatches view",
            task.id,
            task.id
        );
    }
    let outcome = switchbard_core::set_backlog_label(root, &task.id, DISPATCH_LABEL, queued)?;
    println!("{outcome}");
    Ok(())
}

fn claim(root: &Path, id: &str) -> Result<()> {
    let repo = load(root)?;
    let task = resolve(&repo, id)?;
    match queue_state(task) {
        Some("queued") => {}
        Some("claimed") => bail!(
            "{} is already claimed - a second claim would double-run it; `queue list` shows the live queue",
            task.id
        ),
        other => bail!(
            "{} is not queued (state: {}) - send it first with `queue send {}`",
            task.id,
            other.unwrap_or("unlabeled"),
            task.id
        ),
    }
    let prior_status = task.status.clone();
    switchbard_core::claim_task_for_dispatch(root, &task.id)?;
    // Same best-effort status move dispatch_one makes; a failure costs a
    // stale status pill, not correctness.
    let _ = switchbard_core::edit_backlog_task(
        root,
        &task.id,
        &BacklogTaskPatch {
            status: Some(DISPATCH_IN_PROGRESS_STATUS.to_string()),
            ..Default::default()
        },
    );
    println!("{prior_status}");
    Ok(())
}

fn release(
    root: &Path,
    id: &str,
    outcome: &str,
    pr: Option<&str>,
    note: Option<&str>,
    prior_status: Option<&str>,
) -> Result<()> {
    let repo = load(root)?;
    let task = resolve(&repo, id)?;
    if queue_state(task) != Some("claimed") {
        bail!(
            "{} is not claimed (state: {}) - nothing to release; `queue claim {}` acknowledges a queued task",
            task.id,
            queue_state(task).unwrap_or("unlabeled"),
            task.id
        );
    }
    match outcome {
        "dispatched" => {
            let Some(pr_url) = pr else {
                bail!("--outcome dispatched requires --pr <URL> - the PR is the evidence, not the narration");
            };
            switchbard_core::release_as_dispatched(root, &task.id, pr_url)?;
            if let Some(note) = note {
                let _ = switchbard_core::append_backlog_notes(root, &task.id, note);
            }
        }
        "failed" => {
            let Some(reason) = note else {
                bail!(
                    "--outcome failed requires --note <TEXT> - the next reader needs to know why"
                );
            };
            switchbard_core::release_as_failed(
                root,
                &task.id,
                reason,
                prior_status.unwrap_or("To Do"),
            );
        }
        other => bail!("unknown outcome `{other}` - dispatched or failed"),
    }
    println!("Edited {}", task.id);
    Ok(())
}
