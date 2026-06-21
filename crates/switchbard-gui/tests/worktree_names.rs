use std::path::PathBuf;

use switchbard_core::{config::Config, Repo, WorktreeAlias, WorktreeRef};
use switchbard_gui::runtime::worktree_names::{
    default_worktree_location, remove_worktree_alias, unique_worktree_name_error,
    worktree_display_name,
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

#[test]
fn remove_worktree_alias_prunes_matching_entry_and_leaves_others() {
    let repo = repo();
    let kept_path = PathBuf::from("/Users/me/Dev/.worktrees/switchbard/servers");
    let removed_path = PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents");
    let mut cfg = Config {
        version: 1,
        repos: vec![repo.clone()],
        worktrees: vec![
            WorktreeAlias {
                repo_path: repo.path.clone(),
                worktree_path: removed_path.clone(),
                name: "agents".into(),
            },
            WorktreeAlias {
                repo_path: repo.path.clone(),
                worktree_path: kept_path.clone(),
                name: "servers".into(),
            },
        ],
        ui: Default::default(),
    };

    remove_worktree_alias(&mut cfg, &repo.path, &removed_path);

    assert!(
        cfg.worktrees
            .iter()
            .all(|a| a.worktree_path != removed_path),
        "removed path must not remain in config.worktrees"
    );
    assert!(
        cfg.worktrees.iter().any(|a| a.worktree_path == kept_path),
        "unrelated alias must be preserved"
    );
}

fn repo() -> Repo {
    Repo {
        name: "switchbard".into(),
        path: PathBuf::from("/Users/me/Dev/switchbard"),
    }
}
