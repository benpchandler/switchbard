//! UI-level tests that drive the *real* Switchbard views through egui_kittest.
//!
//! These mount the whole window (`HiveApp::render_ui`) against a seeded,
//! thread-free app, then assert via the accesskit tree (`query_by_label`) and
//! via `harness.state()`. They run headless on CI — no GPU, no real
//! filesystem/process scanning — so they are deterministic and safe to gate
//! on. For pixel-level visual regression see `tests/ui_snapshot.rs`.

mod common;

use std::path::PathBuf;

use common::{app_with_items, harness, item, seeded_app, REPO_NAME, REPO_PATH};
use kittest::Queryable;
use switchbard_core::config::Config;
use switchbard_core::{
    AgentKind, BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskSource, ContextKind,
    ContextScope, Repo, WorktreeRef,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, ViewTab};

fn seeded_backlog_task() -> BacklogTask {
    BacklogTask {
        id: "TASK-1".to_string(),
        title: "Seeded Backlog Task".to_string(),
        status: "To Do".to_string(),
        priority: "high".to_string(),
        assignees: vec!["ben".to_string()],
        labels: vec!["demo".to_string()],
        dependencies: vec![],
        references: vec![],
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
    }
}

/// Board lens (task-15 AC #1): switching to it should replace the list/detail
/// split with per-status columns, each showing the tasks currently in that
/// status as flight-strip cards.
#[test]
fn board_lens_renders_kanban_columns_with_the_seeded_task() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::Board;
    app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            loaded_at_unix: 0,
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Board").is_some(),
        "the Board lens tab should render"
    );
    assert!(
        harness.query_all_by_label("To Do").next().is_some(),
        "the To Do column header should render"
    );
    assert!(
        harness.query_all_by_label("In Progress").next().is_some(),
        "the In Progress column header should render even though it's empty"
    );
    assert!(
        harness.query_by_label("Seeded Backlog Task").is_some(),
        "the seeded task's flight strip should render in its status column"
    );
}

/// Global search overlay (task-15 AC #2): opening it and matching a query
/// should surface results across every tracked repo, prefixed with the
/// repo id the same way the All-projects list rows are.
#[test]
fn global_search_overlay_finds_the_matching_task_across_repos() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    // The List lens's own row renders the same "repo:id  title" label the
    // search result does (see the assertion below); the Digest lens
    // (task-21's default) doesn't render that format at all.
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.search.open = true;
    app.backlog_view.search.query = "Seeded".to_string();
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            loaded_at_unix: 0,
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Search all repos").is_some(),
        "the search window should be open"
    );
    // Both the search overlay's result row and the underlying List lens's own
    // row render the same "repo:id  title" label, so two nodes is correct.
    assert_eq!(
        harness
            .query_all_by_label(&format!("{REPO_NAME}:TASK-1  Seeded Backlog Task"))
            .count(),
        2,
        "the matching task should appear as a repo-prefixed search result"
    );

    harness.state_mut().backlog_view.search.query = "no-such-task-anywhere".to_string();
    harness.run();
    assert!(
        harness.query_by_label("No matches").is_some(),
        "a query with no hits should say so rather than showing stale results"
    );
}

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
fn agent_context_estimate_uses_effective_instructions_not_all_assets() {
    let mut instruction = item(
        "claude-md",
        AgentKind::Claude,
        ContextScope::Local,
        ContextKind::Instruction,
        "CLAUDE.md",
    );
    instruction.size_bytes = 1_000;
    let mut skill = item(
        "large-skill",
        AgentKind::Claude,
        ContextScope::Local,
        ContextKind::Skill,
        "large-skill/SKILL.md",
    );
    skill.size_bytes = 8_000;
    let mut nested_instruction = item(
        "nested-claude-md",
        AgentKind::Claude,
        ContextScope::Directory,
        ContextKind::Instruction,
        "apps/web/CLAUDE.md",
    );
    nested_instruction.size_bytes = 4_000;
    nested_instruction.applies_to = Some(PathBuf::from(format!("{REPO_PATH}/apps/web")));

    let mut app = app_with_items(vec![instruction, skill, nested_instruction]);
    app.view_tab = ViewTab::AgentContext;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_all_by_label("startup context ~1.0k chars / ~250 tokens")
            .next()
            .is_some(),
        "startup estimate should include only instructions effective at the selected worktree root"
    );
    assert!(
        harness
            .query_all_by_label("startup context ~13.0k chars / ~3.3k tokens")
            .next()
            .is_none(),
        "startup estimate must not sum every context asset in the repo"
    );
}

