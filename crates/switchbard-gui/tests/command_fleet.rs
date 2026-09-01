//! TASK-98 — Dispatches facets and the Command fleet console, driven through
//! real widgets in `egui_kittest`.
//!
//! `dispatch_operability.rs` already covers the Kill control end to end
//! (arming, confirming, cancelling, every liveness verdict). This file
//! covers what TASK-98 adds on top: the Dispatches facet bar actually
//! changes which rows are visible when clicked (not just when set
//! programmatically); the Command place's Fleet section unions dispatch and
//! interactive rows, facets between them, derives "needs you", and its
//! support card's Respond action deep-links to the task; and the
//! Fleet/Context/Hooks section switcher is reachable by a real click, not
//! just by setting `agent_context_view.section` directly.

mod common;

use std::path::PathBuf;

use common::{harness, seeded_app, REPO_PATH};
use egui_kittest::kittest::{self, NodeT, Queryable};
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun, DispatchRunLiveness};
use switchbard_core::{
    AgentProcessKind, AgentSession, BacklogRepo, BacklogTask, BacklogTaskSource, RepoRanking,
    DISPATCHING_LABEL, DISPATCH_FAILED_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{AgentsSection, CommandFacet, DispatchesFacet, Place, TasksView};

fn task(id: &str, labels: &[&str], notes: &str) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: format!("{id} work"),
        status: "In Progress".to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: labels.iter().map(|l| l.to_string()).collect(),
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: None,
        updated_date: None,
        description: String::new(),
        implementation_plan: String::new(),
        implementation_notes: notes.to_string(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!(
            "{REPO_PATH}/backlog/tasks/{}.md",
            id.to_lowercase()
        )),
    }
}

fn run_with(task_id: &str, age_secs: u64, liveness: DispatchRunLiveness) -> DispatchRun {
    DispatchRun {
        task_id: task_id.to_string(),
        branch: format!("dispatch/{}", task_id.to_lowercase()),
        worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-{task_id}")),
        worktree_exists: false,
        log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch.log")),
        prompt_path: None,
        started_at_unix: Some(now_unix().saturating_sub(age_secs)),
        log_bytes: 0,
        log_modified_unix: None,
        liveness,
        progress: switchbard_core::dispatch_inspect::RunProgress::default(),
    }
}

fn app_with(tasks: Vec<BacklogTask>, runs: Vec<DispatchRun>) -> HiveApp {
    let app = seeded_app();
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
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
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    let mut cached = app.dispatch_runs.lock().unwrap();
    for run in runs {
        cached.insert((PathBuf::from(REPO_PATH), run.task_id.clone()), run);
    }
    drop(cached);
    app
}

// ─── Dispatches facet bar ──────────────────────────────────────────────────

/// A queued task is invisible under the default `Active` facet (see
/// `ui::places::dispatches::facet_for`) and appears once the `Queued` pill is
/// actually clicked — the real-widget counterpart to `dispatch_operability.
/// rs`'s tests, which set the facet by mutating state directly.
#[test]
fn clicking_a_dispatches_facet_pill_changes_the_visible_rows() {
    let mut app = app_with(vec![task("TASK-1", &["dispatch"], "")], vec![]);
    app.place = Place::Tasks;
    app.tasks_view = TasksView::Dispatches;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("TASK-1").is_none(),
        "a queued task must not show under the default Active facet"
    );
    assert!(harness.query_by_label("Queued · 1").is_some());

    harness.get_by_label("Queued · 1").click();
    harness.run();

    assert_eq!(
        harness.state().dispatches_view.facet,
        DispatchesFacet::Queued
    );
    assert!(
        harness.query_by_label("TASK-1").is_some(),
        "clicking the Queued pill should reveal the queued row"
    );
}

// ─── Command fleet ─────────────────────────────────────────────────────────

fn app_with_fleet(
    tasks: Vec<BacklogTask>,
    runs: Vec<DispatchRun>,
    sessions: Vec<AgentSession>,
) -> HiveApp {
    let app = app_with(tasks, runs);
    *app.agent_sessions.lock().unwrap() = sessions;
    app
}

fn interactive_session(pid: u32) -> AgentSession {
    AgentSession {
        pid,
        kind: AgentProcessKind::Claude,
        repo_name: None,
        worktree_path: Some(PathBuf::from(format!("{REPO_PATH}/.worktrees/feature-x"))),
        worktree_branch: Some("feature/x".to_string()),
        started_unix: Some(now_unix().saturating_sub(120)),
    }
}

