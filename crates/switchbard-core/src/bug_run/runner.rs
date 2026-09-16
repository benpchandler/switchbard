//! One durable run per detached supervisor, with explicit human handoff boundaries.
use super::{codex, process, AgentOutcome, Lease, Run, RunAction, RunState, Store};
use anyhow::{ensure, Context, Result};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Execute only this run. Concurrent supervisors cannot acquire the same durable lease.
pub fn run_supervisor(database: &Path, id: &str) -> Result<()> {
    let mut store = Store::open(database)?;
    if !store.get(id)?.state.is_queued() {
        return Ok(());
    }
    let lease = match store.claim(id) {
        Ok(lease) => lease,
        Err(_) if !store.get(id)?.state.is_queued() => return Ok(()),
        Err(error) => return Err(error),
    };
    let result = super::task_flow::project(&lease.run).and_then(|()| {
        if lease.action == RunAction::Publish {
            super::publish::publish(&mut store, &lease)
        } else {
            run_agent(&mut store, &lease, database)
        }
    });
    if let Err(error) = result {
        let current = store.get(id)?;
        if matches!(current.state, RunState::Running | RunState::Publishing) {
            store.fail(
                &lease,
                &format!("{error:#}"),
                lease.action == RunAction::Publish
                    || error.downcast_ref::<process::UnreapedProcess>().is_some(),
            )?;
        }
        super::task_flow::project(&store.get(id)?)?;
        return Err(error);
    }
    super::task_flow::project(&store.get(id)?)?;
    Ok(())
}

fn run_agent(store: &mut Store, lease: &Lease, database: &Path) -> Result<()> {
    let mut version = Command::new(&lease.run.options.codex_binary);
    version.arg("--version");
    process::capture(version, Duration::from_secs(15))
        .context("Codex is unavailable; install or configure Codex, then retry")?;
    let artifacts = artifact_dir(database, &lease.run.id)?;
    let run = prepare_worktree(store, lease, &artifacts)?;
    let outcome = codex::execute(store, lease, &run, &artifacts);
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            // No thread means a crash can conceal an already-created session. Never launch another blindly.
            let unknown = store.get(&run.id)?.thread_id.is_none()
                || error.downcast_ref::<process::UnreapedProcess>().is_some();
            store.fail(lease, &format!("{error:#}"), unknown)?;
            return Err(error);
        }
    };
    let outcome = prepare_review(store, lease, &run, outcome)?;
    if matches!(outcome, AgentOutcome::AwaitingReview { .. }) {
        super::publish::capture_review_target(store, lease)?;
    }
    store.complete_agent(lease, outcome)?;
    Ok(())
}

fn artifact_dir(database: &Path, id: &str) -> Result<PathBuf> {
    let dir = database
        .parent()
        .context("run store has no parent directory")?
        .join("bug-run-artifacts")
        .join(id);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    for path in [
        dir.parent().context("artifact parent missing")?,
        dir.as_path(),
    ] {
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "artifact directory is not a real directory"
        );
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "artifact directory permissions must be private (0700)"
        );
    }
    Ok(dir)
}

fn prepare_worktree(store: &mut Store, lease: &Lease, artifacts: &Path) -> Result<Run> {
    if let Some(worktree) = &lease.run.worktree {
        ensure!(
            super::repository_key(worktree)? == lease.run.repo_key,
            "retained execution worktree no longer belongs to the task repository"
        );
        return Ok(lease.run.clone());
    }
    let root = &lease.run.repo_root;
    git_leased(root, &["fetch", "origin", "main"], store, lease)?;
    let worktree = artifacts.join("worktree");
    let branch = format!(
        "bug/{}-{}",
        lease.run.task_id.to_ascii_lowercase(),
        lease.run.id
    );
    if !worktree.exists() {
        git_leased(
            root,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                worktree.to_str().context("non-UTF8 worktree path")?,
                "origin/main",
            ],
            store,
            lease,
        )?;
    }
    let current = git(&worktree, &["branch", "--show-current"])?;
    ensure!(
        current.trim() == branch,
        "retained path has a different branch; inspect it before retry"
    );
    store.record_workspace(lease, &worktree, &branch)
}

fn prepare_review(
    store: &mut Store,
    lease: &Lease,
    run: &Run,
    outcome: AgentOutcome,
) -> Result<AgentOutcome> {
    let AgentOutcome::AwaitingReview {
        summary,
        mut tests,
        risks,
    } = outcome
    else {
        return Ok(outcome);
    };
    let worktree = run
        .worktree
        .as_ref()
        .context("run has no execution worktree")?;
    validate_agent_writes(run, worktree)?;
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", &run.options.gate_command])
        .current_dir(worktree);
    process::capture_leased(
        command,
        Duration::from_secs(3600),
        store,
        lease,
        "pre-review-validation",
    )
    .context("configured validation gate failed before review")?;
    tests.push(format!(
        "Supervisor verified: {} (exit 0)",
        run.options.gate_command
    ));
    validate_agent_writes(run, worktree)?;
    if !git(worktree, &["status", "--porcelain"])?.trim().is_empty() {
        git_leased(worktree, &["add", "--all"], store, lease)?;
        git_leased(
            worktree,
            &[
                "commit",
                "-m",
                &format!("fix: {} ({})", run.title, run.task_id),
            ],
            store,
            lease,
        )?;
    }
    let head = git(worktree, &["rev-parse", "HEAD"])?;
    store.record_review_head(lease, head.trim())?;
    Ok(AgentOutcome::AwaitingReview {
        summary,
        tests,
        risks,
    })
}

fn validate_agent_writes(run: &Run, worktree: &Path) -> Result<()> {
    ensure!(
        git(worktree, &["branch", "--show-current"])?.trim()
            == run.branch.as_deref().context("run branch missing")?,
        "agent changed execution branch"
    );
    ensure!(
        git(
            worktree,
            &[
                "status",
                "--porcelain",
                "--",
                "backlog",
                ".switchbard/tasks.json"
            ]
        )?
        .trim()
        .is_empty(),
        "agent modified task authority; inspect retained changes before continuing"
    );
    ensure!(
        git(
            worktree,
            &[
                "diff",
                "--name-only",
                "origin/main",
                "HEAD",
                "--",
                "backlog",
                ".switchbard/tasks.json"
            ]
        )?
        .trim()
        .is_empty(),
        "agent committed task authority changes; inspect retained commits before continuing"
    );
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut command = crate::git_cmd();
    command.arg("-C").arg(root).args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    process::capture(command, Duration::from_secs(120))
}

fn git_leased(root: &Path, args: &[&str], store: &mut Store, lease: &Lease) -> Result<String> {
    let mut command = crate::git_cmd();
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    process::capture_leased(
        command,
        Duration::from_secs(120),
        store,
        lease,
        args.first().copied().unwrap_or("git"),
    )
}
