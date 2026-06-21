/// Tests for the multi-step worktree removal orchestration helpers.
///
/// `state_drifted` and `runs_drifted` are unit-tested directly — they are
/// pure functions with no I/O.  `delete_branch_after_removal` requires a real
/// git repo to exercise the success and failure paths; those tests use a temp
/// directory following the `setup_repo_with_worktree` style from
/// `switchbard-core/src/worktree_remove.rs`.
///
/// `execute_remove_worktree` itself spawns a worker thread which makes
/// end-to-end orchestration testing impractical from the outside; the helpers
/// extracted here cover the decision logic that matters.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use switchbard_core::{BranchDeleteAssessment, DirtyFile};
use switchbard_gui::app::{delete_branch_after_removal, runs_drifted, state_drifted};
use switchbard_gui::runtime::{ActiveRunSummary, ConfirmRemoveWorktree};
use tempfile::TempDir;

// ─── state_drifted ──────────────────────────────────────────────────────────

fn dirty(status: &str, path: &str) -> DirtyFile {
    DirtyFile {
        status: status.to_string(),
        path: PathBuf::from(path),
    }
}

#[test]
fn state_drifted_equal_sets_returns_false() {
    let a = vec![dirty(" M", "src/main.rs"), dirty("??", "scratch.txt")];
    let b = vec![dirty("??", "scratch.txt"), dirty(" M", "src/main.rs")]; // different order
    assert!(!state_drifted(&a, &b));
}

#[test]
fn state_drifted_added_file_returns_true() {
    let original = vec![dirty(" M", "src/main.rs")];
    let fresh = vec![dirty(" M", "src/main.rs"), dirty("??", "new.txt")];
    assert!(state_drifted(&original, &fresh));
}

#[test]
fn state_drifted_removed_file_returns_true() {
    let original = vec![dirty(" M", "src/main.rs"), dirty("??", "old.txt")];
    let fresh = vec![dirty(" M", "src/main.rs")];
    assert!(state_drifted(&original, &fresh));
}

#[test]
fn state_drifted_status_flip_returns_true() {
    // Same path, different status (e.g. `??` untracked → `A ` staged add)
    let original = vec![dirty("??", "src/new.rs")];
    let fresh = vec![dirty("A ", "src/new.rs")];
    assert!(state_drifted(&original, &fresh));
}

#[test]
fn state_drifted_empty_sets_returns_false() {
    assert!(!state_drifted(&[], &[]));
}

// ─── runs_drifted ───────────────────────────────────────────────────────────

fn run_summary(pgid: i32) -> ActiveRunSummary {
    ActiveRunSummary {
        service_name: format!("svc-{pgid}"),
        pgid,
    }
}

#[test]
fn runs_drifted_equal_sets_returns_false() {
    let a = vec![run_summary(100), run_summary(200)];
    let b = vec![run_summary(200), run_summary(100)]; // different order
    assert!(!runs_drifted(&a, &b));
}

#[test]
fn runs_drifted_added_run_returns_true() {
    let original = vec![run_summary(100)];
    let fresh = vec![run_summary(100), run_summary(200)];
    assert!(runs_drifted(&original, &fresh));
}

#[test]
fn runs_drifted_removed_run_returns_true() {
    let original = vec![run_summary(100), run_summary(200)];
    let fresh = vec![run_summary(100)];
    assert!(runs_drifted(&original, &fresh));
}

#[test]
fn runs_drifted_empty_sets_returns_false() {
    assert!(!runs_drifted(&[], &[]));
}

// ─── delete_branch_after_removal — git-backed ────────────────────────────────

/// Minimal snapshot for delete_branch_after_removal tests.  We only set the
/// fields the function inspects: `repo_path`, `delete_branch`, and
/// `branch_assessment`.
fn snapshot_for_branch_delete(
    repo_path: PathBuf,
    delete_branch: bool,
    assessment: Option<BranchDeleteAssessment>,
) -> ConfirmRemoveWorktree {
    ConfirmRemoveWorktree {
        repo_path,
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: Some("feat/foo".to_string()),
        dirty_files: vec![],
        active_runs: vec![],
        branch_assessment: assessment,
        delete_branch,
        busy: false,
        error: None,
    }
}

