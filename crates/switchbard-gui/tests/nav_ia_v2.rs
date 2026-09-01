//! IA V2 sidebar shell tests (TASK-96): place routing, the multi-select
//! repo scope, favorites, the filter-key migration end to end through
//! `HiveApp::new_headless`, and the narrow-width icon rail.
//!
//! Mounts the real window via `common::harness`/`common::seeded_app`, same
//! discipline as `tests/ui_views.rs` — these prove the actual render path,
//! not an isolated fragment.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use common::{harness, item, seeded_app, REPO_NAME, REPO_PATH};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use switchbard_core::config::{Config, FavoriteKind, FavoriteRef, FilterMemory};
use switchbard_core::{
    AgentContextMap, AgentKind, BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskSource,
    ContextKind, ContextScope, GoalDef, GoalInputs, GoalMeasure, GoalWeek, Repo, RepoRanking,
    WorktreeRef, DISPATCHING_LABEL, DISPATCH_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{AgentsSection, DispatchesFacet, Place, TasksView};

const REPO_B_NAME: &str = "second";
const REPO_B_PATH: &str = "/tmp/switchbard-ui-test/second";

fn backlog_task(id: &str, title: &str, status: &str) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: vec![],
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: Some("2026-06-20 12:00".to_string()),
        updated_date: Some("2026-06-20 12:00".to_string()),
        description: "body".to_string(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "criterion".to_string(),
        }],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/{id}.md")),
    }
}

fn backlog_repo(root: &str, tasks: Vec<BacklogTask>) -> BacklogRepo {
    BacklogRepo {
        root: PathBuf::from(root),
        tasks,
        warnings: vec![],
        project_defs: vec![],
        initiative_defs: vec![],
        goals: vec![],
        ranking: RepoRanking::default(),
        loaded_at_unix: 0,
        configured_statuses: vec![
            "To Do".into(),
            "In Progress".into(),
            "In Review".into(),
            "Done".into(),
        ],
    }
}

/// Same as [`backlog_repo`] but with `goals` set, for the Goals-place
/// scoping test.
fn backlog_repo_with_goals(
    root: &str,
    tasks: Vec<BacklogTask>,
    goals: Vec<GoalDef>,
) -> BacklogRepo {
    BacklogRepo {
        goals,
        ..backlog_repo(root, tasks)
    }
}

/// A manual goal with a target set for the *current* week (so
/// `compute_goal_statuses` — and therefore the nav's Goals count and the
/// Goals place body — actually picks it up; a goal without this week's key
/// is absence, not a zero, per that function's own doc). No check-ins: the
/// scoping tests only care whether the goal is counted at all, not its pace.
fn goal_def(name: &str) -> GoalDef {
    let week = switchbard_core::week_monday_of(chrono::Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string();
    GoalDef {
        name: name.to_string(),
        unit: "tasks".to_string(),
        measure: GoalMeasure::Manual,
        scope: None,
        inputs: GoalInputs::default(),
        weeks: std::collections::BTreeMap::from([(
            week,
            GoalWeek {
                target: 1,
                checkins: vec![],
            },
        )]),
    }
}

/// A `HiveApp` seeded with two tracked repos, each with one active task,
/// so scope-narrowing tests have something to narrow.
fn two_repo_app() -> HiveApp {
    let repos = vec![
        Repo {
            name: REPO_NAME.to_string(),
            path: PathBuf::from(REPO_PATH),
        },
        Repo {
            name: REPO_B_NAME.to_string(),
            path: PathBuf::from(REPO_B_PATH),
        },
    ];
    let worktrees = vec![
        WorktreeRef {
            repo_name: REPO_NAME.to_string(),
            path: PathBuf::from(REPO_PATH),
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        },
        WorktreeRef {
            repo_name: REPO_B_NAME.to_string(),
            path: PathBuf::from(REPO_B_PATH),
            branch: Some("main".to_string()),
            head: "def5678".to_string(),
        },
    ];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(common::isolated_config_save_path());
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        backlog_repo(
            REPO_PATH,
            // "In Progress", not "To Do": the Digest lens's sections key off
            // due-date/unblocked/in-progress/done — a plain "To Do" task
            // renders in none of them (see `digest::render_digest`'s own
            // section rules), so scope-narrowing tests that assert on
            // Digest visibility need a status that actually shows up there.
            vec![backlog_task(
                "TASK-1",
                "First repo's own task",
                "In Progress",
            )],
        ),
    );
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_B_PATH),
        backlog_repo(
            REPO_B_PATH,
            vec![backlog_task(
                "TASK-2",
                "Second repo's own task",
                "In Progress",
            )],
        ),
    );
    app
}

