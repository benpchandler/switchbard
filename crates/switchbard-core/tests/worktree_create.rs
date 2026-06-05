use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use switchbard_core::{create_worktree, CreateBranchMode, CreateWorktreeOptions};
use tempfile::TempDir;

fn setup_repo() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    run(&repo, &["init", "-q", "-b", "main"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    run(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "init"]);
    (tmp, repo)
}

fn run(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {:?} failed in {:?}", args, cwd);
}

#[test]
fn creates_named_branch_worktree_from_base() {
    let (tmp, repo) = setup_repo();
    let worktree = tmp.path().join("worktrees/agents");

    create_worktree(CreateWorktreeOptions {
        repo_path: repo.clone(),
        worktree_path: worktree.clone(),
        branch_mode: CreateBranchMode::NewBranch {
            branch: "work/agents".into(),
            base: "main".into(),
        },
    })
    .unwrap();

    assert!(worktree.join("README.md").exists());
    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["branch", "--show-current"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "work/agents"
    );
}

#[test]
fn rejects_occupied_location_before_calling_git() {
    let (tmp, repo) = setup_repo();
    let worktree = tmp.path().join("occupied");
    fs::create_dir(&worktree).unwrap();

    let err = create_worktree(CreateWorktreeOptions {
        repo_path: repo,
        worktree_path: worktree,
        branch_mode: CreateBranchMode::NewBranch {
            branch: "work/agents".into(),
            base: "main".into(),
        },
    })
    .unwrap_err();

    assert!(err.to_string().contains("location already exists"));
}

#[test]
fn checks_out_existing_branch_when_available() {
    let (tmp, repo) = setup_repo();
    run(&repo, &["branch", "review"]);
    let worktree = tmp.path().join("worktrees/review");

    create_worktree(CreateWorktreeOptions {
        repo_path: repo,
        worktree_path: worktree.clone(),
        branch_mode: CreateBranchMode::ExistingBranch {
            branch: "review".into(),
        },
    })
    .unwrap();

    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["branch", "--show-current"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "review");
}
