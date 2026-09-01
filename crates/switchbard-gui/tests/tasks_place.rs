//! TASK-97 evidence: the Tasks place's generic group-by, expanding
//! headers, rank sort, filter builder (add/remove/persist), stroke-ring
//! selection, sub-issue indentation, and List/Board facet sharing. Mounts
//! the real window via `common::harness`/`common::seeded_app` (same
//! discipline as `tests/nav_ia_v2.rs`) — these prove the actual render
//! path, not an isolated fragment.

mod common;

use std::path::PathBuf;

use common::{harness, isolated_config_save_path, seeded_app, REPO_PATH};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use switchbard_core::config::{Config, FilterMemory};
use switchbard_core::{
    BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskSource, Repo, RepoRanking,
    WorktreeRef,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogTaskSortKey, Place, TasksView};
use switchbard_gui::ui::places::tasks::fields::TaskField;
use switchbard_gui::ui::places::tasks::state::{FilterPredicate, TasksViewMode};

fn task(id: &str, title: &str, status: &str) -> BacklogTask {
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
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-01 09:00".to_string()),
        description: String::new(),
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

fn repo_with(tasks: Vec<BacklogTask>) -> BacklogRepo {
    BacklogRepo {
        root: PathBuf::from(REPO_PATH),
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

fn tasks_app(tasks: Vec<BacklogTask>) -> HiveApp {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), repo_with(tasks));
    app
}

const SECOND_REPO_PATH: &str = "/tmp/switchbard-ui-test/second";

fn two_repo_tasks_app(mut config: Config) -> HiveApp {
    config.ui.onboarding_dismissed = true;
    let repos = vec![
        Repo {
            name: "demo".to_string(),
            path: PathBuf::from(REPO_PATH),
        },
        Repo {
            name: "second".to_string(),
            path: PathBuf::from(SECOND_REPO_PATH),
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
    let mut app = HiveApp::new_headless(config, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.tasks_place.group_by = None;
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        repo_with(vec![task("TASK-1", "First repo task", "To Do")]),
    );
    let mut second_repo = repo_with(vec![task("TASK-2", "Second repo task", "To Do")]);
    second_repo.root = PathBuf::from(SECOND_REPO_PATH);
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(SECOND_REPO_PATH), second_repo);
    app
}

// ---------------------------------------------------------------------
// Generic group-by, with computed roll-up counts
// ---------------------------------------------------------------------

#[test]
fn group_by_status_shows_computed_header_roll_ups() {
    let mut app = tasks_app(vec![
        task("TASK-1", "First", "To Do"),
        task("TASK-2", "Second", "To Do"),
        task("TASK-3", "Third", "Done"),
    ]);
    app.tasks_place.group_by = Some(TaskField::Status);
    // Done tasks are hidden by default (`backlog_view.show_completed`,
    // shared base visibility, not a filter-builder predicate) — reveal them
    // so the Done bucket actually has a member to compute a roll-up over.
    app.backlog_view.show_completed = true;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("0/2 done").is_some(),
        "the To Do group's header should show its computed roll-up"
    );
    assert!(
        harness.query_by_label("1/1 done").is_some(),
        "the Done group's header should show its computed roll-up"
    );
    assert!(harness.query_all_by_label("To Do").next().is_some());
    assert!(harness.query_all_by_label("Done").next().is_some());
}

#[test]
fn group_by_label_fans_a_multi_labeled_task_into_every_group() {
    let mut labeled = task("TASK-1", "Multi-label task", "To Do");
    labeled.labels = vec!["frontend".to_string(), "urgent".to_string()];
    let mut app = tasks_app(vec![labeled]);
    app.tasks_place.group_by = Some(TaskField::Label);
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("frontend").is_some());
    assert!(harness.query_by_label("urgent").is_some());
}