// ---------------------------------------------------------------------
// Place default + routing
// ---------------------------------------------------------------------

#[test]
fn a_fresh_app_lands_on_digest_every_launch() {
    let app = seeded_app();
    assert_eq!(
        app.place,
        Place::Digest,
        "Digest is the mock's landing place"
    );
}

#[test]
fn each_place_routes_to_its_own_body() {
    // Needs a tracked repo with a `backlog/` — `seeded_app()` has none, so
    // Digest/Tasks/Goals would all hit their own "no tracked backlog repo"
    // empty state instead of the body this test means to distinguish.
    let mut app = two_repo_app();

    app.place = Place::Digest;
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_by_label("In flight").is_some(),
        "Digest place (TASK-99) should render its own In flight section"
    );
    assert!(
        harness.query_by_label("Needs a human").is_some(),
        "Digest place (TASK-99) should render its own attention feed section"
    );
    assert!(
        harness.query_by_label("Statistics").is_none(),
        "Digest place must not carry the Tasks place's lens-tab chrome"
    );

    harness.state_mut().place = Place::Tasks;
    harness.state_mut().tasks_view = TasksView::All;
    harness.run();
    assert!(
        harness.query_by_label("Statistics").is_some(),
        "Tasks/All should render the whole Backlog view, lens tabs included"
    );

    harness.state_mut().place = Place::Tasks;
    harness.state_mut().tasks_view = TasksView::Dispatches;
    harness.run();
    assert!(
        harness.query_by_label("Nothing dispatched yet").is_some(),
        "Tasks/Dispatches should render the Dispatch view's empty state"
    );

    harness.state_mut().place = Place::Command;
    harness.run();
    assert!(
        // Not "Command" alone — the nav sidebar's own place row already
        // renders that label, so it is no longer unique (same reason
        // `dispatch_operability.rs`'s chip test switched to `get_all_by_
        // label`). The subtitle is this place body's own text.
        harness
            .query_by_label("the agent-scoped fleet console")
            .is_some(),
        "Command should render its own fleet console (TASK-98), not the old Agents heading"
    );

    harness.state_mut().place = Place::Goals;
    harness.run();
    assert!(
        harness.query_by_label("No goals this week").is_some(),
        "Goals place (TASK-101) should render its own index, empty-state here \
         since the fixture repos carry no goals.yml"
    );

    harness.state_mut().place = Place::Ops;
    harness.run();
    assert!(
        harness.query_by_label("Tracked repos").is_some(),
        "Ops should render the existing Servers/Workspace view (with its repo panel)"
    );
}

#[test]
fn clicking_a_place_row_navigates_and_tasks_always_lands_on_all_tasks() {
    let mut harness = harness(seeded_app());
    harness.run();

    harness.get_by_label("Command").click();
    harness.run();
    assert_eq!(harness.state().place, Place::Command);

    harness.get_by_label("Tasks").click();
    harness.run();
    assert_eq!(harness.state().place, Place::Tasks);
    assert_eq!(
        harness.state().tasks_view,
        TasksView::All,
        "entering Tasks via the place row always lands on All tasks"
    );

    harness.get_by_label("Dispatches").click();
    harness.run();
    assert_eq!(harness.state().tasks_view, TasksView::Dispatches);

    // Leaving and returning to Tasks resets the subview — never strands the
    // user on Dispatches by surprise.
    harness.get_by_label("Ops").click();
    harness.run();
    harness.get_by_label("Tasks").click();
    harness.run();
    assert_eq!(harness.state().tasks_view, TasksView::All);
}

// ---------------------------------------------------------------------
// Repo scope
// ---------------------------------------------------------------------

#[test]
fn empty_scope_means_all_repos_in_ops() {
    let mut app = two_repo_app();
    app.place = Place::Ops;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.state().repo_scope.is_empty());
    assert!(harness.query_by_label(REPO_NAME).is_some());
    assert!(harness.query_by_label(REPO_B_NAME).is_some());
}

