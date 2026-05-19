use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LocalListener {
    pub pid: u32,
    pub pgid: i32,
    pub port: u16,
    pub command_name: String,
    pub cwd: Option<PathBuf>,
}

/// A canonical project name + its primary checkout path. One Repo can expand to
/// many WorktreeRefs via `worktree::enumerate_worktrees`.
#[derive(Debug, Clone)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
}

/// An individual checkout — either the primary path of a Repo or one of its
/// `git worktree add` siblings. Attribution matches a listener's cwd against
/// these paths (prefix match).
#[derive(Debug, Clone)]
pub struct WorktreeRef {
    pub repo_name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AttributedListener {
    pub listener: LocalListener,
    pub repo_name: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub worktree_branch: Option<String>,
}
