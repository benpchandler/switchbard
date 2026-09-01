//! TASK-97 medic pass (MAJOR finding) evidence: saved-view save/browse/
//! delete restored inside the Tasks place — its only prior UI surface,
//! `saved_views::render_saved_views_bar`, lived in the orphaned legacy
//! `ui::backlog::render` body that nothing routes to anymore. These tests
//! mount the real Tasks place (`common::harness`/`seeded_app`, same
//! discipline as `tests/tasks_place.rs`) and drive the actual restored bar,
//! plus the FAVORITES-sidebar apply path TASK-96 already wired
//! (`HiveApp::navigate_to_favorite`).
//!
//! Also covers the BLOCKER finding's second required regression: applying a
//! saved view lands its predicates on `tasks_place.filters` (a removable
//! chip), never the invisible legacy `backlog_view` facets — including the
//! translation fallback for a view saved before `SavedView::tasks_filters`
//! existed.

mod common;

use std::path::PathBuf;

use common::{harness, seeded_app, REPO_PATH};
use eframe::egui;
use egui_kittest::kittest::{self, Queryable};
use switchbard_core::config::{FavoriteKind, FavoriteRef, SavedFilterPredicate, SavedView};
use switchbard_core::{
    BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskSource, RepoRanking,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{Place, TasksView};
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

fn new_format_saved_view(name: &str) -> SavedView {
    SavedView {
        name: name.to_string(),
        selected_repo: None,
        status_filter: "all".to_string(),
        priority_filter: "all".to_string(),
        project_filter: "all".to_string(),
        label_filter: "all".to_string(),
        sort_key: String::new(),
        sort_direction: String::new(),
        lens: String::new(),
        show_completed: false,
        show_archived: false,
        show_drafts: true,
        tasks_filters: vec![SavedFilterPredicate {
            field: "priority".to_string(),
            value: "high".to_string(),
        }],
        tasks_group_by: "repo".to_string(),
        tasks_view_mode: "board".to_string(),
    }
}

// ---------------------------------------------------------------------
// Save round-trip (through the real, now-reachable "Save current view
// as…  ⏎" field)
// ---------------------------------------------------------------------

#[test]
fn saving_the_current_tasks_place_state_writes_a_named_saved_view() {
    let mut app = tasks_app(vec![task("TASK-1", "A", "In Progress")]);
    app.tasks_place.group_by = Some(TaskField::Status);
    app.tasks_place.view_mode = TasksViewMode::List;
    app.tasks_place.filters = vec![FilterPredicate {
        field: TaskField::Status,
        value: "In Progress".to_string(),
    }];
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness.state().config.ui.saved_views.is_empty(),
        "precondition: no saved views yet"
    );

    // The create-modal's own tests query by role + ordinal for the same
    // reason (see `backlog_controls.rs`'s comment): this bar's field is the
    // *last* singleline text input in the tree — the free-text search field
    // above it is the only other one on screen.
    let name_field = harness
        .query_all(kittest::by().role(egui::accesskit::Role::TextInput))
        .last()
        .expect("saved-view name field");
    name_field.focus();
    harness.state_mut().backlog_view.saved_view_name_draft = "My rank queue".to_string();
    harness.run();
    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Enter,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
        repeat: false,
        physical_key: None,
    });
    harness.run();

    let saved = harness
        .state()
        .config
        .ui
        .saved_views
        .iter()
        .find(|v| v.name == "My rank queue")
        .cloned()
        .expect("the typed name should have been saved");
    assert_eq!(
        saved.tasks_filters,
        vec![SavedFilterPredicate {
            field: "status".to_string(),
            value: "In Progress".to_string(),
        }]
    );
    assert_eq!(saved.tasks_group_by, "status");
    assert_eq!(saved.tasks_view_mode, "list");
    assert_eq!(
        harness.state().backlog_view.active_saved_view,
        Some("My rank queue".to_string())
    );
}

// ---------------------------------------------------------------------
// Apply from the FAVORITES sidebar (TASK-96's existing navigation path)
// ---------------------------------------------------------------------