#[test]
fn narrowing_scope_hides_the_other_repos_worktree_in_ops() {
    let mut app = two_repo_app();
    app.place = Place::Ops;
    app.repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label(REPO_NAME).is_some(),
        "the in-scope repo should still render"
    );
    assert!(
        harness.query_by_label(REPO_B_NAME).is_none(),
        "the out-of-scope repo must not render its worktree card"
    );
}

#[test]
fn narrowing_scope_hides_the_other_repos_tasks_in_digest() {
    let mut app = two_repo_app();
    app.place = Place::Digest;
    app.repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("First repo's own task").is_some(),
        "the in-scope repo's task should still render"
    );
    assert!(
        harness.query_by_label("Second repo's own task").is_none(),
        "the out-of-scope repo's task must not render"
    );
}

/// Missing coverage the reviewer noted: the Goals place (`render_goals_place`
/// -> `digest::render_goals_section`) reads `scoped_repos`, same as Digest,
/// but had no dedicated test of its own.
#[test]
fn narrowing_scope_hides_the_other_repos_goal_in_goals_place() {
    let mut app = two_repo_app();
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        backlog_repo_with_goals(
            REPO_PATH,
            vec![backlog_task(
                "TASK-1",
                "First repo's own task",
                "In Progress",
            )],
            vec![goal_def("First repo goal")],
        ),
    );
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_B_PATH),
        backlog_repo_with_goals(
            REPO_B_PATH,
            vec![backlog_task(
                "TASK-2",
                "Second repo's own task",
                "In Progress",
            )],
            vec![goal_def("Second repo goal")],
        ),
    );
    app.place = Place::Goals;
    let mut harness = harness(app);
    harness.run();
    assert_eq!(harness.state().place, Place::Goals, "precondition");
    assert!(
        harness.query_by_label("First repo goal").is_some(),
        "precondition: both goals render unscoped"
    );
    assert!(harness.query_by_label("Second repo goal").is_some());

    harness.state_mut().repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
    harness.run();

    assert!(
        harness.query_by_label("First repo goal").is_some(),
        "the in-scope repo's goal should still render in the Goals place"
    );
    assert!(
        harness.query_by_label("Second repo goal").is_none(),
        "the out-of-scope repo's goal must not render in the Goals place"
    );
}

#[test]
fn narrowing_scope_hides_the_other_repos_context_item_in_command() {
    let mut app = two_repo_app();
    app.agent_contexts.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        AgentContextMap {
            worktree: PathBuf::from(REPO_PATH),
            items: vec![item(
                "first-instruction",
                AgentKind::Claude,
                ContextScope::Local,
                ContextKind::Instruction,
                "FirstRepoInstruction.md",
            )],
            hooks: Vec::new(),
            hook_warnings: Vec::new(),
            hooks_disabled_by: None,
            scanned_at: Some(std::time::SystemTime::UNIX_EPOCH),
        },
    );
    app.agent_contexts.lock().unwrap().insert(
        PathBuf::from(REPO_B_PATH),
        AgentContextMap {
            worktree: PathBuf::from(REPO_B_PATH),
            items: vec![item(
                "second-instruction",
                AgentKind::Claude,
                ContextScope::Local,
                ContextKind::Instruction,
                "SecondRepoInstruction.md",
            )],
            hooks: Vec::new(),
            hook_warnings: Vec::new(),
            hooks_disabled_by: None,
            scanned_at: Some(std::time::SystemTime::UNIX_EPOCH),
        },
    );
    app.place = Place::Command;
    // TASK-98: Command's default section is now `Fleet`, not `Context` —
    // this test exercises the Context section specifically.
    app.agent_context_view.section = AgentsSection::Context;
    app.repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label(REPO_NAME).is_some(),
        "the in-scope repo's Command section should still render"
    );
    assert!(
        harness.query_by_label(REPO_B_NAME).is_none(),
        "the out-of-scope repo's Command section must not render"
    );
}

