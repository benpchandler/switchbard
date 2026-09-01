//! IA V2 parity-audit evidence (TASK-76) — screenshot fixtures for states
//! the frozen design mock (`~/.lavish/switchbard-ia-places.html`) shows but
//! the existing `qa_screenshots.rs` / `qa_screenshots_tasks_place.rs`
//! fixtures don't cover: the Digest place with all three §1 goal cards, a
//! Tasks-place group header whose goal-pace chip and meter come from an
//! attached `GoalInputs` project (mock §3), the Command place's needs-you
//! row + support-request card (mock §2c), and the Dispatches table view
//! (mock §2b). Same machinery and posture as `qa_screenshots.rs` (`wgpu`,
//! real GPU adapter, `#[ignore]`d — pixel output is GPU/driver/font
//! sensitive, not part of the CI gate).
//!
//! Two parity-audit passes each wrote an overlapping fixture for Command's
//! needs-you state; this file keeps the one seeded directly onto app state
//! (matching `legibility_audit.rs`'s own `seed_command_fleet` pattern, with
//! its own `assert!`s that the needs-you facet, the Respond affordance, and
//! the support-request card actually rendered before the screenshot counts
//! as evidence) rather than the click-driven duplicate — one harness per
//! state, not two competing ones.
//!
//! Note: `ops_table_with_squatter` was deliberately NOT added — the
//! existing `ops_table_populated_{light,dark}.png` fixture
//! (`qa_screenshots.rs`'s `ops_screenshot_app`) already includes the
//! external squatter row (mock §6's bottom row).
//!
//! Regenerate with:
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test -p switchbard-gui --test qa_screenshots_ia_parity -- --ignored
//! ```

mod common;

use std::path::{Path, PathBuf};

use common::{harness, seeded_app, REPO_PATH};
use egui_kittest::kittest::{self, NodeT, Queryable};
use egui_kittest::{Harness, SnapshotOptions};
use switchbard_core::config::ThemeChoice;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun, DispatchRunLiveness};
use switchbard_core::{
    AgentProcessKind, AgentSession, BacklogChecklistItem, BacklogRepo, BacklogTask,
    BacklogTaskSource, GoalDef, GoalInputs, GoalMeasure, GoalWeek, ProjectDef, DISPATCHING_LABEL,
    DISPATCH_FAILED_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{Place, TasksView};

fn output_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/qa/screenshots")
}

fn snapshot(harness: &mut Harness<'_, HiveApp>, name: &str) {
    let options = SnapshotOptions::new().output_path(output_dir());
    // Intentionally ignore the Result — same posture as `qa_screenshots.rs`:
    // the `.png` this writes is the artifact; the diff verdict is noise.
    let _ = harness.try_snapshot_options(name, &options);
}

/// This week's Monday, the key `GoalDef::weeks` is bucketed by.
fn this_week() -> String {
    switchbard_core::week_monday_of(chrono::Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string()
}

fn task(id: &str, title: &str, status: &str, project: Option<&str>) -> BacklogTask {
    BacklogTask {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        priority: "medium".to_string(),
        assignees: vec!["ben".to_string()],
        labels: vec!["demo".to_string()],
        dependencies: vec![],
        references: vec![],
        project: project.map(str::to_string),
        parent: None,
        created_date: Some("2026-06-01 09:00".to_string()),
        updated_date: Some("2026-06-20 12:00".to_string()),
        description: String::new(),
        implementation_plan: String::new(),
        implementation_notes: String::new(),
        final_summary: String::new(),
        acceptance_criteria: vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "Criterion".to_string(),
        }],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!(
            "{REPO_PATH}/backlog/tasks/{}.md",
            id.to_lowercase()
        )),
    }
}

fn repo_with(
    tasks: Vec<BacklogTask>,
    project_defs: Vec<ProjectDef>,
    goals: Vec<GoalDef>,
) -> BacklogRepo {
    BacklogRepo {
        root: PathBuf::from(REPO_PATH),
        tasks,
        warnings: vec![],
        project_defs,
        initiative_defs: vec![],
        goals,
        ranking: switchbard_core::RepoRanking::default(),
        loaded_at_unix: 0,
        configured_statuses: vec![
            "To Do".into(),
            "In Progress".into(),
            "In Review".into(),
            "Done".into(),
        ],
    }
}

