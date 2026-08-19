//! TASK-41: the bulk-remove sweep's two safety guarantees —
//!
//!   1. a dirty or unmerged worktree in the bulk selection lands in the
//!      needs-review list, never in `removable`, and the primary worktree
//!      never survives into either list even if it somehow ends up selected
//!      (`bulk_remove_confirm_auto_deselects_dirty_and_unmerged_into_needs_review`,
//!      driven through `egui_kittest`);
//!   2. actually confirming removal deletes every `removable` worktree AND
//!      its branch, verified against real `git worktree list` output
//!      (`run_bulk_removal_deletes_all_removable_worktrees_and_their_branches`,
//!      calling `worktree_actions::run_bulk_removal` directly).
//!
//! Both use real temp git repos (like `worktree_removal_orchestration.rs`)
//! because the classification and removal underneath (`collect_dirty_files`,
//! `assess_branch_delete`, `remove_worktree`, `delete_branch`) shell out to
//! real `git` — there's no fake to substitute without re-testing a parallel
//! "is this safe" check instead of the real one.
//!
//! `execute_bulk_remove_worktrees` itself (the `Arc<Mutex<>>`/`egui::Context`/
//! worker-thread wrapper around `run_bulk_removal`) is deliberately left
//! untested here, the same reasoning `worktree_removal_orchestration.rs`
//! gives for skipping `execute_remove_worktree`: threading makes end-to-end
//! orchestration testing impractical from the outside, so the decision/
//! execution logic is tested directly instead.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{harness, isolated_config_save_path};
use egui_kittest::kittest::Queryable;
use switchbard_core::config::Config;
use switchbard_core::git_cmd;
use switchbard_core::{Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;

/// One repo, one primary + three linked worktrees covering the three
/// classification outcomes the bulk dialog must distinguish:
///
///   - `clean_merged` — no unique commits vs `main`, no uncommitted files.
///   - `dirty` — landed on `main` but has an uncommitted scratch file.
///   - `unmerged` — a commit not yet on `main`.
///
/// Returns `(TempDir, repo_path, clean_merged, dirty, unmerged)`.
fn setup_repo_with_three_worktrees() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();

    run(&repo, &["init", "-q", "-b", "main"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    run(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "init"]);

    let clean_merged = tmp.path().join("wt-clean-merged");
    run(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/clean-merged",
            clean_merged.to_str().unwrap(),
        ],
    );

    let dirty = tmp.path().join("wt-dirty");
    run(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/dirty",
            dirty.to_str().unwrap(),
        ],
    );
    fs::write(dirty.join("scratch.txt"), "uncommitted\n").unwrap();

    let unmerged = tmp.path().join("wt-unmerged");
    run(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/unmerged",
            unmerged.to_str().unwrap(),
        ],
    );
    fs::write(unmerged.join("feature.txt"), "new work\n").unwrap();
    run(&unmerged, &["add", "."]);
    run(&unmerged, &["commit", "-qm", "unmerged work"]);

    (tmp, repo, clean_merged, dirty, unmerged)
}

