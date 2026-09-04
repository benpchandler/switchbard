//! `sb work`: the live-work protocol between an agent session, the board, and
//! the harness (TASK-150). Designed against
//! `~/.claude/standards/agent-facing-design.md`:
//!
//! - **Claim is the acknowledgment.** `work claim` records the session on the
//!   task before any edit; `sbt` shows the row as being worked from that
//!   moment. It also moves the task to `In Progress` and hands the ball to
//!   the agent, best effort, so a reader of the board sees it without `sbt`.
//! - **Release carries the outcome.** `work release` refuses to let go of a
//!   task with unchecked acceptance criteria unless `--note` says why - the
//!   next reader must never guess whether the work was finished or dropped.
//!   Either way the ball goes back to the owner.
//! - **Pass is the human's word.** `work pass` (and `w` in `sbt`) releases
//!   every session's claim on a task regardless of its criteria.
//! - **The hook is the enforcement.** `work hook` reads a Claude Code hook
//!   event on stdin and answers on stdout: deny edits until a claim exists in
//!   this repo, hold the session at Stop while it still holds claims (bounded
//!   by `--max-stop-blocks`, after which the session is marked abandoned so
//!   the board shows an honest failure instead of a looping agent), and drop
//!   the record at SessionEnd. Every other event, an edit to a file outside
//!   the repo (the agent's own memory, scratch files), and any repo without
//!   a backlog, is a silent exit 0.
//!
//! stdout carries the payload only; narration goes to stderr.

use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
use std::path::{Path, PathBuf};
use switchbard_core::{
    BacklogRepo, BacklogTask, BacklogTaskPatch, Ball, WorkIdentity, WorkSession,
    DISPATCH_IN_PROGRESS_STATUS,
};

#[derive(Subcommand)]
pub enum WorkCmd {
    /// Record that this session is working <ID>: the row lights up in sbt,
    /// the task moves to In Progress, the ball goes to the agent. Repeat for
    /// every task the session holds. Identity comes from CLAUDE_CODE_SESSION_ID
    /// and CLAUDE_PID unless given
    Claim {
        id: String,
        #[command(flatten)]
        identity: IdentityArgs,
    },
    /// Let go of <ID>. Refused while acceptance criteria are unchecked unless
    /// --note explains what was left; the ball returns to the owner either way
    Release {
        id: String,
        /// Why the task is being released unfinished (appended to its notes)
        #[arg(long)]
        note: Option<String>,
        #[command(flatten)]
        identity: IdentityArgs,
    },
    /// The owner's word: release every session's claim on <ID>
    Pass { id: String },
    /// Live sessions in this repo: one TSV row per claim - session, pid,
    /// agent, task id, claimed at
    List,
    /// Claude Code hook entry point: reads the hook event JSON on stdin,
    /// answers on stdout (PreToolUse deny / Stop block / SessionEnd cleanup)
    Hook {
        /// How many times Stop may be held before the session is let go and
        /// marked abandoned
        #[arg(long, default_value_t = 5)]
        max_stop_blocks: u32,
    },
}

#[derive(clap::Args)]
pub struct IdentityArgs {
    /// Session id (default: $CLAUDE_CODE_SESSION_ID)
    #[arg(long)]
    session: Option<String>,
    /// The agent process id whose liveness keeps the claim alive
    /// (default: $CLAUDE_PID)
    #[arg(long)]
    pid: Option<u32>,
}

impl IdentityArgs {
    fn resolve(&self) -> Result<WorkIdentity> {
        let env = WorkIdentity::from_env();
        let session_id = self
            .session
            .clone()
            .or_else(|| env.as_ref().map(|id| id.session_id.clone()))
            .ok_or_else(|| {
                anyhow!("no session identity: run inside a Claude Code shell or pass --session <ID> --pid <PID>")
            })?;
        let pid = self
            .pid
            .or_else(|| env.as_ref().map(|id| id.pid))
            .ok_or_else(|| {
                anyhow!("no agent pid: pass --pid <PID> (the process whose exit ends the claim)")
            })?;
        Ok(WorkIdentity {
            session_id,
            pid,
            agent: "claude".to_string(),
        })
    }
}