/// A manual-measure weekly goal with one check-in — `actual`/`target` drive
/// the pace pill and meter fill directly (same helper shape as
/// `qa_screenshots.rs`'s `goal_def`).
fn manual_goal(name: &str, target: i64, actual: i64) -> GoalDef {
    let week = this_week();
    GoalDef {
        name: name.to_string(),
        unit: "tasks".to_string(),
        measure: GoalMeasure::Manual,
        scope: None,
        inputs: GoalInputs::default(),
        weeks: std::collections::BTreeMap::from([(
            week.clone(),
            GoalWeek {
                target,
                checkins: vec![switchbard_core::GoalCheckIn {
                    date: week,
                    value: actual,
                }],
            },
        )]),
    }
}

// ─── 1. Digest place with all three mock-§1 goal cards, compact grid ───────

/// Mock §1 shows three compact goal-card tiles at once, 3-across: "Close out
/// Stack Ranking" 1/4 (behind), "IA decision record" 0/1 (behind), "Dispatch
/// throughput" 7/8 (on-track) — the TASK-76 parity pass's compact grid
/// (`ui::backlog::digest::render_compact_goal_grid`), not the full-width
/// editable row the Goals place index still uses.
fn digest_three_goals(theme: ThemeChoice, suffix: &str) {
    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Digest;
    let dispatching = {
        let mut t = task("TASK-83", "Dispatching now", "To Do", None);
        t.labels = vec![DISPATCHING_LABEL.to_string()];
        t
    };
    let in_progress = task("TASK-76", "In-progress task", "In Progress", None);
    let mut repo = repo_with(vec![dispatching, in_progress], vec![], vec![]);
    repo.goals = vec![
        manual_goal("Close out Stack Ranking", 4, 1),
        manual_goal("IA decision record", 1, 0),
        manual_goal("Dispatch throughput", 8, 7),
    ];
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), repo);
    let mut h = harness(app);
    h.run();
    // All three §1 goal cards must actually be on screen before this
    // screenshot counts as evidence.
    for name in [
        "Close out Stack Ranking",
        "IA decision record",
        "Dispatch throughput",
    ] {
        assert!(
            h.query_by_label(name).is_some(),
            "goal card '{name}' missing from the Digest render"
        );
    }
    snapshot(&mut h, &format!("digest_place_three_goals{suffix}"));
}

// ─── 2. Tasks place: expanded group header with a GoalInputs-attached goal ─

/// Mock §3's expanded Stack Ranking header carries a real progress meter and
/// a "goal: behind · 1/4" chip. The existing `tasks_place_header_expanded`
/// fixture reaches that chip through the goal's `scope` field; this shot
/// proves the *other* attachment path — TASK-92's `goal attach`, i.e.
/// `GoalDef::inputs.projects` — feeds the same chip and meter
/// (`groups.rs`'s `goal_chip_for_project` matches on either). One member
/// task is Done this week so the tasks-measured actual reads 1 of 4.
fn tasks_grouped_header_goal_chip(theme: ThemeChoice, suffix: &str) {
    let mut done_this_week = task("TASK-3", "Reorder controls", "Done", Some("Stack Ranking"));
    done_this_week.updated_date = Some(format!("{} 12:00", this_week()));
    let repo = repo_with(
        vec![
            task(
                "TASK-1",
                "Ship the rank CLI",
                "In Progress",
                Some("Stack Ranking"),
            ),
            task("TASK-2", "GUI sort by rank", "To Do", Some("Stack Ranking")),
            done_this_week,
            task("TASK-4", "Reorder polish", "To Do", Some("Stack Ranking")),
            task("TASK-5", "Lavish mockup", "In Progress", Some("IA V2")),
        ],
        vec![ProjectDef {
            name: "Stack Ranking".to_string(),
            status: "In Progress".to_string(),
            target_date: None,
            initiative: None,
            lead: None,
            description: "One global rank order across the backlog.".to_string(),
            path: PathBuf::from(format!("{REPO_PATH}/backlog/projects/Stack-Ranking.md")),
        }],
        vec![GoalDef {
            name: "Close out Stack Ranking".to_string(),
            unit: "tasks".to_string(),
            measure: GoalMeasure::Tasks,
            scope: None,
            inputs: GoalInputs {
                tasks: vec![],
                projects: vec!["Stack Ranking".to_string()],
            },
            weeks: std::collections::BTreeMap::from([(
                this_week(),
                GoalWeek {
                    target: 4,
                    checkins: vec![],
                },
            )]),
        }],
    );
    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Tasks;
    app.tasks_view = TasksView::All;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), repo);
    app.tasks_place
        .expanded_groups
        .insert("Stack Ranking".to_string());
    let mut h = harness(app);
    h.run();
    // The chip only exists if `goal_chip_for_project` matched via
    // `inputs.projects` (the goal has no `scope`) — its absence would mean
    // this screenshot silently doesn't show the state it's named for.
    assert!(
        h.query_all(kittest::by())
            .flat_map(|node| [node.accesskit_node().label(), node.value()])
            .flatten()
            .any(|text| text.starts_with("goal: ")),
        "no goal-pace chip rendered on the expanded group header"
    );
    snapshot(
        &mut h,
        &format!("tasks_place_grouped_header_expanded_goal_chip{suffix}"),
    );
}