#[test]
fn group_by_repo_buckets_cross_repo_tasks_by_repo_name() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.tasks_place.group_by = Some(TaskField::Repo);
    app.repos.lock().unwrap().push(Repo {
        name: "second".to_string(),
        path: PathBuf::from("/tmp/switchbard-ui-test/second"),
    });
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        repo_with(vec![task("TASK-1", "A", "To Do")]),
    );
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from("/tmp/switchbard-ui-test/second"),
        repo_with(vec![task("TASK-2", "B", "To Do")]),
    );
    let mut harness = harness(app);
    harness.run();

    // "demo"/"second" each appear twice (the group header's key AND the
    // per-row repo badge directive #9 always renders) — query_all rather
    // than a single-match query.
    assert!(harness.query_all_by_label("demo").next().is_some());
    assert!(harness.query_all_by_label("second").next().is_some());
    assert!(harness.query_by_label("TASK-1  A").is_some());
    assert!(harness.query_by_label("TASK-2  B").is_some());
}

#[test]
fn group_by_project_matches_the_pre_task_97_projects_lens_grouping() {
    let mut a = task("TASK-1", "In Alpha", "To Do");
    a.project = Some("Alpha".to_string());
    let app = tasks_app(vec![a, task("TASK-2", "No project", "To Do")]);
    // Project is the default group-by field.
    assert_eq!(app.tasks_place.group_by, Some(TaskField::Project));
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("Alpha").is_some());
    assert!(harness.query_by_label("No project").is_some());
}

// ---------------------------------------------------------------------
// Expanding header summary
// ---------------------------------------------------------------------

#[test]
fn clicking_a_group_header_caret_expands_its_in_place_summary() {
    let mut a = task("TASK-1", "In Alpha", "In Progress");
    a.project = Some("Alpha".to_string());
    let app = tasks_app(vec![a]);
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("1 remaining").is_none(),
        "precondition: the summary band is not shown before expanding"
    );
    assert!(harness.state().tasks_place.expanded_groups.is_empty());

    harness
        .state_mut()
        .tasks_place
        .expanded_groups
        .insert("Alpha".to_string());
    harness.run();
    assert!(
        harness.query_by_label("1 remaining").is_some(),
        "expanding the group header should show its in-place summary band"
    );
}

// ---------------------------------------------------------------------
// Rank sort ordering vs a fixture RepoRanking
// ---------------------------------------------------------------------

#[test]
fn rank_sort_orders_by_the_computed_repo_ranking() {
    // `repo.tasks` is already in the computed `RepoRanking::sort_tasks`
    // order by construction time here (mirroring what `load_backlog_repo`
    // would produce for this ranking) — TASK-1 expedited jumps to the
    // front regardless of the list's insertion order.
    let mut repo = repo_with(vec![
        task("TASK-2", "Second in rank order", "To Do"),
        task("TASK-1", "Expedited, jumps the order", "To Do"),
        task("TASK-3", "Third in rank order", "To Do"),
    ]);
    repo.ranking = RepoRanking {
        expedite: vec!["TASK-1".to_string()],
        ..RepoRanking::default()
    };
    // Reorder `repo.tasks` itself into the rank order `sort_tasks` would
    // produce (expedited first) — this test proves the GUI's Rank sort key
    // respects that pre-computed order, not that core re-derives it (core
    // owns that; `backlog::ranking::sort_tasks`'s own tests cover it).
    repo.tasks = vec![
        task("TASK-1", "Expedited, jumps the order", "To Do"),
        task("TASK-2", "Second in rank order", "To Do"),
        task("TASK-3", "Third in rank order", "To Do"),
    ];

    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.tasks_place.group_by = None;
    app.backlog_view.sort_key = BacklogTaskSortKey::Rank;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), repo);
    let mut harness = harness(app);
    harness.run();

    // TASK-97 medic pass (should-fix finding): the original version of this
    // test only asserted the expedited task's *presence* — it would have
    // passed even if Rank sort silently fell back to insertion order, since
    // "Expedited, jumps the order" renders somewhere in the list either way.
    // Asserting each row's rect's vertical position against the others
    // proves the actual order: expedited first, then TASK-2, then TASK-3 —
    // the rank order, not the construction order (TASK-2 first, TASK-1
    // second) `repo_with` was built with above.
    let y = |label: &str| harness.get_by_label(label).rect().top();
    let expedited_y = y("TASK-1  Expedited, jumps the order");
    let second_y = y("TASK-2  Second in rank order");
    let third_y = y("TASK-3  Third in rank order");

    assert!(
        expedited_y < second_y,
        "the expedited task should render above the second-ranked one \
         (y={expedited_y} vs y={second_y})"
    );
    assert!(
        second_y < third_y,
        "the second-ranked task should render above the third-ranked one \
         (y={second_y} vs y={third_y})"
    );
}