#[test]
fn narrowing_scope_hides_the_other_repos_row_in_the_dispatches_list() {
    let mut app = two_repo_app();
    {
        let mut repos = app.backlog_repos.lock().unwrap();
        repos
            .get_mut(&PathBuf::from(REPO_PATH))
            .expect("first repo seeded")
            .tasks[0]
            .labels = vec![DISPATCH_LABEL.to_string()];
        repos
            .get_mut(&PathBuf::from(REPO_B_PATH))
            .expect("second repo seeded")
            .tasks[0]
            .labels = vec![DISPATCH_LABEL.to_string()];
    }
    app.place = Place::Tasks;
    app.tasks_view = TasksView::Dispatches;
    // Both tasks only carry `DISPATCH_LABEL` (queued, no run yet), which
    // facets as `Queued`, not the default `Active` — see `ui::places::
    // dispatches::facet_for`.
    app.dispatches_view.facet = DispatchesFacet::Queued;
    app.repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("TASK-1").is_some(),
        "the in-scope repo's dispatch-flagged task should still list"
    );
    assert!(
        harness.query_by_label("TASK-2").is_none(),
        "the out-of-scope repo's dispatch-flagged task must not list"
    );
}

/// Post-review (MAJOR 1): `summarize_dispatch` used to be unscoped while
/// `collect_rows` (the Dispatches list itself) was already scoped, so
/// narrowing scope could leave the nav's "Dispatches (N)" badge and footer
/// lamp claiming a run the list didn't show at all. Both must now agree with
/// the list under a narrowed scope — and, since MINOR 1 makes them the exact
/// same `DispatchSummary` value computed once per frame, the top-bar chip
/// necessarily agrees too.
#[test]
fn dispatch_badge_and_lamp_match_the_scoped_dispatches_list() {
    let mut app = two_repo_app();
    {
        let mut repos = app.backlog_repos.lock().unwrap();
        repos
            .get_mut(&PathBuf::from(REPO_PATH))
            .expect("first repo seeded")
            .tasks[0]
            .labels = vec![DISPATCHING_LABEL.to_string()];
        repos
            .get_mut(&PathBuf::from(REPO_B_PATH))
            .expect("second repo seeded")
            .tasks[0]
            .labels = vec![DISPATCHING_LABEL.to_string()];
    }
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;

    // Unscoped first: both repos' in-flight runs count. "⚙ 2 running" should
    // appear TWICE — the top-bar chip and the nav footer lamp both render it,
    // which is exactly the "one shared DispatchSummary" property MINOR 1
    // fixed; querying for both nodes, not just "at least one", is the point.
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_by_label("Dispatches (2)").is_some(),
        "unscoped: the nav badge should count both repos' running dispatches"
    );
    assert_eq!(
        harness.query_all_by_label("⚙ 2 running").count(),
        2,
        "unscoped: both the top-bar chip and the nav footer lamp should show \
         the same unscoped running count"
    );

    // Narrow to the first repo only.
    harness.state_mut().repo_scope = std::iter::once(PathBuf::from(REPO_PATH)).collect();
    harness.run();

    assert!(
        harness.query_by_label("Dispatches (2)").is_none(),
        "the stale unscoped badge count must not survive narrowing"
    );
    assert!(
        harness.query_by_label("Dispatches (1)").is_some(),
        "scoped: the nav badge should match the scoped Dispatches list's one visible run"
    );
    assert_eq!(
        harness.query_all_by_label("⚙ 1 running").count(),
        2,
        "scoped: both the top-bar chip and the nav footer lamp should match \
         the scoped Dispatches list"
    );
    assert!(
        harness.query_by_label("⚙ 2 running").is_none(),
        "the stale unscoped chip/lamp text must not survive narrowing"
    );

    // And the list itself agrees with both.
    harness.state_mut().tasks_view = TasksView::Dispatches;
    harness.run();
    assert!(harness.query_by_label("TASK-1").is_some());
    assert!(harness.query_by_label("TASK-2").is_none());
}

// ---------------------------------------------------------------------
// Favorites
// ---------------------------------------------------------------------

#[test]
fn favorites_group_is_absent_when_empty_and_present_once_favorited() {
    let app = seeded_app();
    assert!(app.config.ui.favorites.is_empty());
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_by_label("FAVORITES").is_none(),
        "no header when there are no favorites"
    );

    harness.state_mut().toggle_favorite(
        FavoriteKind::Task,
        std::path::Path::new(REPO_PATH),
        "TASK-1",
    );
    harness.run();
    assert!(harness.query_by_label("FAVORITES").is_some());
    assert!(harness.query_by_label("TASK-1").is_some());
}