// ─── 3. Command place: needs-you row + support-request card ────────────────

/// Mock §2c: an agent row flagged NEEDS YOU with a Respond affordance, and
/// the support-request card below the table explaining the blast radius.
/// Wired exactly like `command_fleet.rs`'s
/// `needs_you_facet_and_support_card_respond_deep_links_to_the_task` (a
/// dispatch-failed task + its dead run) plus `legibility_audit.rs`'s
/// `seed_command_fleet` selection (`CommandRowKey::Dispatch`) so the card
/// renders without a click; an in-flight dispatch and an interactive
/// session fill out the rest of the fleet table.
fn command_needs_you(theme: ThemeChoice, suffix: &str) {
    let failed = {
        let mut t = task("TASK-8", "Stack rank persistence", "In Progress", None);
        t.labels = vec![DISPATCH_FAILED_LABEL.to_string()];
        t.implementation_notes = "Dispatch failed: claude exited with 1".to_string();
        t
    };
    let running = {
        let mut t = task("TASK-6", "Rank CLI verbs", "In Progress", None);
        t.labels = vec![DISPATCHING_LABEL.to_string()];
        t
    };
    let repo = repo_with(vec![failed, running], vec![], vec![]);

    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Command;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), repo);
    let mut runs = app.dispatch_runs.lock().unwrap();
    runs.insert(
        (PathBuf::from(REPO_PATH), "TASK-8".to_string()),
        DispatchRun {
            task_id: "TASK-8".to_string(),
            branch: "dispatch/task-8".to_string(),
            worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-task-8")),
            worktree_exists: false,
            log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch-task-8.log")),
            prompt_path: None,
            started_at_unix: Some(now_unix().saturating_sub(360)),
            log_bytes: 900,
            log_modified_unix: Some(now_unix().saturating_sub(300)),
            liveness: DispatchRunLiveness::NoSidecar,
            progress: switchbard_core::dispatch_inspect::RunProgress::default(),
        },
    );
    runs.insert(
        (PathBuf::from(REPO_PATH), "TASK-6".to_string()),
        DispatchRun {
            task_id: "TASK-6".to_string(),
            branch: "dispatch/task-6".to_string(),
            worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-task-6")),
            worktree_exists: true,
            log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch-task-6.log")),
            prompt_path: None,
            started_at_unix: Some(now_unix().saturating_sub(840)),
            log_bytes: 0,
            log_modified_unix: None,
            liveness: DispatchRunLiveness::Alive {
                pgid: 4242,
                supervised: true,
            },
            progress: switchbard_core::dispatch_inspect::RunProgress::default(),
        },
    );
    drop(runs);
    *app.agent_sessions.lock().unwrap() = vec![AgentSession {
        pid: 5150,
        kind: AgentProcessKind::Claude,
        repo_name: Some(common::REPO_NAME.to_string()),
        worktree_path: Some(PathBuf::from(format!("{REPO_PATH}/.worktrees/feature-x"))),
        worktree_branch: Some("feature/stack-ranking-core".to_string()),
        started_unix: Some(now_unix().saturating_sub(900)),
        pgid: Some(5150),
    }];
    app.command_view.selected = Some(switchbard_gui::runtime::CommandRowKey::Dispatch((
        PathBuf::from(REPO_PATH),
        "TASK-8".to_string(),
    )));
    let mut h = harness(app);
    h.run();
    // The needs-you facet count, the row's Respond affordance, and the
    // support-request card must all be present for this to match mock §2c.
    assert!(
        h.query_by_label("Needs you · 1").is_some(),
        "no needs-you row derived from the failed dispatch"
    );
    assert!(
        h.query_all(kittest::by().label("Respond")).next().is_some(),
        "no Respond affordance rendered"
    );
    assert!(
        h.query_all(kittest::by())
            .flat_map(|node| [node.accesskit_node().label(), node.value()])
            .flatten()
            .any(|text| text.starts_with("Support request")),
        "support-request card missing despite the selected needs-you row"
    );
    snapshot(&mut h, &format!("command_place_needs_you{suffix}"));
}

