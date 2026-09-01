//! TASK-100 medic pass — BLOCKER: `ops::Pending::open_create_worktree` and
//! `create_worktree::render_modal` (the real `CreateWorktreeDialog` render
//! path) have existed since the merged Ops table shipped, but nothing in the
//! table ever set `pending.open_create_worktree`. The old workspace repo
//! card's "+ New worktree" header button died with the swimlanes and no
//! replacement was wired up — Create Worktree was reachable from no UI at
//! all. `worktree_create_state.rs` only unit-tests the `CreateWorktreeDialog`
//! struct's validation logic; this file closes the actual reachability gap
//! by clicking the real entry point (the primary row's "+ New worktree"
//! button, `row::render_actions_cell`) through to the real modal.

mod common;

use std::path::PathBuf;

use common::{harness, isolated_config_save_path};
use egui_kittest::kittest::Queryable;
use switchbard_core::config::Config;
use switchbard_core::{Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::Place;

const REPO_NAME: &str = "demo";
const REPO_PATH: &str = "/tmp/switchbard-create-wt-entry-test/demo";

fn app_with_one_repo() -> HiveApp {
    let repo_path = PathBuf::from(REPO_PATH);
    let worktrees = vec![WorktreeRef {
        repo_name: REPO_NAME.to_string(),
        path: repo_path.clone(),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let repos = vec![Repo {
        name: REPO_NAME.to_string(),
        path: repo_path,
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    cfg.repos = repos.clone();
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.place = Place::Ops;
    app
}

/// The button exists, is reachable on the primary row, and is not offered on
/// a linked (non-primary) worktree's row — creating a worktree is a per-repo
/// action, and the repo's identity lives on the primary row only.
#[test]
fn the_new_worktree_button_renders_once_on_the_primary_row_only() {
    let app = app_with_one_repo();
    app.worktrees.lock().unwrap().push(WorktreeRef {
        repo_name: REPO_NAME.to_string(),
        path: PathBuf::from(format!("{REPO_PATH}-linked")),
        branch: Some("feat/linked".to_string()),
        head: "def5678".to_string(),
    });
    let mut harness = harness(app);
    harness.run();

    assert_eq!(
        harness.get_all_by_label("+ New worktree").count(),
        1,
        "exactly one primary row exists for this one repo"
    );
}

/// Clicking it opens `HiveApp::create_worktree_dialog` for the right repo —
/// proving `pending.open_create_worktree` is actually set now, not just
/// declared.
#[test]
fn clicking_new_worktree_opens_the_dialog_for_its_repo() {
    let mut harness = harness(app_with_one_repo());
    harness.run();

    assert!(
        harness
            .state()
            .create_worktree_dialog
            .lock()
            .unwrap()
            .is_none(),
        "no dialog before the click"
    );

    harness.get_by_label("+ New worktree").click();
    harness.run();

    let dialog = harness
        .state()
        .create_worktree_dialog
        .lock()
        .unwrap()
        .clone()
        .expect("clicking + New worktree must open the create-worktree dialog");
    assert_eq!(dialog.repo.name, REPO_NAME);
}

/// The click reaches the *real* modal (`create_worktree::render_modal`), not
/// just the dialog struct `worktree_create_state.rs` already covers: the
/// window renders with the repo name and a working Create/Cancel pair.
#[test]
fn clicking_new_worktree_renders_the_real_create_worktree_modal() {
    let mut harness = harness(app_with_one_repo());
    harness.run();

    harness.get_by_label("+ New worktree").click();
    harness.run();

    // "Create worktree" labels both the window title and its submit button —
    // both must be present, which is exactly the real modal's shape
    // (`create_worktree::render_modal`), not just dialog state.
    assert_eq!(
        harness.get_all_by_label("Create worktree").count(),
        2,
        "expected the window title and the submit button"
    );
    // "demo" also labels the (still-rendered, behind the modal) Ops row —
    // just prove the modal's own repo-name label exists among them.
    assert!(harness.get_all_by_label(REPO_NAME).count() >= 2);
    assert!(harness.query_by_label("Cancel").is_some());
    assert!(harness.query_by_label("Location").is_some());
    assert!(harness.query_by_label("Checkout").is_some());

    // Cancel closes it — end-to-end proof this is the live modal, not a
    // frozen snapshot of one.
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(harness
        .state()
        .create_worktree_dialog
        .lock()
        .unwrap()
        .is_none());
}