pub fn run_work(root: &Path, cmd: &WorkCmd) -> Result<()> {
    let dir = work_dir()?;
    match cmd {
        WorkCmd::Claim { id, identity } => claim(root, &dir, id, &identity.resolve()?),
        WorkCmd::Release { id, note, identity } => {
            release(root, &dir, id, note.as_deref(), &identity.resolve()?)
        }
        WorkCmd::Pass { id } => pass(root, &dir, id),
        WorkCmd::List => list(root, &dir),
        WorkCmd::Hook { max_stop_blocks } => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            hook(&dir, &text, *max_stop_blocks)
        }
    }
}

fn work_dir() -> Result<PathBuf> {
    switchbard_core::default_work_dir()
        .ok_or_else(|| anyhow!("no home directory for the work store"))
}

fn claim(root: &Path, dir: &Path, id: &str, identity: &WorkIdentity) -> Result<()> {
    let repo = load(root)?;
    let task = resolve(&repo, id)?;
    let session = switchbard_core::claim_work(dir, identity, root, &task.id)?;
    // Board-visible side effects are best effort, like `queue claim`: a
    // failure costs a stale pill, not the claim itself.
    if task.status != DISPATCH_IN_PROGRESS_STATUS {
        let _ = switchbard_core::edit_backlog_task(
            root,
            &task.id,
            &BacklogTaskPatch {
                status: Some(DISPATCH_IN_PROGRESS_STATUS.to_string()),
                ..Default::default()
            },
        );
    }
    let _ = switchbard_core::set_backlog_ball(root, &task.id, Some(Ball::Agent));
    println!(
        "Claimed {} (session {} holds: {})",
        task.id,
        session.short_id(),
        switchbard_core::held_ids(&session)
    );
    Ok(())
}

fn release(
    root: &Path,
    dir: &Path,
    id: &str,
    note: Option<&str>,
    identity: &WorkIdentity,
) -> Result<()> {
    let repo = load(root)?;
    let task = resolve(&repo, id)?;
    let unchecked = unchecked_criteria(task);
    if !unchecked.is_empty() && note.is_none() {
        bail!(
            "{} still has {} unchecked acceptance criteria:\n{}\ncheck them (`sb edit {} --check-ac N`) or release with --note <why it is left unfinished>",
            task.id,
            unchecked.len(),
            unchecked.join("\n"),
            task.id
        );
    }
    let session = switchbard_core::release_work(dir, &identity.session_id, &task.id)?;
    if let Some(note) = note {
        let _ = switchbard_core::append_backlog_notes(
            root,
            &task.id,
            &format!(
                "Released unfinished by session {}: {note}",
                session.short_id()
            ),
        );
    }
    let _ = switchbard_core::set_backlog_ball(root, &task.id, Some(Ball::Me));
    println!(
        "Released {} (session {} still holds: {})",
        task.id,
        session.short_id(),
        switchbard_core::held_ids(&session)
    );
    Ok(())
}

fn pass(root: &Path, dir: &Path, id: &str) -> Result<()> {
    let repo = load(root)?;
    let task = resolve(&repo, id)?;
    let released = switchbard_core::pass_work(dir, root, &task.id)?;
    if released.is_empty() {
        println!("no session is working {}", task.id);
        return Ok(());
    }
    let _ = switchbard_core::set_backlog_ball(root, &task.id, None);
    println!(
        "Passed {} (released from {})",
        task.id,
        released
            .iter()
            .map(WorkSession::short_id)
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

fn list(root: &Path, dir: &Path) -> Result<()> {
    for session in switchbard_core::list_work_sessions(dir, root)? {
        let state = if session.abandoned {
            "abandoned"
        } else {
            "live"
        };
        for claim in &session.claims {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                session.session_id,
                session.pid,
                session.agent,
                state,
                claim.task_id,
                claim.claimed_at
            );
        }
    }
    Ok(())
}

fn unchecked_criteria(task: &BacklogTask) -> Vec<String> {
    task.acceptance_criteria
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.checked)
        .map(|(index, item)| format!("  {}. {}", index + 1, item.text))
        .collect()
}

