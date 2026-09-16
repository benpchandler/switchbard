//! Stable local repository scope shared by all linked worktrees.
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub fn repository_key(root: &Path) -> Result<String> {
    let root = root.canonicalize().context("resolve bug repository")?;
    ensure!(root.is_dir(), "bug repository is not a directory");
    let output = crate::git_cmd()
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()?;
    ensure!(
        output.status.success(),
        "bug dispatch requires a Git repository"
    );
    let common = PathBuf::from(String::from_utf8(output.stdout)?.trim()).canonicalize()?;
    let path = common
        .to_str()
        .context("bug repository path is not UTF-8")?;
    Ok(format!("git:{path}"))
}
