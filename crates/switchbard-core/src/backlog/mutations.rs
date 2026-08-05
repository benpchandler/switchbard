//! Wrappers around the `backlog` CLI that mutate task state on disk. Every
//! function here shells out via `run_backlog`; none of them parse task
//! markdown themselves (see `super::parse` for that side).

use super::parse::backlog_cli_path;
use super::types::{BacklogTaskPatch, NewBacklogTask};
use anyhow::{anyhow, bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

pub fn edit_backlog_task(
    project_root: &Path,
    task_id: &str,
    patch: &BacklogTaskPatch,
) -> Result<String> {
    if patch.is_empty() {
        return Ok("no changes".to_string());
    }
    let mut args: Vec<OsString> = vec![
        "task".into(),
        "edit".into(),
        task_id.into(),
        "--plain".into(),
    ];
    if let Some(title) = &patch.title {
        args.push("-t".into());
        args.push(title.into());
    }
    if let Some(description) = &patch.description {
        args.push("-d".into());
        args.push(description.into());
    }
    if let Some(status) = &patch.status {
        args.push("-s".into());
        args.push(status.into());
    }
    if let Some(priority) = &patch.priority {
        args.push("--priority".into());
        args.push(priority.into());
    }
    if let Some(labels) = &patch.labels {
        args.push("-l".into());
        args.push(labels.join(",").into());
    }
    if let Some(assignees) = &patch.assignees {
        args.push("-a".into());
        args.push(assignees.join(",").into());
    }
    if let Some(dependencies) = &patch.dependencies {
        args.push("--depends-on".into());
        args.push(dependencies.join(",").into());
    }
    if let Some(references) = &patch.references {
        for reference in references {
            args.push("--ref".into());
            args.push(reference.into());
        }
    }
    if let Some(plan) = &patch.implementation_plan {
        args.push("--plan".into());
        args.push(plan.into());
    }
    if let Some(milestone) = &patch.milestone {
        args.push("-m".into());
        args.push(milestone.into());
    } else if patch.clear_milestone {
        args.push("--clear-milestone".into());
    }
    run_backlog(project_root, args)
}

pub fn set_backlog_dod_checked(
    project_root: &Path,
    task_id: &str,
    index: usize,
    checked: bool,
) -> Result<String> {
    let flag = if checked {
        "--check-dod"
    } else {
        "--uncheck-dod"
    };
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from(flag),
            OsString::from(index.to_string()),
        ],
    )
}

pub fn archive_backlog_task(project_root: &Path, task_id: &str) -> Result<String> {
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("archive"),
            OsString::from(task_id),
        ],
    )
}

/// Backlog.md's terminal disposition for a *Done* task (verified against a
/// real fixture repo: `backlog task complete --help` and the CLI's own
/// refusal of `task archive` on a Done task — "Done tasks should be
/// completed, not archived. Use: backlog task complete"). Moves the task
/// into `backlog/completed/`, reparsed as `BacklogTaskSource::Completed`,
/// not `Archived`. `archive_backlog_task` (above) is for a non-Done task's
/// abandonment and stays unchanged; the two are mutually exclusive
/// destinations for a task, chosen by status, not interchangeable options.
/// No `--plain` flag — same as `archive_backlog_task`, confirmed via
/// `backlog task complete --help`, which lists none.
pub fn complete_backlog_task(project_root: &Path, task_id: &str) -> Result<String> {
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("complete"),
            OsString::from(task_id),
        ],
    )
}

pub fn set_backlog_acceptance_checked(
    project_root: &Path,
    task_id: &str,
    index: usize,
    checked: bool,
) -> Result<String> {
    let flag = if checked {
        "--check-ac"
    } else {
        "--uncheck-ac"
    };
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from(flag),
            OsString::from(index.to_string()),
        ],
    )
}

/// Atomically swap one label for another via the CLI's own `--remove-label`/
/// `--add-label` flags (a single `task edit` invocation), rather than
/// round-tripping the full label list through `-l` — that would race with any
/// other label a human or another process added between our read and write.
/// This is the in-flight guard the dispatch queue depends on: a task moves
/// `dispatch` → `dispatching` before work starts, so a queue reload never
/// sees it as eligible twice.
pub fn swap_backlog_label(
    project_root: &Path,
    task_id: &str,
    from: &str,
    to: &str,
) -> Result<String> {
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from("--remove-label"),
            OsString::from(from),
            OsString::from("--add-label"),
            OsString::from(to),
        ],
    )
}

/// Add or remove one label without touching a task's other labels — the
/// per-task "Dispatch" opt-in affordance uses this to flag/unflag
/// `dispatch::DISPATCH_LABEL` rather than a full `BacklogTaskPatch::labels`
/// replace, which would race a concurrent edit of some other label (e.g. the
/// dispatch worker's own `swap_backlog_label` running at the same moment).
pub fn set_backlog_label(
    project_root: &Path,
    task_id: &str,
    label: &str,
    enabled: bool,
) -> Result<String> {
    let flag = if enabled {
        "--add-label"
    } else {
        "--remove-label"
    };
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from(flag),
            OsString::from(label),
        ],
    )
}

pub fn append_backlog_notes(project_root: &Path, task_id: &str, note: &str) -> Result<String> {
    if note.trim().is_empty() {
        bail!("note is empty");
    }
    run_backlog(
        project_root,
        [
            OsString::from("task"),
            OsString::from("edit"),
            OsString::from(task_id),
            OsString::from("--plain"),
            OsString::from("--append-notes"),
            OsString::from(note),
        ],
    )
}

pub fn create_backlog_task(project_root: &Path, task: &NewBacklogTask) -> Result<String> {
    if task.title.trim().is_empty() {
        bail!("title is required");
    }
    let mut args: Vec<OsString> = vec![
        "task".into(),
        "create".into(),
        task.title.clone().into(),
        "--plain".into(),
    ];
    if !task.description.trim().is_empty() {
        args.push("-d".into());
        args.push(task.description.clone().into());
    }
    if !task.status.trim().is_empty() {
        args.push("-s".into());
        args.push(task.status.clone().into());
    }
    if !task.priority.trim().is_empty() {
        args.push("--priority".into());
        args.push(task.priority.clone().into());
    }
    for criterion in &task.acceptance_criteria {
        if criterion.trim().is_empty() {
            continue;
        }
        args.push("--ac".into());
        args.push(criterion.clone().into());
    }
    if let Some(parent) = &task.parent {
        args.push("-p".into());
        args.push(parent.clone().into());
    }
    if !task.labels.is_empty() {
        args.push("-l".into());
        args.push(task.labels.join(",").into());
    }
    if !task.assignees.is_empty() {
        args.push("-a".into());
        args.push(task.assignees.join(",").into());
    }
    if let Some(milestone) = &task.milestone {
        args.push("-m".into());
        args.push(milestone.clone().into());
    }
    if !task.dependencies.is_empty() {
        args.push("--depends-on".into());
        args.push(task.dependencies.join(",").into());
    }
    run_backlog(project_root, args)
}

fn run_backlog<I, S>(project_root: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let cli = backlog_cli_path().ok_or_else(|| {
        anyhow!(
            "Backlog CLI not found. Install backlog or make it visible on PATH before editing tasks."
        )
    })?;
    let output = Command::new(&cli)
        .current_dir(project_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", cli.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if stderr.is_empty() { stdout } else { stderr };
        bail!("backlog failed: {msg}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