#[test]
fn rank_sort_label_appears_only_in_the_sort_menu() {
    assert_eq!(BacklogTaskSortKey::Rank.label(), "Rank");
    assert_eq!(BacklogTaskSortKey::Rank.as_saved_id(), "rank");
}

// ---------------------------------------------------------------------
// Filter builder: add / remove / persist
// ---------------------------------------------------------------------

#[test]
fn facets_controls_remain_horizontal_across_supported_widths() {
    let mut bad = Vec::new();
    for width in (600..=1400).step_by(25) {
        let app = tasks_app(vec![task("TASK-1", "Visible task", "To Do")]);
        let mut harness = harness(app);
        harness.set_size(egui::vec2(width as f32, 548.0));
        harness.run();

        let filter = harness.get_by_label("+ Filter").rect();
        let list = harness.get_by_label("List").rect();
        let board = harness.get_by_label("Board").rect();
        if filter.height() > 44.0
            || filter.width() < 52.0
            || list.width() < 36.0
            || board.width() < 48.0
            || filter.intersects(list)
            || filter.intersects(board)
            || list.intersects(board)
        {
            bad.push((width, filter, list, board));
        }
    }
    assert!(
        bad.is_empty(),
        "facets controls collapsed or overlapped: {bad:?}"
    );
}

#[test]
fn adding_a_filter_predicate_narrows_the_visible_tasks() {
    let mut app = tasks_app(vec![
        task("TASK-1", "In progress task", "In Progress"),
        task("TASK-2", "To do task", "To Do"),
    ]);
    app.tasks_place.group_by = None;
    let mut harness = harness(app);
    harness.run();
    assert!(harness.query_by_label("TASK-1  In progress task").is_some());
    assert!(harness.query_by_label("TASK-2  To do task").is_some());

    harness
        .state_mut()
        .tasks_place
        .filters
        .push(FilterPredicate {
            field: TaskField::Status,
            value: "In Progress".to_string(),
        });
    harness.run();

    assert!(harness.query_by_label("TASK-1  In progress task").is_some());
    assert!(
        harness.query_by_label("TASK-2  To do task").is_none(),
        "a Status: In Progress filter predicate should hide the To Do task"
    );
    assert!(
        harness.query_by_label("Status: In Progress ✕").is_some(),
        "the active predicate should render as a removable chip"
    );
}

#[test]
fn a_positive_scope_with_no_filter_matches_renders_an_honest_empty_state() {
    let mut app = tasks_app(vec![task("TASK-1", "Only scoped task", "To Do")]);
    app.tasks_place.group_by = None;
    app.tasks_place.filters = vec![FilterPredicate {
        field: TaskField::Status,
        value: "In Review".to_string(),
    }];

    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_by_label("No tasks match the current filters")
            .is_some(),
        "a loaded positive scope narrowed to zero must explain the empty body"
    );
    assert!(
        harness.query_by_label("0 of 1 · 1 open").is_some(),
        "the summary must distinguish zero filter matches from one open scoped task"
    );
    assert!(harness.query_by_label("TASK-1  Only scoped task").is_none());
}

#[test]
fn removing_the_last_filter_predicate_restores_every_task_and_remembers_it_as_recent() {
    let mut app = tasks_app(vec![
        task("TASK-1", "In progress task", "In Progress"),
        task("TASK-2", "To do task", "To Do"),
    ]);
    app.tasks_place.group_by = None;
    app.tasks_place.filters = vec![FilterPredicate {
        field: TaskField::Status,
        value: "In Progress".to_string(),
    }];
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_by_label("TASK-2  To do task").is_none(),
        "precondition"
    );

    harness.get_by_label("Status: In Progress ✕").click();
    harness.run();

    assert!(
        harness.query_by_label("TASK-2  To do task").is_some(),
        "removing the predicate should restore every task"
    );
    assert_eq!(
        harness.state().tasks_place.recent_filter_sets.len(),
        1,
        "the cleared set should be remembered as recent"
    );
    assert!(harness.query_by_label("recent:").is_some());
}

