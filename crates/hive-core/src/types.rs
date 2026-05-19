use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LocalListener {
    pub pid: u32,
    pub pgid: i32,
    pub port: u16,
    pub command_name: String,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
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