// ─── 4. Dispatches table (mock §2b) ─────────────────────────────────────────

/// Mock §2b's aligned table (`Task | Status | Now doing | Elapsed | []`) —
/// the same underlying dispatch runs as [`command_needs_you`], viewed
/// task-scoped under Tasks / Dispatches instead of agent-scoped under
/// Command (this module's own doc: same runs, two axes).
fn dispatches_table(theme: ThemeChoice, suffix: &str) {
    let dispatching = {
        let mut t = task("TASK-83", "CLI: rank / expedite verbs", "In Progress", None);
        t.labels = vec![DISPATCHING_LABEL.to_string()];
        t
    };
    let failed = {
        let mut t = task(
            "TASK-61",
            "gh probe has no subprocess timeout",
            "To Do",
            None,
        );
        t.labels = vec![DISPATCH_FAILED_LABEL.to_string()];
        t.implementation_notes = "Dispatch failed: agent error after 22m".to_string();
        t
    };
    let repo = repo_with(vec![dispatching, failed], vec![], vec![]);

    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Tasks;
    app.tasks_view = TasksView::Dispatches;
    app.backlog_repos
        .lock()
        .unwrap()
        .insert(PathBuf::from(REPO_PATH), repo);
    {
        let mut runs = app.dispatch_runs.lock().unwrap();
        runs.insert(
            (PathBuf::from(REPO_PATH), "TASK-83".to_string()),
            DispatchRun {
                task_id: "TASK-83".to_string(),
                branch: "dispatch/task-83-rank-verbs".to_string(),
                worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-task-83")),
                worktree_exists: true,
                log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch-task-83.log")),
                prompt_path: None,
                started_at_unix: Some(now_unix().saturating_sub(14 * 60)),
                log_bytes: 400,
                log_modified_unix: Some(now_unix().saturating_sub(30)),
                liveness: DispatchRunLiveness::Alive {
                    pgid: 12345,
                    supervised: true,
                },
                progress: switchbard_core::dispatch_inspect::RunProgress::default(),
            },
        );
        runs.insert(
            (PathBuf::from(REPO_PATH), "TASK-61".to_string()),
            DispatchRun {
                task_id: "TASK-61".to_string(),
                branch: "dispatch/task-61-gh-timeout".to_string(),
                worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-task-61")),
                worktree_exists: false,
                log_path: Some(PathBuf::from("/tmp/switchbard-logs/dispatch-task-61.log")),
                prompt_path: None,
                started_at_unix: Some(now_unix().saturating_sub(22 * 60)),
                log_bytes: 900,
                log_modified_unix: Some(now_unix().saturating_sub(1200)),
                liveness: DispatchRunLiveness::NoSidecar,
                progress: switchbard_core::dispatch_inspect::RunProgress::default(),
            },
        );
    }
    app.dispatches_view.selected = Some((PathBuf::from(REPO_PATH), "TASK-83".to_string()));
    let mut h = harness(app);
    h.run();
    assert!(
        h.query_by_label("Task").is_some(),
        "Dispatches table header missing"
    );
    snapshot(&mut h, &format!("dispatches_view{suffix}"));
}

fn shots_for_theme(theme: ThemeChoice) {
    let suffix = format!("_{theme:?}").to_lowercase();
    digest_three_goals(theme, &suffix);
    tasks_grouped_header_goal_chip(theme, &suffix);
    command_needs_you(theme, &suffix);
    dispatches_table(theme, &suffix);
}

#[test]
#[ignore = "pixel screenshots — GPU/driver/font sensitive, run explicitly (see module doc)"]
fn ia_parity_screenshots_both_themes() {
    shots_for_theme(ThemeChoice::Light);
    shots_for_theme(ThemeChoice::Dark);
}