#[test]
fn applying_a_saved_view_from_a_favorites_sidebar_click_restores_group_filters_and_view_mode() {
    let mut app = tasks_app(vec![task("TASK-1", "A", "To Do")]);
    app.config
        .ui
        .saved_views
        .push(new_format_saved_view("Repo view"));
    app.config.ui.favorites.push(FavoriteRef {
        kind: FavoriteKind::View,
        repo: String::new(),
        key: "Repo view".to_string(),
    });
    // Starts different from what the saved view holds, so the click is what
    // proves the change, not the fixture's own starting state.
    app.tasks_place.group_by = Some(TaskField::Project);
    app.tasks_place.filters.clear();
    app.tasks_place.view_mode = TasksViewMode::List;
    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("Repo view").click();
    harness.run();

    assert_eq!(harness.state().place, Place::Tasks);
    assert_eq!(harness.state().tasks_place.group_by, Some(TaskField::Repo));
    assert_eq!(harness.state().tasks_place.view_mode, TasksViewMode::Board);
    assert_eq!(
        harness.state().tasks_place.filters,
        vec![FilterPredicate {
            field: TaskField::Priority,
            value: "high".to_string(),
        }]
    );
}

// ---------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------

#[test]
fn deleting_the_active_saved_view_removes_it_and_clears_the_selection() {
    let mut app = tasks_app(vec![task("TASK-1", "A", "To Do")]);
    app.config
        .ui
        .saved_views
        .push(new_format_saved_view("Repo view"));
    app.backlog_view.active_saved_view = Some("Repo view".to_string());
    let mut harness = harness(app);
    harness.run();
    assert!(harness.query_by_label("Delete").is_some());

    harness.get_by_label("Delete").click();
    harness.run();

    assert!(harness.state().config.ui.saved_views.is_empty());
    assert!(harness.state().backlog_view.active_saved_view.is_none());
}

// ---------------------------------------------------------------------
// BLOCKER finding, second required regression: apply a saved view, see
// its chip, remove it, get every task back — including the legacy-facet
// translation fallback for a view saved before `tasks_filters` existed.
// ---------------------------------------------------------------------

#[test]
fn applying_a_saved_view_directly_lands_predicates_on_the_removable_chip_not_the_legacy_facet() {
    // Exercises `apply_saved_view_by_name` (the function both this bar's
    // combo and a FAVORITES click share) with a new-format view, proving the
    // fast path with no legacy-facet fallback involved.
    let mut app = tasks_app(vec![
        task("TASK-1", "In progress task", "In Progress"),
        task("TASK-2", "To do task", "To Do"),
    ]);
    app.tasks_place.filters.clear();
    app.config.ui.saved_views.push(SavedView {
        tasks_filters: vec![SavedFilterPredicate {
            field: "status".to_string(),
            value: "In Progress".to_string(),
        }],
        ..new_format_saved_view("Status view")
    });

    switchbard_gui::ui::backlog::apply_saved_view_by_name(&mut app, "Status view");

    assert_eq!(
        app.tasks_place.filters,
        vec![FilterPredicate {
            field: TaskField::Status,
            value: "In Progress".to_string(),
        }]
    );
    assert_eq!(app.backlog_view.status_filter, "all");
}

#[test]
fn applying_a_legacy_only_saved_view_then_removing_the_translated_chip_restores_every_task() {
    let mut app = tasks_app(vec![
        task("TASK-1", "In progress task", "In Progress"),
        task("TASK-2", "To do task", "To Do"),
    ]);
    app.tasks_place.filters.clear();
    app.place = Place::Digest; // starts elsewhere — the click must navigate
                               // A view saved before `SavedView::tasks_filters` existed: only the
                               // legacy single-value facets carry the predicate.
    app.config.ui.saved_views.push(SavedView {
        status_filter: "In Progress".to_string(),
        tasks_filters: Vec::new(),
        tasks_group_by: String::new(),
        tasks_view_mode: String::new(),
        ..new_format_saved_view("Legacy view")
    });
    app.config.ui.favorites.push(FavoriteRef {
        kind: FavoriteKind::View,
        repo: String::new(),
        key: "Legacy view".to_string(),
    });
    let mut harness = harness(app);
    harness.run();

    harness.get_by_label("Legacy view").click();
    harness.run();
    assert_eq!(harness.state().place, Place::Tasks);

    // The click's own frame still painted Digest; this second run is the
    // first one that renders the Tasks place with the translated predicate
    // applied.
    harness.run();
    assert!(harness.query_by_label("TASK-1  In progress task").is_some());
    assert!(
        harness.query_by_label("TASK-2  To do task").is_none(),
        "the translated Status: In Progress predicate should hide the To Do task"
    );
    assert!(harness.query_by_label("Status: In Progress ✕").is_some());

    harness.get_by_label("Status: In Progress ✕").click();
    harness.run();

    assert!(
        harness.query_by_label("TASK-2  To do task").is_some(),
        "removing the translated chip should restore every task"
    );
}