#[test]
fn legacy_repo_picker_state_cannot_hide_repos_or_filter_values() {
    let mut config = Config::default();
    config
        .ui
        .filters
        .entry("tasks.all".to_string())
        .or_default()
        .facets
        .insert("repo".to_string(), REPO_PATH.to_string());
    let mut app = two_repo_tasks_app(config);
    app.tasks_place.filter_builder_open = true;
    app.tasks_place.draft_field = TaskField::Repo;

    let mut harness = harness(app);
    harness.run();
    harness.get_by_value("Choose a value").click();
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_repo,
        None,
        "the removed one-repo picker must not survive as an invisible scope"
    );
    assert!(harness.query_by_label("TASK-1  First repo task").is_some());
    assert!(
        harness.query_by_label("TASK-2  Second repo task").is_some(),
        "All repos must include tasks outside the obsolete persisted picker value"
    );
    assert!(
        harness.query_all_by_label("second").count() >= 2,
        "the second repo must appear on its task row and in the open Repo value picker"
    );
    assert!(
        !harness
            .state()
            .config
            .ui
            .filters
            .get("tasks.all")
            .expect("tasks.all filter memory")
            .facets
            .contains_key("repo"),
        "loading the config must purge the obsolete repo facet"
    );
}

#[test]
fn filter_value_picker_uses_the_pre_predicate_task_set() {
    let mut app = two_repo_tasks_app(Config::default());
    app.tasks_place.filters = vec![FilterPredicate {
        field: TaskField::Repo,
        value: "demo".to_string(),
    }];
    app.tasks_place.filter_builder_open = true;
    app.tasks_place.draft_field = TaskField::Repo;

    let mut harness = harness(app);
    harness.run();
    harness.get_by_value("Choose a value").click();
    harness.run();

    assert!(harness.query_by_label("TASK-1  First repo task").is_some());
    assert!(
        harness.query_by_label("TASK-2  Second repo task").is_none(),
        "the active Repo predicate should still narrow the task rows"
    );
    assert!(
        harness.query_by_label("second").is_some(),
        "the open picker must still offer values outside the active predicate result"
    );
}

// TASK-97: `HiveApp::persist_filter_facets` (called from `eframe::App::
// update`'s end-of-frame wrapper, not `render_ui` itself) is what drives
// this once a frame in production — but `common::harness` calls `render_ui`
// directly (bypassing `update`), the same reason no existing test proves
// `BacklogViewState::persist_filters` fires through a harness run either.
// This proves the wiring the same way: call `TasksPlaceState::persist`
// directly. Round-trip encoding correctness (including the `filters`/
// `recent_filters` predicate-set format) is `state.rs`'s own unit test's
// job (`filter_set_encoding_round_trips_through_persist_and_restore`).
#[test]
fn tasks_place_state_persists_group_by_view_mode_and_filters_under_tasks_all() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.tasks_place.group_by = Some(TaskField::Status);
    app.tasks_place.view_mode = TasksViewMode::Board;
    app.tasks_place.filters = vec![FilterPredicate {
        field: TaskField::Label,
        value: "bug".to_string(),
    }];

    app.tasks_place.persist(&mut app.config.ui);

    let memory: &FilterMemory = app
        .config
        .ui
        .filters
        .get("tasks.all")
        .expect("tasks.all filter memory should exist after persisting");
    assert_eq!(
        memory.facets.get("group_by").map(String::as_str),
        Some("status")
    );
    assert_eq!(
        memory.facets.get("view_mode").map(String::as_str),
        Some("board")
    );
    assert!(memory.facets.contains_key("filters"));
}

// ---------------------------------------------------------------------
// Stroke-ring selection
// ---------------------------------------------------------------------

#[test]
fn clicking_a_row_selects_it_the_same_way_the_boards_stroke_ring_selection_does() {
    // A second task: with only one, `reconcile_selected_task` auto-selects
    // it immediately (same as everywhere else this pattern shows up), which
    // would make "select TASK-1" indistinguishable from "nothing changed."
    let mut app = tasks_app(vec![
        task("TASK-1", "Selectable task", "To Do"),
        task("TASK-2", "Other task", "To Do"),
    ]);
    app.tasks_place.group_by = None;
    // Deterministic precondition rather than relying on which task the
    // default Triage sort happens to auto-select first.
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-2".to_string()));
    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("TASK-1  Selectable task").click();
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string())),
        "clicking a row should select it via the same backlog_view.selected_task \
         Board and List already share"
    );
}

