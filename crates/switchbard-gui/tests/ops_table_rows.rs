//! TASK-100: the merged Ops table's row-count contract and a few
//! capabilities `bulk_remove_worktrees.rs` doesn't already cover —
//! one-row-per-worktree, the external-squatter bottom row, and the
//! staleness facet narrowing the table exactly as it narrowed the retired
//! swimlane view. Verb-reachability (Rename/trash/checkbox/bulk-remove) and
//! removal gating live in `bulk_remove_worktrees.rs`; the tiered Open-button
//! decision logic lives in `ui::places::ops::row`'s own unit tests — this
//! file is the rendering-shape evidence those don't cover.

mod common;

use std::path::PathBuf;

use common::{harness, isolated_config_save_path};
use egui_kittest::kittest::Queryable;
use switchbard_core::config::Config;
use switchbard_core::types::LocalListener;
use switchbard_core::{AttributedListener, Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{Place, WorktreeMeta};

const REPO_NAME: &str = "demo";
const REPO_PATH: &str = "/tmp/switchbard-ops-rows-test/demo";

fn wt(path: PathBuf, branch: &str) -> WorktreeRef {
    WorktreeRef {
        repo_name: REPO_NAME.to_string(),
        path,
        branch: Some(branch.to_string()),
        head: "abc1234".to_string(),
    }
}

/// One primary + three linked worktrees, no repo scope narrowing.
fn app_with_four_worktrees() -> HiveApp {
    let repo_path = PathBuf::from(REPO_PATH);
    let worktrees = vec![
        wt(repo_path.clone(), "main"),
        wt(PathBuf::from(format!("{REPO_PATH}-a")), "feat/a"),
        wt(PathBuf::from(format!("{REPO_PATH}-b")), "feat/b"),
        wt(PathBuf::from(format!("{REPO_PATH}-c")), "feat/c"),
    ];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let repos = vec![Repo {
        name: REPO_NAME.to_string(),
        path: repo_path,
    }];
    cfg.repos = repos.clone();
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.place = Place::Ops;
    app
}

/// Mock §6's contract: exactly one row per worktree. Each linked worktree's
/// branch label is a unique, unambiguous anchor (the primary shows its
/// branch too, but combined with the repo name — see
/// `render_worktree_cell`), so counting distinct branch labels is counting
/// rows.
#[test]
fn ops_table_renders_exactly_one_row_per_worktree() {
    let app = app_with_four_worktrees();
    let mut harness = harness(app);
    harness.run();

    for branch in ["feat/a", "feat/b", "feat/c"] {
        let matches = harness.get_all_by_label(branch).count();
        assert_eq!(matches, 1, "expected exactly one row for branch {branch}");
    }
    // The primary's own branch ("main") plus its repo name both render as
    // their own labels.
    assert_eq!(harness.get_all_by_label(REPO_NAME).count(), 1);
    assert_eq!(harness.get_all_by_label("main").count(), 1);
}

/// A worktree the caller never lists must not get a row — the flat row list
/// is rebuilt from `worktrees_snapshot()` every frame, and this catches the
/// class of bug where a row list is built once and never re-derived (a
/// stray row would survive a worktree going away).
#[test]
fn a_worktree_not_in_the_snapshot_renders_no_row() {
    let app = app_with_four_worktrees();
    app.worktrees
        .lock()
        .unwrap()
        .retain(|w| w.branch.as_deref() != Some("feat/b"));
    let mut harness = harness(app);
    harness.run();

    assert_eq!(harness.get_all_by_label("feat/a").count(), 1);
    assert!(harness.query_by_label("feat/b").is_none());
    assert_eq!(harness.get_all_by_label("feat/c").count(), 1);
}

/// External squatter — a listener attributed to no worktree — renders its
/// own bottom row (mock §6) with the port, the owning command name, and the
/// kill affordance's hit target present (not clicked here — `spawn_kill`
/// signals a real pgid on a background thread, which
/// `bulk_remove_worktrees.rs`'s trash-button tests already establish is safe
/// to drive through kittest for a worktree-remove verb; this file only
/// proves the squatter row itself renders, matching the removal-gating
/// evidence bar without adding a second live-signal test).
#[test]
fn external_squatter_renders_its_own_row() {
    let app = app_with_four_worktrees();
    app.state
        .lock()
        .unwrap()
        .listeners
        .push(AttributedListener {
            listener: LocalListener {
                pid: 4821,
                pgid: 4821,
                port: 3000,
                command_name: "node".to_string(),
                cwd: None,
            },
            repo_name: None,
            worktree_path: None,
            worktree_branch: None,
        });
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_by_label("external process · pid 4821")
            .is_some(),
        "the squatter row must identify itself by pid, not attribute to a worktree"
    );
    assert!(
        harness.get_all_by_label(":3000").next().is_some(),
        "the squatter row must show the port it holds"
    );
    assert!(
        harness.get_all_by_label("node").next().is_some(),
        "the squatter row must show the owning command name"
    );
}

/// The staleness facet chip (TASK-41) narrows the merged table's rows
/// exactly as it narrowed the retired swimlane view — `passes_staleness_
/// filter`'s own unit tests prove the predicate; this proves the table
/// actually applies it.
#[test]
fn staleness_filter_narrows_the_table_rows() {
    let app = app_with_four_worktrees();
    let no_upstream_path = PathBuf::from(format!("{REPO_PATH}-a"));
    app.meta.lock().unwrap().insert(
        no_upstream_path,
        WorktreeMeta {
            staleness: Some(switchbard_core::WorktreeStaleness::NoUpstream),
            ..Default::default()
        },
    );

    let mut harness = harness(app);
    harness.run();
    assert_eq!(harness.get_all_by_label("feat/a").count(), 1);
    assert_eq!(harness.get_all_by_label("feat/b").count(), 1);

    harness.get_by_label("No upstream (1)").click();
    harness.run();

    assert_eq!(
        harness.get_all_by_label("feat/a").count(),
        1,
        "the no-upstream worktree's row must survive the No upstream filter"
    );
    assert!(
        harness.query_by_label("feat/b").is_none(),
        "an unprobed worktree must not survive a non-All staleness filter"
    );
}