/// Set up a real git repo + a linked worktree on a feature branch.
/// Returns (TempDir, repo_path, worktree_path).
fn setup_repo_with_worktree() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();

    run_git(&repo, &["init", "-q", "-b", "main"]);
    run_git(&repo, &["config", "user.email", "test@example.com"]);
    run_git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-qm", "init"]);

    let wt = tmp.path().join("wt-feat");
    run_git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/foo",
            wt.to_str().unwrap(),
        ],
    );

    (tmp, repo, wt)
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

#[test]
fn delete_branch_after_removal_not_requested_returns_empty() {
    // delete_branch=false → will_delete_branch() is false regardless of assessment
    let snapshot = snapshot_for_branch_delete(PathBuf::from("/tmp/repo"), false, None);
    let note = delete_branch_after_removal(&snapshot, Some("feat/foo"));
    assert!(note.is_empty());
}

#[test]
fn delete_branch_after_removal_success_returns_note() {
    let (_tmp, repo, wt) = setup_repo_with_worktree();

    // Remove the worktree first so git allows branch deletion
    run_git(&repo, &["worktree", "remove", wt.to_str().unwrap()]);

    // Assessment: landed (0 unmerged), not blocked
    let assessment = BranchDeleteAssessment {
        branch: "feat/foo".to_string(),
        other_checkouts: vec![],
        unmerged_commits: Some(0),
        compared_against: Some("main".to_string()),
    };
    let snapshot = snapshot_for_branch_delete(repo, true, Some(assessment));
    let note = delete_branch_after_removal(&snapshot, Some("feat/foo"));
    assert!(
        note.contains("deleted branch 'feat/foo'"),
        "expected success note, got: {note:?}"
    );
}

#[test]
fn delete_branch_after_removal_failure_is_non_fatal() {
    let (_tmp, repo, _wt) = setup_repo_with_worktree();

    // The worktree is still checked out on feat/foo, so git branch -d will fail.
    // Assessment says landed so no force; but git will reject it.
    let assessment = BranchDeleteAssessment {
        branch: "feat/foo".to_string(),
        other_checkouts: vec![],
        unmerged_commits: Some(0),
        compared_against: Some("main".to_string()),
    };
    let snapshot = snapshot_for_branch_delete(repo, true, Some(assessment));
    // Returns a non-empty error note rather than panicking.
    let note = delete_branch_after_removal(&snapshot, Some("feat/foo"));
    assert!(
        note.contains("NOT deleted"),
        "expected failure note, got: {note:?}"
    );
}

// ─── integration: remove_worktree success + drift-abort ──────────────────────

/// These tests drive the core-level `remove_worktree` + `collect_dirty_files`
/// calls that `execute_remove_worktree` delegates to, proving the lower-level
/// pieces work against a real git repo.  The thread-spawning wrapper in
/// `execute_remove_worktree` is not driven here (impractical without a headless
/// egui context); we test the extracted decision functions instead.

#[test]
fn remove_worktree_success_and_state_is_clean_after() {
    use switchbard_core::{collect_dirty_files, remove_worktree};

    let (_tmp, repo, wt) = setup_repo_with_worktree();

    let dirty_before = collect_dirty_files(&wt).unwrap();
    assert!(dirty_before.is_empty(), "worktree should start clean");

    remove_worktree(&repo, &wt, false).expect("clean removal should succeed");
    assert!(!wt.exists(), "worktree path should be gone after removal");
}

#[test]
fn state_drifted_catches_new_file_written_to_worktree() {
    use switchbard_core::collect_dirty_files;

    let (_tmp, _repo, wt) = setup_repo_with_worktree();

    let original = collect_dirty_files(&wt).unwrap();
    assert!(original.is_empty());

    // Simulate the world changing between dialog-open and confirm.
    fs::write(wt.join("surprise.txt"), "oops\n").unwrap();

    let fresh = collect_dirty_files(&wt).unwrap();
    assert!(
        state_drifted(&original, &fresh),
        "new untracked file should trip the drift check"
    );
}
