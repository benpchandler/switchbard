use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LocalListener {
    pub pid: u32,
    pub pgid: i32,
    pub port: u16,
    pub command_name: String,
    pub cwd: Option<PathBuf>,
}

/// A configured top-level repository directory. Used as both the runtime
/// model and the persisted form in `~/.switchbard/config.toml` — there is no
/// separate "entry" type, just this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
}

/// User-assigned display name for one worktree. Git remains authoritative for
/// branch/path/head; this is Switchbard-local metadata keyed by paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeAlias {
    pub repo_path: PathBuf,
    pub worktree_path: PathBuf,
    pub name: String,
}

/// A single checkout used both for listener attribution (cwd-prefix match) and
/// for the dedicated Worktrees view. Carries the immutable `git worktree list`
/// info; mutable git-status fields (dirty, ahead/behind, last-commit age) live
/// separately in a probe-populated metadata map keyed by `path` so attribution
/// scans don't contend on the probe thread.
#[derive(Debug, Clone)]
pub struct WorktreeRef {
    pub repo_name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: String,
}

#[derive(Debug, Clone)]
pub struct AttributedListener {
    pub listener: LocalListener,
    pub repo_name: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub worktree_branch: Option<String>,
}