fn run(cwd: &Path, args: &[&str]) {
    let status = git_cmd()
        .arg("-C")
        .arg(cwd)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

fn wt(repo_name: &str, path: PathBuf, branch: &str) -> WorktreeRef {
    WorktreeRef {
        repo_name: repo_name.to_string(),
        path,
        branch: Some(branch.to_string()),
        head: "abc1234".to_string(),
    }
}

fn app_with_worktrees(repo: PathBuf, worktrees: Vec<WorktreeRef>) -> HiveApp {
    let repos = vec![Repo {
        name: "demo".to_string(),
        path: repo,
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    // MUST be set on every test-constructed HiveApp — see `common::
    // isolated_config_save_path`'s doc (this is exactly how TASK-22 happened).
    app.config_save_path = Some(isolated_config_save_path());
    app
}

#[test]
fn bulk_remove_confirm_auto_deselects_dirty_and_unmerged_into_needs_review() {
    let (_tmp, repo, clean_merged, dirty, unmerged) = setup_repo_with_three_worktrees();
    let worktrees = vec![
        wt("demo", repo.clone(), "main"), // primary
        wt("demo", clean_merged.clone(), "feat/clean-merged"),
        wt("demo", dirty.clone(), "feat/dirty"),
        wt("demo", unmerged.clone(), "feat/unmerged"),
    ];
    let mut app = app_with_worktrees(repo.clone(), worktrees);
    // Select every worktree, including the primary — the dialog-open path
    // must silently drop it rather than ever offering it up for removal.
    app.bulk_selected_worktrees = [
        repo.clone(),
        clean_merged.clone(),
        dirty.clone(),
        unmerged.clone(),
    ]
    .into_iter()
    .collect();

    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("Remove 4 selected…").simulate_click();
    harness.run();

    let state = harness
        .state()
        .confirm_bulk_remove_worktrees
        .lock()
        .unwrap()
        .clone()
        .expect("confirming should open the bulk-remove dialog");

    assert_eq!(
        state.removable.len(),
        1,
        "only the clean, merged worktree should be removable; got {:?}",
        state.removable
    );
    assert_eq!(state.removable[0].worktree_path, clean_merged);

    assert_eq!(
        state.needs_review.len(),
        2,
        "dirty + unmerged should both be routed to needs-review; got {:?}",
        state.needs_review
    );
    let review_paths: Vec<PathBuf> = state
        .needs_review
        .iter()
        .map(|c| c.worktree_path.clone())
        .collect();
    assert!(
        review_paths.contains(&dirty),
        "dirty worktree must land in needs-review, not removable"
    );
    assert!(
        review_paths.contains(&unmerged),
        "unmerged worktree must land in needs-review, not removable"
    );

    // The primary was in the selection but must never appear in either list
    // — it is never a legal bulk-remove candidate.
    assert!(!state.removable.iter().any(|c| c.worktree_path == repo));
    assert!(!state.needs_review.iter().any(|c| c.worktree_path == repo));

    // Opening the dialog consumes the transient selection.
    assert!(
        harness.state().bulk_selected_worktrees.is_empty(),
        "the working selection should be cleared once the dialog opens"
    );
}

#[test]
fn bulk_remove_button_is_disabled_with_nothing_selected() {
    let (_tmp, repo, _clean_merged, _dirty, _unmerged) = setup_repo_with_three_worktrees();
    let app = app_with_worktrees(repo, vec![]);
    let mut harness = harness(app);
    harness.run();

    let button = harness.get_by_label("Remove 0 selected…");
    assert!(
        button.is_disabled(),
        "the bulk-remove button should be disabled when nothing is selected"
    );
}

/// One repo, one primary, and `n` linked worktrees each on a fresh branch
/// checked out from `main`'s tip — i.e. clean and trivially merged (no
/// unique commits), matching `worktree_remove.rs`'s
/// `fresh_branch_at_main_is_landed_and_not_forced` fixture shape. Returns
/// `(TempDir, repo_path, worktree_paths)`.
fn setup_repo_with_n_clean_merged_worktrees(
    n: usize,
) -> (tempfile::TempDir, PathBuf, Vec<PathBuf>) {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();

    run(&repo, &["init", "-q", "-b", "main"]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    run(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "init"]);

    let worktrees: Vec<PathBuf> = (0..n)
        .map(|i| {
            let path = tmp.path().join(format!("wt-clean-{i}"));
            run(
                &repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    &format!("feat/clean-{i}"),
                    path.to_str().unwrap(),
                ],
            );
            path
        })
        .collect();

    (tmp, repo, worktrees)
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    git_cmd()
        .arg("-C")
        .arg(repo)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// AC #3: selecting 5 merged+clean worktrees and confirming removal deletes
/// all 5 worktrees AND their branches, and `git worktree list` agrees
/// afterward. Drives `open_bulk_remove_worktree_confirm` (classification)
/// then `worktree_actions::run_bulk_removal` (the synchronous core
/// `execute_bulk_remove_worktrees`'s worker thread calls) directly against
/// real git state — no thread, no mocking of `remove_worktree`/`delete_branch`.
#[test]
fn run_bulk_removal_deletes_all_removable_worktrees_and_their_branches() {
    let (_tmp, repo, clean_worktrees) = setup_repo_with_n_clean_merged_worktrees(5);
    let mut worktree_refs = vec![wt("demo", repo.clone(), "main")];
    worktree_refs.extend(
        clean_worktrees
            .iter()
            .enumerate()
            .map(|(i, path)| wt("demo", path.clone(), &format!("feat/clean-{i}"))),
    );

    let mut app = app_with_worktrees(repo.clone(), worktree_refs);
    app.bulk_selected_worktrees = clean_worktrees.iter().cloned().collect();
    app.open_bulk_remove_worktree_confirm();

    let state = app
        .confirm_bulk_remove_worktrees
        .lock()
        .unwrap()
        .clone()
        .expect("all 5 clean+merged worktrees should open the dialog");
    assert_eq!(state.removable.len(), 5, "all 5 should be removable");
    assert!(
        state.needs_review.is_empty(),
        "none should need review: {:?}",
        state.needs_review
    );
    assert!(state.delete_branches, "delete_branches defaults on");

    let summary =
        switchbard_gui::worktree_actions::run_bulk_removal(&state.removable, state.delete_branches);

    assert_eq!(summary.removed.len(), 5, "all 5 should report removed");
    assert_eq!(
        summary.branch_deleted, 5,
        "all 5 branches should be deleted"
    );
    assert!(
        summary.first_error.is_none(),
        "got {:?}",
        summary.first_error
    );

    // `git worktree list` agrees: every clean worktree is gone from disk AND
    // from git's own bookkeeping.
    let output = git_cmd()
        .arg("-C")
        .arg(&repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&output.stdout);
    for path in &clean_worktrees {
        assert!(
            !path.exists(),
            "worktree dir {path:?} should be gone from disk"
        );
        assert!(
            !listed.contains(path.to_str().unwrap()),
            "worktree {path:?} should be gone from `git worktree list`, got:\n{listed}"
        );
    }
    for i in 0..5 {
        assert!(
            !branch_exists(&repo, &format!("feat/clean-{i}")),
            "branch feat/clean-{i} should have been deleted"
        );
    }
}
