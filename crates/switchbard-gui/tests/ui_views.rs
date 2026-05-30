//! UI-level tests that drive the *real* Switchbard views through egui_kittest.
//!
//! These mount the whole window (`HiveApp::render_ui`) against a seeded,
//! thread-free app, then assert via the accesskit tree (`query_by_label`) and
//! via `harness.state()`. They run headless on CI — no GPU, no real
//! filesystem/process scanning — so they are deterministic and safe to gate
//! on. For pixel-level visual regression see `tests/ui_snapshot.rs`.

mod common;

use common::{harness, seeded_app, REPO_NAME};
use kittest::Queryable;
use switchbard_gui::runtime::ViewTab;

#[test]
fn window_defaults_to_servers_view() {
    let harness = harness(seeded_app());

    assert_eq!(harness.state().view_tab, ViewTab::Servers);
    // Both view tabs are always offered in the top bar.
    assert!(
        harness.query_by_label("Servers").is_some(),
        "Servers tab should be present"
    );
    assert!(
        harness.query_by_label("Agent Context").is_some(),
        "Agent Context tab should be present"
    );
}

#[test]
fn clicking_agent_context_tab_switches_view() {
    let mut harness = harness(seeded_app());

    // In the default Servers view the only "Agent Context" widget is the tab,
    // so this is unambiguous.
    harness.get_by_label("Agent Context").click();
    harness.run();

    assert_eq!(harness.state().view_tab, ViewTab::AgentContext);
}

#[test]
fn agent_context_view_surfaces_seeded_assets() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::AgentContext;
    let mut harness = harness(app);
    harness.run();

    // Summary counts the two seeded assets, the repo heading renders, and the
    // seeded CLAUDE.md item shows in the explorer under the default selection.
    // The "N assets" count renders in both the page summary and the repo card,
    // and CLAUDE.md appears in both the item row and the effective-instruction
    // stack, so use the duplicate-tolerant `query_all_*` variants.
    assert!(
        harness.query_all_by_label("2 assets").next().is_some(),
        "summary should report the two seeded assets"
    );
    assert!(
        harness.query_all_by_label(REPO_NAME).next().is_some(),
        "repo heading should render"
    );
    assert!(
        harness.query_all_by_label("CLAUDE.md").next().is_some(),
        "seeded CLAUDE.md item should render in the explorer"
    );
}
