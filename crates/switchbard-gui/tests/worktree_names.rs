use std::path::PathBuf;

use switchbard_core::{config::Config, Repo, WorktreeAlias, WorktreeRef};
use switchbard_gui::runtime::worktree_names::{
    default_worktree_location, unique_worktree_name_error, worktree_display_name,
};

#[test]
fn persisted_worktree_name_wins_over_folder_leaf() {
    let repo = repo();
    let wt = WorktreeRef {
        repo_name: repo.name.clone(),
        path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/slot-a"),
        branch: Some("codex/perf-ledger".into()),
        head: "abcdef".into(),
    };
    let cfg = Config {
        version: 1,
        repos: vec![repo.clone()],
        worktrees: vec![WorktreeAlias {
            repo_path: repo.path.clone(),
            worktree_path: wt.path.clone(),
            name: "agents".into(),
        }],
        ui: Default::default(),
    };

    assert_eq!(worktree_display_name(&cfg, &repo, &wt), "agents");
}

#[test]
fn missing_worktree_name_falls_back_to_folder_leaf() {
    let repo = repo();
    let wt = WorktreeRef {
        repo_name: repo.name.clone(),
        path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/servers"),
        branch: Some("main".into()),
        head: "abcdef".into(),
    };

    assert_eq!(
        worktree_display_name(&Config::default(), &repo, &wt),
        "servers"
    );
}

#[test]
fn duplicate_worktree_names_are_rejected_per_repo() {
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

    let err = unique_worktree_name_error(&cfg, &repo, "agents", None).unwrap();

    assert!(err.contains("already used"));
}

#[test]
fn default_location_uses_repo_scoped_worktrees_directory() {
    let repo = repo();

    assert_eq!(
        default_worktree_location(&repo, "agents"),
        PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents")
    );
}

fn repo() -> Repo {
    Repo {
        name: "switchbard".into(),
        path: PathBuf::from("/Users/me/Dev/switchbard"),
    }
}