// ---------------------------------------------------------------------
// Sub-issue indentation, always expanded
// ---------------------------------------------------------------------

#[test]
fn sub_issues_render_indented_and_always_expanded_with_no_collapse_affordance() {
    let parent = task("TASK-1", "Parent task", "To Do");
    let mut child = task("TASK-1.1", "Child task", "To Do");
    child.parent = Some("TASK-1".to_string());
    let mut app = tasks_app(vec![parent, child]);
    app.tasks_place.group_by = None;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_by_label("TASK-1  Parent task  [0/1]")
            .is_some(),
        "the parent row should show a computed child roll-up"
    );
    assert!(
        harness.query_by_label("TASK-1.1  Child task").is_some(),
        "the sub-issue should render without needing an expand click — always expanded"
    );
}

/// TASK-97 medic pass (MINOR finding, pinned): `list_body::flatten` only
/// nests a child under its parent when the parent is *also* in the current
/// frame's visible set (`list_body.rs`'s own module doc has the full
/// rationale). Filter the parent out while its child stays visible — a
/// `tasks_place.filters` predicate that only the child matches — and the
/// child must still render, promoted to a top-level row (no indentation),
/// not silently dropped because its parent doesn't match.
#[test]
fn orphaned_sub_issue_promotes_to_a_top_level_row() {
    let parent = task("TASK-1", "Parent task", "In Progress");
    let mut child = task("TASK-1.1", "Child task", "To Do");
    child.parent = Some("TASK-1".to_string());
    let sibling = task("TASK-2", "Unrelated top-level task", "To Do");
    let mut app = tasks_app(vec![parent, child, sibling]);
    app.tasks_place.group_by = None;
    app.tasks_place.filters = vec![FilterPredicate {
        field: TaskField::Status,
        value: "To Do".to_string(),
    }];
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("TASK-1  Parent task").is_none(),
        "precondition: the Status: To Do predicate filters out the In Progress parent"
    );
    let child_row = harness.get_by_label("TASK-1.1  Child task");
    let sibling_row = harness.get_by_label("TASK-2  Unrelated top-level task");
    let left_gap = (child_row.rect().left() - sibling_row.rect().left()).abs();
    assert!(
        // Sub-pixel layout jitter between rows (checkbox/text-width
        // rounding) is well under a point; one real indent level is
        // `list::TREE_INDENT` = 20.0 — the gap between "same depth" and
        // "one level indented" is wide enough that a small tolerance can't
        // hide a real regression.
        left_gap < 4.0,
        "an orphaned child (its parent filtered out) should render at the \
         same left edge as any other top-level row, not indented under a \
         parent that isn't on screen (gap={left_gap})"
    );
}

// ---------------------------------------------------------------------
// List/Board share facet state
// ---------------------------------------------------------------------

#[test]
fn switching_to_board_view_mode_keeps_the_same_scope_and_filters() {
    let mut app = tasks_app(vec![
        task("TASK-1", "In progress task", "In Progress"),
        task("TASK-2", "To do task", "To Do"),
    ]);
    app.tasks_place.filters = vec![FilterPredicate {
        field: TaskField::Status,
        value: "In Progress".to_string(),
    }];
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.query_by_label("Board").is_some(),
        "precondition: the segmented control renders"
    );

    harness.get_by_label("Board").click();
    harness.run();

    assert_eq!(harness.state().tasks_place.view_mode, TasksViewMode::Board);
    assert!(
        harness
            .query_all_by_label("In progress task")
            .next()
            .is_some(),
        "the same Status: In Progress filter should still apply in Board mode"
    );
    assert!(harness.query_all_by_label("To do task").next().is_none());
}

#[test]
fn group_by_combo_is_disabled_in_board_view_mode() {
    let mut app = tasks_app(vec![task("TASK-1", "A", "To Do")]);
    app.tasks_place.view_mode = TasksViewMode::Board;
    let mut harness = harness(app);
    harness.run();

    let group_by = harness.get_by_label("Group by");
    assert!(
        group_by.accesskit_node().is_disabled(),
        "Group by is List-only — Board keeps its own status columns"
    );
}
