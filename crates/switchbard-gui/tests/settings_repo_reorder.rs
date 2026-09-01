//! TASK-100 medic pass — MAJOR: repo reordering dropped. `HiveApp::move_repo`
//! has existed since before the old sidebar panel's up/down triangles were
//! retired with it, but the merged Ops table never replaced that entry
//! point, and `ui::settings` (the sanctioned home for repo CRUD per its own
//! module doc) only ever offered Add/Remove. This proves the Settings
//! window's new reorder controls (`settings::render_reorder_controls`) are
//! actually wired to `move_repo`, not just drawn.

mod common;

use std::path::PathBuf;

use common::{harness, isolated_config_save_path};
use egui_kittest::kittest::{NodeT, Queryable};
use switchbard_core::config::Config;
use switchbard_core::{Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;

fn repo(name: &str) -> Repo {
    Repo {
        name: name.to_string(),
        path: PathBuf::from(format!("/tmp/switchbard-settings-reorder-test/{name}")),
    }
}

fn app_with_three_repos() -> HiveApp {
    let repos = vec![repo("alpha"), repo("bravo"), repo("charlie")];
    let worktrees: Vec<WorktreeRef> = repos
        .iter()
        .map(|r| WorktreeRef {
            repo_name: r.name.clone(),
            path: r.path.clone(),
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        })
        .collect();
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    cfg.repos = repos.clone();
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.settings_open = true;
    app
}

fn repo_order(harness: &egui_kittest::Harness<'static, HiveApp>) -> Vec<String> {
    harness
        .state()
        .config
        .repos
        .iter()
        .map(|r| r.name.clone())
        .collect()
}

#[test]
fn clicking_move_down_swaps_the_repo_with_its_successor() {
    let mut harness = harness(app_with_three_repos());
    harness.run();

    assert_eq!(repo_order(&harness), vec!["alpha", "bravo", "charlie"]);

    // One "Move repo down" per row (mirrors `rank_arrows`'s own convention:
    // always rendered, just disabled at the boundary — see
    // `render_reorder_controls`'s doc). Row order is alpha, bravo, charlie,
    // so index 0 is alpha's.
    let downs: Vec<_> = harness.get_all_by_label("Move repo down").collect();
    assert_eq!(downs.len(), 3, "one per repo row, disabled or not");
    downs[0].click();
    harness.run();

    assert_eq!(
        repo_order(&harness),
        vec!["bravo", "alpha", "charlie"],
        "moving alpha down must swap it with bravo"
    );
}

#[test]
fn clicking_move_up_swaps_the_repo_with_its_predecessor() {
    let mut harness = harness(app_with_three_repos());
    harness.run();

    // Row order is alpha, bravo, charlie; index 2 is charlie's "Move repo up".
    let ups: Vec<_> = harness.get_all_by_label("Move repo up").collect();
    assert_eq!(ups.len(), 3, "one per repo row, disabled or not");
    ups[2].click();
    harness.run();

    assert_eq!(
        repo_order(&harness),
        vec!["alpha", "charlie", "bravo"],
        "moving charlie up must swap it with bravo"
    );
}

/// Both arrows always render (matching `rank_arrows`'s convention — see
/// `render_reorder_controls`'s doc) but the boundary ones report themselves
/// disabled via AccessKit, and clicking a disabled control is a no-op.
#[test]
fn the_top_repos_move_up_and_the_bottom_repos_move_down_are_disabled() {
    let mut harness = harness(app_with_three_repos());
    harness.run();

    let ups: Vec<_> = harness.get_all_by_label("Move repo up").collect();
    assert_eq!(ups.len(), 3);
    assert!(
        ups[0].accesskit_node().is_disabled(),
        "alpha (top) can't move up"
    );
    assert!(!ups[1].accesskit_node().is_disabled());
    assert!(!ups[2].accesskit_node().is_disabled());

    let downs: Vec<_> = harness.get_all_by_label("Move repo down").collect();
    assert_eq!(downs.len(), 3);
    assert!(!downs[0].accesskit_node().is_disabled());
    assert!(!downs[1].accesskit_node().is_disabled());
    assert!(
        downs[2].accesskit_node().is_disabled(),
        "charlie (bottom) can't move down"
    );

    // Clicking the disabled boundary control must not reorder anything.
    ups[0].click();
    downs[2].click();
    harness.run();
    assert_eq!(repo_order(&harness), vec!["alpha", "bravo", "charlie"]);
}

/// Reordering persists through `move_repo`'s own `save_config` +
/// `rebuild_worktrees` — the runtime `repos` mutex (what the Ops table
/// actually reads) must reflect the new order too, not just `config.repos`.
#[test]
fn reordering_updates_the_runtime_repo_order_the_ops_table_reads() {
    let mut harness = harness(app_with_three_repos());
    harness.run();

    harness
        .get_all_by_label("Move repo down")
        .next()
        .expect("alpha's down arrow")
        .click();
    harness.run();

    let runtime_order: Vec<String> = harness
        .state()
        .repos_snapshot()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(runtime_order, vec!["bravo", "alpha", "charlie"]);
}
