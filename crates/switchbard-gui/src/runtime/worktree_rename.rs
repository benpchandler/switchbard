use std::path::PathBuf;

use switchbard_core::{config::Config, Repo, WorktreeRef};

use crate::runtime::worktree_names::worktree_name_conflict_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameWorktreeDialog {
    pub repo: Repo,
    pub worktree_path: PathBuf,
    pub original_name: String,
    pub name: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameWorktreeValidationError {
    EmptyName,
    DuplicateName,
}

impl RenameWorktreeDialog {
    pub fn new(repo: Repo, worktree_path: PathBuf, name: String) -> Self {
        Self {
            repo,
            worktree_path,
            original_name: name.clone(),
            name,
            error: None,
        }
    }

    pub fn validate(&self, config: &Config) -> Result<(), RenameWorktreeValidationError> {
        self.validate_with_worktrees(config, &[])
    }

    pub fn validate_with_worktrees(
        &self,
        config: &Config,
        worktrees: &[WorktreeRef],
    ) -> Result<(), RenameWorktreeValidationError> {
        if self.name.trim().is_empty() {
            return Err(RenameWorktreeValidationError::EmptyName);
        }
        if worktree_name_conflict_error(
            config,
            &self.repo,
            worktrees,
            &self.name,
            Some(&self.worktree_path),
        )
        .is_some()
        {
            return Err(RenameWorktreeValidationError::DuplicateName);
        }
        Ok(())
    }
}

impl RenameWorktreeValidationError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::EmptyName => "Name cannot be empty.",
            Self::DuplicateName => "Name is already used in this repo.",
        }
    }
}