#[test]
fn backlog_view_surfaces_seeded_task() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            loaded_at_unix: 0,
        },
    );
    app.backlog_view
        .bulk_selected_tasks
        .insert((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
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

/// One task from each of two tracked repos, both numbered "TASK-1" — the
/// unified All-projects scope must merge them without id collisions, prefix
/// each row's id with its repo, and render a repo badge per row.
#[test]
fn backlog_all_projects_scope_merges_repos_with_a_repo_badge() {
    let repo_path = |name: &str| PathBuf::from(format!("/tmp/switchbard-ui-test/{name}"));
    let repos = vec![
        Repo {
            name: "alpha".to_string(),
            path: repo_path("alpha"),
        },
        Repo {
            name: "beta".to_string(),
            path: repo_path("beta"),
        },
    ];
    let worktrees = repos
        .iter()
        .map(|repo| WorktreeRef {
            repo_name: repo.name.clone(),
            path: repo.path.clone(),
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        })
        .collect();
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;

    for (repo_name, title) in [("alpha", "Alpha task"), ("beta", "Beta task")] {
        app.backlog_projects.lock().unwrap().insert(
            repo_path(repo_name),
            BacklogProject {
                root: repo_path(repo_name),
                cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
                tasks: vec![BacklogTask {
                    id: "TASK-1".to_string(),
                    title: title.to_string(),
                    status: "To Do".to_string(),
                    priority: "medium".to_string(),
                    assignees: vec![],
                    labels: vec![],
                    dependencies: vec![],
                    references: vec![],
                    milestone: None,
                    parent: None,
                    created_date: None,
                    updated_date: None,
                    description: String::new(),
                    implementation_plan: String::new(),
                    implementation_notes: String::new(),
                    final_summary: String::new(),
                    acceptance_criteria: vec![],
                    definition_of_done: vec![],
                    source: BacklogTaskSource::Active,
                    path: repo_path(repo_name).join("backlog/tasks/task-1.md"),
                }],
                warnings: vec![],
                loaded_at_unix: 0,
            },
        );
    }

    let mut harness = harness(app);
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_project,
        None,
        "the Backlog view defaults to the All-projects scope"
    );
    assert!(
        harness.query_by_label("alpha:TASK-1  Alpha task").is_some(),
        "row id is repo-prefixed in the All-projects scope"
    );
    assert!(
        harness.query_by_label("beta:TASK-1  Beta task").is_some(),
        "a same-numbered task from a different repo renders as a distinct row"
    );
    assert!(
        harness.query_all_by_label("alpha").next().is_some(),
        "repo badge should render on the alpha row"
    );
    assert!(
        harness.query_all_by_label("beta").next().is_some(),
        "repo badge should render on the beta row"
    );
}

/// Digest lens (task-21): the Backlog tab's default landing screen should
/// surface an in-progress task under its "In progress" section.
#[test]
fn digest_lens_is_the_backlog_default_and_surfaces_in_progress_tasks() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    let mut in_progress_task = seeded_backlog_task();
    in_progress_task.status = "In Progress".to_string();
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![in_progress_task],
            warnings: vec![],
            loaded_at_unix: 0,
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert_eq!(
        harness.state().backlog_view.lens,
        BacklogLens::Digest,
        "Digest is the Backlog tab's default lens"
    );
    assert!(
        harness.query_all_by_label("In progress").next().is_some(),
        "the In progress section header should render"
    );
    assert!(
        harness.query_by_label("Seeded Backlog Task").is_some(),
        "the in-progress task should render as a digest strip"
    );

    // Sections render Overdue, Newly unblocked, In progress, Recently done in
    // that order, each with its own "View all" button.
    harness
        .get_all_by_label("View all")
        .nth(2)
        .expect("the In progress section's View all button")
        .click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.lens,
        BacklogLens::List,
        "View all on a digest section should jump to the List lens"
    );
}

/// Portfolio lens (task-19): a read-only per-repo health table.
#[test]
fn portfolio_lens_renders_per_repo_health() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::Portfolio;
    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            loaded_at_unix: 0,
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_all_by_label(REPO_NAME).next().is_some(),
        "the repo name should render as a portfolio row"
    );
    assert!(
        harness.query_by_label("Oldest open").is_some(),
        "the oldest-open column header should render"
    );
    assert!(
        harness.query_by_label("Last activity").is_some(),
        "the last-activity column header should render"
    );
}

/// Dependency/blocked visibility (task-18): a task with an open dependency
/// should show a "blocked" marker in the List lens row and a per-dependency
/// status in the detail pane's Dependencies section; the dependency itself
/// should list this task under "Blocks".
#[test]
fn blocked_task_shows_a_marker_and_dependency_status_in_detail() {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_project = Some(PathBuf::from(REPO_PATH));

    let mut blocker = seeded_backlog_task();
    blocker.id = "TASK-1".to_string();
    blocker.title = "Blocking task".to_string();
    blocker.status = "To Do".to_string();

    let mut dependent = seeded_backlog_task();
    dependent.id = "TASK-2".to_string();
    dependent.title = "Dependent task".to_string();
    dependent.status = "To Do".to_string();
    dependent.dependencies = vec!["TASK-1".to_string()];
    dependent.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-2.md"));

    app.backlog_projects.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogProject {
            root: PathBuf::from(REPO_PATH),
            cli_path: Some(PathBuf::from("/usr/local/bin/backlog")),
            tasks: vec![blocker, dependent],
            warnings: vec![],
            loaded_at_unix: 0,
        },
    );
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-2".to_string()));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_all_by_label("blocked").next().is_some(),
        "the dependent task's row should show a blocked marker"
    );
    assert!(
        harness.query_by_label("TASK-1 Blocking task").is_some(),
        "the detail pane's Dependencies section should name the open dependency"
    );

    harness.state_mut().backlog_view.selected_task =
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    harness.run();
    assert!(
        harness.query_by_label("TASK-2 Dependent task").is_some(),
        "the blocking task's detail pane should list what it Blocks"
    );
}
