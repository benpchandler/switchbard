//! Worktree creation — the non-destructive sibling to `worktree_remove`.
//!
//! Switchbard owns the UX around names and default locations, but Git remains
//! authoritative for the actual checkout. This module validates obvious local
//! file-system preconditions and then delegates to `git worktree add`.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateBranchMode {
    NewBranch { branch: String, base: String },
    ExistingBranch { branch: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorktreeOptions {
    pub repo_path: PathBuf,
    pub worktree_path: PathBuf,
    pub branch_mode: CreateBranchMode,
}

pub fn create_worktree(opts: CreateWorktreeOptions) -> Result<()> {
    validate_worktree_location(&opts.worktree_path)?;
    if let Some(parent) = opts.worktree_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&opts.repo_path).args(["worktree", "add"]);
    match &opts.branch_mode {
        CreateBranchMode::NewBranch { branch, base } => {
            validate_refish("branch", branch)?;
            validate_refish("base", base)?;
            cmd.arg("-b").arg(branch).arg(&opts.worktree_path).arg(base);
        }
        CreateBranchMode::ExistingBranch { branch } => {
            validate_refish("branch", branch)?;
            cmd.arg(&opts.worktree_path).arg(branch);
        }
    }

    let output = cmd.output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Err(anyhow!(
        "git worktree add failed: {}",
        stderr.trim().replace('\n', "; ")
    ))
}

fn validate_worktree_location(path: &Path) -> Result<()> {
    if path.exists() {
        return Err(anyhow!("location already exists: {}", path.display()));
    }
    Ok(())
}

fn validate_refish(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(anyhow!("{label} cannot be empty"));
    }
    Ok(())
}
