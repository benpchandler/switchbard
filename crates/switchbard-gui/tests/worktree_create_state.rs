use std::path::PathBuf;

use switchbard_core::{config::Config, Repo, WorktreeAlias, WorktreeRef};
use switchbard_gui::runtime::worktree_create::{
    CreateWorktreeDialog, CreateWorktreeValidationError,
};

#[test]
fn dialog_defaults_location_branch_and_base_from_repo_context() {
    let repo = repo();
    let worktrees = vec![WorktreeRef {
        repo_name: repo.name.clone(),
        path: repo.path.clone(),
        branch: Some("main".into()),
        head: "abcdef".into(),
    }];

    let dialog = CreateWorktreeDialog::new(repo, &worktrees);

    assert_eq!(dialog.name, "agents");
    assert_eq!(
        dialog.worktree_path,
        PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents")
            .display()
            .to_string()
    );
    assert_eq!(dialog.branch, "work/agents");
    assert_eq!(dialog.base, "main");
}

#[test]
fn validation_rejects_duplicate_names_within_repo() {
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
    let dialog = CreateWorktreeDialog::new(repo, &[]);

    let err = dialog.validate(&cfg, &[]).unwrap_err();

    assert_eq!(err, CreateWorktreeValidationError::DuplicateName);
}

#[test]
fn validation_rejects_duplicate_inferred_worktree_names() {
    let repo = repo();
    let dialog = CreateWorktreeDialog::new(repo.clone(), &[]);
    let worktrees = vec![WorktreeRef {
        repo_name: repo.name.clone(),
        path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents"),
        branch: Some("other/branch".into()),
        head: "abcdef".into(),
    }];

    let err = dialog.validate(&Config::default(), &worktrees).unwrap_err();

    assert_eq!(err, CreateWorktreeValidationError::DuplicateName);
}

#[test]
fn dialog_default_name_skips_existing_inferred_worktree_names() {
    let repo = repo();
    let worktrees = vec![WorktreeRef {
        repo_name: repo.name.clone(),
        path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents"),
        branch: Some("work/agents".into()),
        head: "abcdef".into(),
    }];

    let dialog = CreateWorktreeDialog::new(repo, &worktrees);

    assert_eq!(dialog.name, "servers");
}

#[test]
fn validation_rejects_branch_checked_out_in_another_worktree() {
    let repo = repo();
    let mut dialog = CreateWorktreeDialog::new(repo.clone(), &[]);
    dialog.branch = "codex/perf-ledger".into();
    let worktrees = vec![WorktreeRef {
        repo_name: repo.name.clone(),
        path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/review"),
        branch: Some("codex/perf-ledger".into()),
        head: "abcdef".into(),
    }];

    let err = dialog.validate(&Config::default(), &worktrees).unwrap_err();

    assert_eq!(err, CreateWorktreeValidationError::BranchAlreadyCheckedOut);
}

#[test]
fn changing_name_updates_untouched_path_and_branch_defaults() {
    let repo = repo();
    let mut dialog = CreateWorktreeDialog::new(repo, &[]);
    let old_name = dialog.name.clone();
    dialog.name = "review".into();

    dialog.sync_defaults_after_name_edit(&old_name);

    assert_eq!(
        dialog.worktree_path,
        PathBuf::from("/Users/me/Dev/.worktrees/switchbard/review")
            .display()
            .to_string()
    );
    assert_eq!(dialog.branch, "work/review");
}

fn repo() -> Repo {
    Repo {
        name: "switchbard".into(),
        path: PathBuf::from("/Users/me/Dev/switchbard"),
    }
}