/// Command's Fleet section (the new default) unions a dispatch-origin row
/// and an interactive-origin row under `All`, and each facet narrows to its
/// own source.
#[test]
fn fleet_section_unions_dispatch_and_interactive_rows_and_facets_between_them() {
    let mut app = app_with_fleet(
        vec![task("TASK-1", &[DISPATCHING_LABEL], "")],
        vec![run_with(
            "TASK-1",
            120,
            DispatchRunLiveness::Alive {
                pgid: 4242,
                supervised: true,
            },
        )],
        vec![interactive_session(999)],
    );
    app.place = Place::Command;
    let mut harness = harness(app);
    harness.run();

    // Default section is Fleet (TASK-98); default facet is All.
    assert!(harness.query_by_label("TASK-1 · TASK-1 work").is_some());
    assert!(harness.query_by_label("interactive session").is_some());
    assert!(harness.query_by_label("All · 2").is_some());

    harness.get_by_label("Dispatch · 1").click();
    harness.run();
    assert_eq!(harness.state().command_view.facet, CommandFacet::Dispatch);
    assert!(harness.query_by_label("TASK-1 · TASK-1 work").is_some());
    assert!(
        harness.query_by_label("interactive session").is_none(),
        "the Dispatch facet must hide the interactive row"
    );

    harness.get_by_label("Interactive · 1").click();
    harness.run();
    assert_eq!(
        harness.state().command_view.facet,
        CommandFacet::Interactive
    );
    assert!(
        harness.query_by_label("TASK-1 · TASK-1 work").is_none(),
        "the Interactive facet must hide the dispatch row"
    );
    assert!(harness.query_by_label("interactive session").is_some());
}

/// A failed dispatch row needs a human; the interactive row never does. The
/// support card renders only for the selected row once it is flagged, and
/// Respond deep-links to the task in Tasks/All — the same navigation
/// `HiveApp::navigate_to_favorite`'s Task branch uses.
#[test]
fn needs_you_facet_and_support_card_respond_deep_links_to_the_task() {
    let mut app = app_with_fleet(
        vec![task(
            "TASK-1",
            &[DISPATCH_FAILED_LABEL],
            "Dispatch failed: claude exited with 1",
        )],
        vec![run_with("TASK-1", 300, DispatchRunLiveness::NoSidecar)],
        vec![interactive_session(999)],
    );
    app.place = Place::Command;
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("Needs you · 1").is_some());
    harness.get_by_label("Needs you · 1").click();
    harness.run();
    assert!(harness.query_by_label("TASK-1 · TASK-1 work").is_some());
    assert!(
        harness.query_by_label("interactive session").is_none(),
        "an interactive session never needs a human"
    );

    // Select the failed row (clicking its own mission text hits the row's
    // click-sensing region — the same convention `board.rs` row selection
    // uses).
    harness.get_by_label("TASK-1 · TASK-1 work").click();
    harness.run();

    // Not an exact-seconds match, and not label-only: the fixture's elapsed
    // time is computed from real wall-clock `now_unix()` calls at
    // fixture-build time and again at render time, so an exact "5m 0s" is
    // one slow-CI-run away from ticking to "5m 1s" and flaking. A bare
    // `ui.label` also surfaces its text as the accesskit node's *value*,
    // not its *label* (see `dispatch_operability.rs`'s `text_containing`
    // doc), so this checks both the same way that helper does.
    assert!(harness
        .query_all(kittest::by())
        .flat_map(|node| [node.accesskit_node().label(), node.value()])
        .flatten()
        .any(|text| text.starts_with("Support request · claude · 5m")));
    assert!(harness
        .query_all(kittest::by())
        .filter_map(|node| node.value())
        .any(|text| text.contains("failed") && text.contains("claude exited with 1")));

    harness
        .get_all_by_label("Respond")
        .next()
        .expect("Respond renders on both the row and the support card")
        .click();
    harness.run();

    assert_eq!(harness.state().place, Place::Tasks);
    assert_eq!(harness.state().tasks_view, TasksView::All);
    assert_eq!(
        harness.state().backlog_view.selected_task,
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()))
    );
}

/// The Fleet/Context/Hooks switcher (TASK-98 adds Fleet) is reachable by a
/// real click, and Context/Hooks still render their pre-existing content —
/// TASK-96's decision record requirement that this place keep hosting them.
#[test]
fn the_section_switcher_reaches_fleet_context_and_hooks_by_click() {
    let mut app = seeded_app();
    app.place = Place::Command;
    let mut harness = harness(app);
    harness.run();

    // Fleet is the default landing section.
    assert_eq!(
        harness.state().agent_context_view.section,
        AgentsSection::Fleet
    );
    assert!(harness
        .query_by_label("the agent-scoped fleet console")
        .is_some());

    harness.get_by_label("Context").click();
    harness.run();
    assert_eq!(
        harness.state().agent_context_view.section,
        AgentsSection::Context
    );
    assert!(
        harness.query_by_label("CLAUDE.md").is_some(),
        "Context section should render the seeded instruction item"
    );

    harness.get_by_label("Hooks").click();
    harness.run();
    assert_eq!(
        harness.state().agent_context_view.section,
        AgentsSection::Hooks
    );
    assert!(harness
        .query_by_label("No configured hooks detected for Claude in this worktree.")
        .is_some());

    harness.get_by_label("Fleet").click();
    harness.run();
    assert_eq!(
        harness.state().agent_context_view.section,
        AgentsSection::Fleet
    );
}
