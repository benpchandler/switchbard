//! TASK-44: the detail rail's "Refine" button — the grooming affordance that
//! sits beside Dispatch.
//!
//! Scope note, deliberately narrow. Clicking Refine spawns a thread that
//! shells out to a real headless `claude -p` run; driving that click here
//! would launch an agent against whatever repo path the fixture names, which
//! is categorically worse than the "fire-and-forget background thread"
//! problem `backlog_controls.rs`'s module doc already rules out for this
//! harness. So this file asserts what is observable *without* starting a run:
//! that the button renders for an editable task, that it disappears for a
//! read-only one, and that it disables itself while a run is in flight for
//! that exact task (`HiveApp::refining_tasks` — the same set
//! `spawn_backlog_refine` guards on, so the affordance and the guard cannot
//! disagree).
//!
//! The pure half of the feature — prompt, parse, additive merge — is proved
//! in `switchbard-core`'s `refine::tests`, and the CLI write it produces in
//! `switchbard-core/tests/backlog_mutations.rs`.

mod common;

use egui_kittest::kittest::NodeT;
use std::path::PathBuf;

use common::{harness, seeded_app, REPO_PATH};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use switchbard_core::{BacklogRepo, BacklogTask, BacklogTaskSource};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, ViewTab};

fn task(source: BacklogTaskSource) -> BacklogTask {
    BacklogTask {
        id: "TASK-1".to_string(),
        title: "Half-baked card".to_string(),
        status: "To Do".to_string(),
        priority: "high".to_string(),
        assignees: vec![],
        labels: vec![],
        dependencies: vec![],
        references: vec![],
        milestone: None,
        parent: None,
        created_date: Some("2026-08-19 09:00".to_string()),
        updated_date: Some("2026-08-19 09:00".to_string()),
        description: "Needs fleshing out.".to_string(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source,
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-1.md")),
    }
}

/// A detail rail showing one task, with the repo's `backlog` CLI reported
/// as available — `editable` in the detail pane is `task.editable() &&
/// repo`, and editability matters to this button.
fn rail_app(task: BacklogTask) -> HiveApp {
    let mut app = seeded_app();
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    app.backlog_view.show_archived = true;
    app.backlog_view.show_completed = true;
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), task.id.clone()));
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![task],
            warnings: vec![],
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    app
}

fn rail_harness(app: HiveApp) -> Harness<'static, HiveApp> {
    let mut harness = harness(app);
    harness.run();
    harness
}

#[test]
fn an_editable_task_offers_an_enabled_refine_button() {
    let harness = rail_harness(rail_app(task(BacklogTaskSource::Active)));

    assert!(
        !harness
            .get_by_label("Refine")
            .accesskit_node()
            .is_disabled(),
        "an editable task with the backlog CLI available should offer Refine"
    );
}

#[test]
fn refine_sits_beside_dispatch_in_the_detail_rail() {
    let harness = rail_harness(rail_app(task(BacklogTaskSource::Active)));

    // Both grooming and execution affordances render for the same task —
    // Refine is the step upstream of Dispatch, not a replacement for it.
    harness.get_by_label("Refine");
    harness.get_by_label("Dispatch");
}

#[test]
fn a_read_only_task_offers_no_refine_button_at_all() {
    let harness = rail_harness(rail_app(task(BacklogTaskSource::Archived)));

    assert!(
        harness.query_by_label("Refine").is_none(),
        "an archived task is not editable, so there is nothing to refine into"
    );
}

/// AC #4's "cannot stack" half, at the affordance level: a task already
/// being refined cannot be clicked into a second run.
#[test]
fn refine_disables_itself_while_a_run_is_in_flight_for_that_task() {
    let app = rail_app(task(BacklogTaskSource::Active));
    app.refining_tasks
        .lock()
        .unwrap()
        .insert((PathBuf::from(REPO_PATH), "TASK-1".to_string()));

    let harness = rail_harness(app);

    assert!(
        harness
            .get_by_label("Refine")
            .accesskit_node()
            .is_disabled(),
        "a second refine must be unclickable while the first is running"
    );
}

/// The in-flight guard is keyed on `(project_root, task_id)`, not on a bare
/// id — the same reason `BacklogTaskKey` exists at all. A different repo's
/// TASK-1 must stay refinable.
#[test]
fn an_in_flight_run_in_another_project_does_not_disable_this_ones_button() {
    let app = rail_app(task(BacklogTaskSource::Active));
    app.refining_tasks
        .lock()
        .unwrap()
        .insert((PathBuf::from("/tmp/some-other-repo"), "TASK-1".to_string()));

    let harness = rail_harness(app);

    assert!(!harness
        .get_by_label("Refine")
        .accesskit_node()
        .is_disabled());
}

#[test]
fn is_refining_reports_only_the_keys_actually_in_flight() {
    let app = rail_app(task(BacklogTaskSource::Active));
    let key = (PathBuf::from(REPO_PATH), "TASK-1".to_string());

    assert!(!app.is_refining(&key));
    app.refining_tasks.lock().unwrap().insert(key.clone());
    assert!(app.is_refining(&key));
    app.refining_tasks.lock().unwrap().remove(&key);
    assert!(!app.is_refining(&key));
}
