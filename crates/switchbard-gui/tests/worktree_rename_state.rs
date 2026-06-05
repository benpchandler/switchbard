use std::path::PathBuf;

use switchbard_core::{config::Config, Repo, WorktreeAlias, WorktreeRef};
use switchbard_gui::runtime::worktree_rename::{
    RenameWorktreeDialog, RenameWorktreeValidationError,
};

#[test]
fn rename_validation_allows_current_name_for_same_worktree() {
    let repo = repo();
    let path = PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents");
    let cfg = Config {
        version: 1,
        repos: vec![repo.clone()],
        worktrees: vec![WorktreeAlias {
            repo_path: repo.path.clone(),
            worktree_path: path.clone(),
            name: "agents".into(),
        }],
        ui: Default::default(),
    };
    let dialog = RenameWorktreeDialog::new(repo, path, "agents".into());

    assert!(dialog.validate(&cfg).is_ok());
}

#[test]
fn rename_validation_rejects_duplicate_name_on_other_worktree() {
    let repo = repo();
    let cfg = Config {
        version: 1,
        repos: vec![repo.clone()],
        worktrees: vec![WorktreeAlias {
            repo_path: repo.path.clone(),
            worktree_path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents"),
            name: "agents".into(),
        }],
        ui: Default::default(),
    };
    let dialog = RenameWorktreeDialog::new(
        repo,
        PathBuf::from("/Users/me/Dev/.worktrees/switchbard/review"),
        "agents".into(),
    );

    assert_eq!(
        dialog.validate(&cfg).unwrap_err(),
        RenameWorktreeValidationError::DuplicateName
    );
}

#[test]
fn rename_validation_rejects_duplicate_inferred_name_on_other_worktree() {
    let repo = repo();
    let dialog = RenameWorktreeDialog::new(
        repo.clone(),
        PathBuf::from("/Users/me/Dev/.worktrees/switchbard/review"),
        "agents".into(),
    );
    let worktrees = vec![WorktreeRef {
        repo_name: repo.name.clone(),
        path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents"),
        branch: Some("work/agents".into()),
        head: "abcdef".into(),
    }];

    assert_eq!(
        dialog
            .validate_with_worktrees(&Config::default(), &worktrees)
            .unwrap_err(),
        RenameWorktreeValidationError::DuplicateName
    );
}

fn repo() -> Repo {
    Repo {
        name: "switchbard".into(),
        path: PathBuf::from("/Users/me/Dev/switchbard"),
    }
}
