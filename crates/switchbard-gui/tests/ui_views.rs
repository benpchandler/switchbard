//! UI-level tests that drive the *real* Switchbard views through egui_kittest.
//!
//! These mount the whole window (`HiveApp::render_ui`) against a seeded,
//! thread-free app, then assert via the accesskit tree (`query_by_label`) and
//! via `harness.state()`. They run headless on CI — no GPU, no real
//! filesystem/process scanning — so they are deterministic and safe to gate
//! on. For pixel-level visual regression see `tests/ui_snapshot.rs`.

mod common;

use std::path::PathBuf;

use common::{harness, seeded_app, REPO_NAME, REPO_PATH};
use kittest::Queryable;
use switchbard_core::{BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskSource};
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
    assert!(
        harness.query_by_label("Backlog").is_some(),
        "Backlog tab should be present"
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
fn clicking_backlog_tab_switches_view() {
    let mut harness = harness(seeded_app());

    harness.get_by_label("Backlog").click();
    harness.run();

    assert_eq!(harness.state().view_tab, ViewTab::Backlog);
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

#[test]
fn backlog_view_surfaces_seeded_task() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![BacklogTask {
                id: "TASK-1".to_string(),
                title: "Seeded Backlog Task".to_string(),
                status: "To Do".to_string(),
                priority: "high".to_string(),
                assignees: vec!["ben".to_string()],
                labels: vec!["demo".to_string()],
                dependencies: vec![],
                milestone: None,
                parent: None,
                created_date: Some("2026-06-20 12:00".to_string()),
                updated_date: Some("2026-06-20 12:00".to_string()),
                description: "Task detail body".to_string(),
                implementation_plan: String::new(),
                implementation_notes: "Existing note".to_string(),
                final_summary: String::new(),
                acceptance_criteria: vec![BacklogChecklistItem {
                    index: 1,
                    checked: false,
                    text: "Criterion renders".to_string(),
                }],
                definition_of_done: vec![],
                source: BacklogTaskSource::Active,
                path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-1.md")),
            }],
            warnings: vec![],
            loaded_at_unix: 0,
        },
    );
    app.backlog_view
        .bulk_selected_task_ids
        .insert("TASK-1".to_string());
    app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_by_label("TASK-1  Seeded Backlog Task")
            .is_some(),
        "task row should render"
    );
    assert!(
        harness.query_by_label("Sort").is_some(),
        "task sort controls should render"
    );
    assert!(
        harness.query_by_label("Ascending").is_some(),
        "sort direction control should render"
    );
    assert!(
        harness.query_by_label("1 selected").is_some(),
        "bulk-selection count should render"
    );
    assert!(
        harness.query_by_label("#1 Criterion renders").is_some(),
        "acceptance criterion should render in the detail pane"
    );
}