#[test]
fn clicking_a_favorited_task_navigates_to_tasks_and_selects_it() {
    let mut app = two_repo_app();
    app.config.ui.favorites.push(FavoriteRef {
        kind: FavoriteKind::Task,
        repo: REPO_PATH.to_string(),
        key: "TASK-1".to_string(),
    });
    app.place = Place::Ops;
    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("TASK-1").click();
    harness.run();

    let state = harness.state();
    assert_eq!(state.place, Place::Tasks);
    assert_eq!(state.tasks_view, TasksView::All);
    assert_eq!(
        state.backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()))
    );
}

#[test]
fn toggling_a_favorite_twice_is_idempotent_removal() {
    let mut app = seeded_app();
    let repo = PathBuf::from(REPO_PATH);
    assert!(!app.is_favorited(FavoriteKind::Goal, &repo, "Ship it"));
    app.toggle_favorite(FavoriteKind::Goal, &repo, "Ship it");
    assert!(app.is_favorited(FavoriteKind::Goal, &repo, "Ship it"));
    app.toggle_favorite(FavoriteKind::Goal, &repo, "Ship it");
    assert!(!app.is_favorited(FavoriteKind::Goal, &repo, "Ship it"));
    assert!(app.config.ui.favorites.is_empty());
}

// ---------------------------------------------------------------------
// Filter-key migration, end to end through HiveApp construction
// ---------------------------------------------------------------------

#[test]
fn old_lens_filter_keys_migrate_on_construction_and_unmatched_keys_drop() {
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    cfg.ui.filters.insert(
        "servers".to_string(),
        FilterMemory {
            query: "attributed".to_string(),
            facets: BTreeMap::new(),
        },
    );
    cfg.ui.filters.insert(
        "backlog".to_string(),
        FilterMemory {
            query: "triage".to_string(),
            facets: BTreeMap::new(),
        },
    );
    cfg.ui.filters.insert(
        "some-stale-key-nobody-recognizes".to_string(),
        FilterMemory {
            query: "should be dropped".to_string(),
            facets: BTreeMap::new(),
        },
    );

    let repos = vec![Repo {
        name: REPO_NAME.to_string(),
        path: PathBuf::from(REPO_PATH),
    }];
    let app = HiveApp::new_headless(cfg, repos, vec![]);

    assert_eq!(
        app.config.ui.filters.get("ops").map(|m| m.query.as_str()),
        Some("attributed"),
        "servers -> ops"
    );
    assert_eq!(
        app.config
            .ui
            .filters
            .get("tasks.all")
            .map(|m| m.query.as_str()),
        Some("triage"),
        "backlog -> tasks.all"
    );
    assert!(
        !app.config.ui.filters.contains_key("servers"),
        "the pre-migration key must not survive"
    );
    assert!(
        !app.config.ui.filters.contains_key("backlog"),
        "the pre-migration key must not survive"
    );
    assert!(
        !app.config
            .ui
            .filters
            .contains_key("some-stale-key-nobody-recognizes"),
        "an unrecognized key is dropped, never guessed"
    );
}

// ---------------------------------------------------------------------
// Narrow-width icon rail
// ---------------------------------------------------------------------

#[test]
fn narrow_window_collapses_the_sidebar_to_an_icon_rail() {
    let mut harness = harness(seeded_app());
    harness.set_size(egui::vec2(600.0, 700.0));
    harness.run();

    assert!(
        harness.query_by_label("SWITCHBARD").is_none(),
        "the full brand mark is rail-only-absent below the narrow threshold"
    );
    assert!(
        harness.query_by_label("SB").is_some(),
        "the rail shows the abbreviated brand mark"
    );
    assert!(
        harness.query_by_label("Ops").is_none(),
        "place names move to tooltips at rail width, not visible text"
    );
}

#[test]
fn wide_window_shows_the_expanded_sidebar() {
    let mut harness = harness(seeded_app());
    harness.set_size(egui::vec2(1280.0, 860.0));
    harness.run();

    assert!(harness.query_by_label("SWITCHBARD").is_some());
    assert!(harness.query_by_label("Ops").is_some());
}