/// The fields of a Claude Code hook event this command reads.
#[derive(Deserialize)]
struct HookEvent {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    hook_event_name: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    cwd: Option<PathBuf>,
    /// The editing tools' arguments; only `file_path` is read, to gate edits
    /// inside the repo and leave the agent's own notes and memory alone.
    #[serde(default)]
    tool_input: ToolInput,
}

#[derive(Deserialize, Default)]
struct ToolInput {
    #[serde(default)]
    file_path: Option<PathBuf>,
}

const EDITING_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

fn hook(dir: &Path, text: &str, max_stop_blocks: u32) -> Result<()> {
    let event: HookEvent =
        serde_json::from_str(text).context("hook input is not a Claude Code hook event")?;
    if event.session_id.is_empty() {
        return Ok(());
    }
    let Some(root) = event.cwd.as_deref().and_then(crate::find_repo_root) else {
        return Ok(());
    };
    let session = switchbard_core::load_work_session(dir, &event.session_id)?
        .filter(|session| session.claims_in(&root));
    match event.hook_event_name.as_str() {
        "PreToolUse"
            if EDITING_TOOLS.contains(&event.tool_name.as_str())
                && edits_inside(&root, event.tool_input.file_path.as_deref()) =>
        {
            let held = session
                .as_ref()
                .filter(|session| !session.claims.is_empty() && !session.abandoned);
            if held.is_none() {
                println!(
                    "{}",
                    json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PreToolUse",
                            "permissionDecision": "deny",
                            "permissionDecisionReason": format!(
                                "No task claimed in {}. Claim the task this edit serves first: `sb work claim <ID>` (`sb list` shows the board). The claim keeps the row lit in sbt until you `sb work release <ID>`.",
                                root.display()
                            ),
                        }
                    })
                );
            }
        }
        "Stop" => {
            let Some(session) = session.filter(|session| !session.claims.is_empty()) else {
                return Ok(());
            };
            if session.abandoned {
                return Ok(());
            }
            let blocks = switchbard_core::record_stop_block(dir, &session.session_id)?;
            if blocks > max_stop_blocks {
                switchbard_core::abandon_work_session(dir, &session.session_id)?;
                eprintln!(
                    "sb work: session {} held Stop {} times and still holds {}; letting go and marking it abandoned",
                    session.short_id(),
                    blocks - 1,
                    switchbard_core::held_ids(&session)
                );
                return Ok(());
            }
            println!(
                "{}",
                json!({
                    "decision": "block",
                    "reason": format!(
                        "You still hold {} (stop held {}/{}). Keep working until every acceptance criterion is checked, then `sb work release <ID>`; if you must stop unfinished, `sb work release <ID> --note <what is left and why>`. Only the owner's `sb work pass <ID>` ends a claim otherwise.",
                        switchbard_core::held_ids(&session), blocks, max_stop_blocks
                    ),
                })
            );
        }
        "SessionEnd" => switchbard_core::end_work_session(dir, &event.session_id)?,
        _ => {}
    }
    Ok(())
}

/// Whether an edit lands in the repo the claim is about. A relative path is
/// taken against the repo; no path at all (a tool this hook does not know the
/// shape of) counts as inside, so the gate fails closed on the repo's files.
fn edits_inside(root: &Path, file_path: Option<&Path>) -> bool {
    let Some(file_path) = file_path else {
        return true;
    };
    let absolute = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        root.join(file_path)
    };
    resolve_existing_prefix(&absolute).starts_with(resolve_existing_prefix(root))
}

/// Canonicalize the longest existing ancestor (a new file has none of its
/// own yet) and re-append the rest, so `/var` and `/private/var` compare equal.
fn resolve_existing_prefix(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut current = path;
    loop {
        if let Ok(canonical) = std::fs::canonicalize(current) {
            return rest
                .iter()
                .rev()
                .fold(canonical, |acc, part| acc.join(part));
        }
        match (current.file_name(), current.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                current = parent;
            }
            _ => return path.to_path_buf(),
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
            "no task {id} in {} - try `sb list --all`",
            repo.root.display()
        )
    })
}
