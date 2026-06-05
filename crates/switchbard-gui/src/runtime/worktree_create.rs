use std::path::PathBuf;

use switchbard_core::{config::Config, CreateBranchMode, CreateWorktreeOptions, Repo, WorktreeRef};

use crate::runtime::worktree_names::{
    default_worktree_location, slug_for_worktree_name, worktree_name_conflict_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateCheckoutMode {
    NewBranch,
    ExistingBranch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorktreeDialog {
    pub repo: Repo,
    pub name: String,
    pub worktree_path: String,
    pub checkout_mode: CreateCheckoutMode,
    pub branch: String,
    pub base: String,
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWorktree {
    pub repo: Repo,
    pub worktree_path: PathBuf,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateWorktreeOutcome {
    Created(CreatedWorktree),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateWorktreeValidationError {
    EmptyName,
    DuplicateName,
    EmptyLocation,
    LocationExists,
    EmptyBranch,
    EmptyBase,
    BranchAlreadyCheckedOut,
}

impl CreateWorktreeDialog {
    pub fn new(repo: Repo, worktrees: &[WorktreeRef]) -> Self {
        Self::new_with_config(repo, &Config::default(), worktrees)
    }

    pub fn new_with_config(repo: Repo, config: &Config, worktrees: &[WorktreeRef]) -> Self {
        let name = first_available_name(config, &repo, worktrees);
        let branch = default_branch_for_name(&name);
        let base = primary_branch_for_repo(&repo, worktrees).unwrap_or_else(|| "main".to_string());
        let worktree_path = default_worktree_location(&repo, &name)
            .display()
            .to_string();
        Self {
            repo,
            name,
            worktree_path,
            checkout_mode: CreateCheckoutMode::NewBranch,
            branch,
            base,
            busy: false,
            error: None,
        }
    }

    pub fn validate(
        &self,
        config: &Config,
        worktrees: &[WorktreeRef],
    ) -> Result<CreateWorktreeOptions, CreateWorktreeValidationError> {
        if self.name.trim().is_empty() {
            return Err(CreateWorktreeValidationError::EmptyName);
        }
        if worktree_name_conflict_error(config, &self.repo, worktrees, &self.name, None).is_some() {
            return Err(CreateWorktreeValidationError::DuplicateName);
        }
        let worktree_path = PathBuf::from(self.worktree_path.trim());
        if self.worktree_path.trim().is_empty() {
            return Err(CreateWorktreeValidationError::EmptyLocation);
        }
        if worktree_path.exists() {
            return Err(CreateWorktreeValidationError::LocationExists);
        }
        if self.branch.trim().is_empty() {
            return Err(CreateWorktreeValidationError::EmptyBranch);
        }
        if self.checkout_mode == CreateCheckoutMode::NewBranch && self.base.trim().is_empty() {
            return Err(CreateWorktreeValidationError::EmptyBase);
        }
        if branch_checked_out(&self.repo, worktrees, self.branch.trim()) {
            return Err(CreateWorktreeValidationError::BranchAlreadyCheckedOut);
        }

        let branch_mode = match self.checkout_mode {
            CreateCheckoutMode::NewBranch => CreateBranchMode::NewBranch {
                branch: self.branch.trim().to_string(),
                base: self.base.trim().to_string(),
            },
            CreateCheckoutMode::ExistingBranch => CreateBranchMode::ExistingBranch {
                branch: self.branch.trim().to_string(),
            },
        };
        Ok(CreateWorktreeOptions {
            repo_path: self.repo.path.clone(),
            worktree_path,
            branch_mode,
        })
    }

    pub fn sync_defaults_after_name_edit(&mut self, old_name: &str) {
        let old_path = default_worktree_location(&self.repo, old_name)
            .display()
            .to_string();
        if self.worktree_path == old_path {
            self.worktree_path = default_worktree_location(&self.repo, &self.name)
                .display()
                .to_string();
        }

        let old_branch = default_branch_for_name(old_name);
        if self.branch == old_branch {
            self.branch = default_branch_for_name(&self.name);
        }
    }

    pub fn command_preview(&self) -> String {
        match self.checkout_mode {
            CreateCheckoutMode::NewBranch => format!(
                "git -C {} worktree add -b {} {} {}",
                self.repo.path.display(),
                self.branch.trim(),
                self.worktree_path.trim(),
                self.base.trim()
            ),
            CreateCheckoutMode::ExistingBranch => format!(
                "git -C {} worktree add {} {}",
                self.repo.path.display(),
                self.worktree_path.trim(),
                self.branch.trim()
            ),
        }
    }
}

impl CreateWorktreeValidationError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::EmptyName => "Name cannot be empty.",
            Self::DuplicateName => "Name is already used in this repo.",
            Self::EmptyLocation => "Location cannot be empty.",
            Self::LocationExists => "Location already exists.",
            Self::EmptyBranch => "Branch cannot be empty.",
            Self::EmptyBase => "Base cannot be empty.",
            Self::BranchAlreadyCheckedOut => "Branch is already checked out in another worktree.",
        }
    }
}

fn first_available_name(config: &Config, repo: &Repo, worktrees: &[WorktreeRef]) -> String {
    for name in ["agents", "servers", "review", "scratch"] {
        if worktree_name_conflict_error(config, repo, worktrees, name, None).is_none() {
            return name.to_string();
        }
    }
    let mut i = 2usize;
    loop {
        let name = format!("worktree-{i}");
        if worktree_name_conflict_error(config, repo, worktrees, &name, None).is_none() {
            return name;
        }
        i += 1;
    }
}

fn default_branch_for_name(name: &str) -> String {
    format!("work/{}", slug_for_worktree_name(name))
}

fn primary_branch_for_repo(repo: &Repo, worktrees: &[WorktreeRef]) -> Option<String> {
    worktrees
        .iter()
        .find(|wt| wt.repo_name == repo.name && wt.path == repo.path)
        .and_then(|wt| wt.branch.clone())
}

fn branch_checked_out(repo: &Repo, worktrees: &[WorktreeRef], branch: &str) -> bool {
    worktrees
        .iter()
        .any(|wt| wt.repo_name == repo.name && wt.branch.as_deref() == Some(branch))
}
