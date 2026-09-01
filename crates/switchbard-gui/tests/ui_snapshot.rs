//! Opt-in pixel snapshots of the Agents view, for visual-regression work.
//!
//! This is `#[ignore]`d on purpose. It renders through `wgpu` (a real GPU
//! adapter) and compares against a committed PNG baseline — which is sensitive
//! to GPU/driver/font differences across machines, so it is **not** part of the
//! CI gate. CI still *compiles* it (catching API breakage); it just doesn't run
//! it. The accesskit interaction tests in `ui_views.rs` are the durable,
//! deterministic layer.
//!
//! Workflow when you want visual regression locally — create/refresh the
//! baseline, then validate against it:
//!
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test -p switchbard-gui --test ui_snapshot -- --ignored
//! cargo test -p switchbard-gui --test ui_snapshot -- --ignored
//! ```
//!
//! Baselines land in `tests/snapshots/<name>.png`; the `.new.png` / `.diff.png`
//! outputs are gitignored.

mod common;

use std::path::PathBuf;

use common::{harness, seeded_app, REPO_NAME, REPO_PATH};
use switchbard_core::config::ThemeChoice;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun, DispatchRunLiveness, RunProgress};
use switchbard_core::{
    AgentHook, AgentKind, AgentProcessKind, AgentSession, BacklogRepo, BacklogTask,
    BacklogTaskSource, ContextScope, RepoRanking, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{AgentsSection, CommandRowKey, Place, TasksView};

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn agents_context_view_snapshot() {
    let mut app = seeded_app();
    app.place = Place::Command;
    // TASK-98: Command's default section is now Fleet, not Context — this
    // snapshot is specifically of the Context section.
    app.agent_context_view.section = AgentsSection::Context;
    let mut harness = harness(app);
    harness.run();
    harness.snapshot("agent_context_view");
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn agents_hooks_view_snapshot() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    let hooks = vec![
        AgentHook {
            id: "global-pre-tool".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Global,
            source_path: PathBuf::from("/Users/demo/.claude/settings.json"),
            event: "PreToolUse".to_string(),
            matcher: Some("Write|Edit".to_string()),
            hook_type: "command".to_string(),
            action: "python3".to_string(),
            arguments: vec!["/Users/demo/.claude/hooks/check-write.py".to_string()],
            condition: Some("Edit(*.rs)".to_string()),
            asynchronous: false,
            timeout_seconds: Some(30),
        },
        AgentHook {
            id: "repo-stop".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from("/tmp/switchbard-ui-test/demo/.claude/settings.local.json"),
            event: "Stop".to_string(),
            matcher: None,
            hook_type: "command".to_string(),
            action: "./scripts/rebuild-and-reload.sh".to_string(),
            arguments: Vec::new(),
            condition: None,
            asynchronous: true,
            timeout_seconds: None,
        },
    ];
    app.agent_contexts
        .lock()
        .expect("invariant: seeded agent context cache")
        .values_mut()
        .next()
        .expect("invariant: seeded worktree context")
        .hooks = hooks;
    let mut harness = harness(app);
    harness.run();
    harness.snapshot("agents_hooks_view");
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn agents_hooks_active_filters_narrow_snapshot() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    app.agent_context_view.hook_scope = Some(ContextScope::Local);
    app.agent_context_view.hook_event = Some("PostToolUse".to_string());
    *app.filter_mut() = "after-edit".to_string();
    app.agent_contexts
        .lock()
        .expect("invariant: seeded agent context cache")
        .values_mut()
        .next()
        .expect("invariant: seeded worktree context")
        .hooks = vec![AgentHook {
        id: "repo-post-tool".to_string(),
        agent: AgentKind::Claude,
        scope: ContextScope::Local,
        source_path: PathBuf::from("/tmp/switchbard-ui-test/demo/.claude/settings.local.json"),
        event: "PostToolUse".to_string(),
        matcher: Some("Write|Edit".to_string()),
        hook_type: "command".to_string(),
        action: "./scripts/after-edit-with-a-long-unbroken-action-name.sh".to_string(),
        arguments: Vec::new(),
        condition: None,
        asynchronous: false,
        timeout_seconds: Some(30),
    }];
    let mut harness = harness(app);
    harness.set_size(eframe::egui::vec2(720.0, 620.0));
    harness.run();
    harness.snapshot("agents_hooks_active_filters_narrow");
}

// ─── TASK-98: Dispatches + Command ──────────────────────────────────────────

fn task_in_flight() -> BacklogTask {
    BacklogTask {
        id: "TASK-83".to_string(),
        title: "rank / expedite verbs".to_string(),
        status: "In Progress".to_string(),
        priority: "high".to_string(),
        assignees: vec![],
        labels: vec![DISPATCHING_LABEL.to_string()],
        dependencies: vec![],
        references: vec![],
        project: None,
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
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-83.md")),
    }
}

fn task_failed() -> BacklogTask {
    BacklogTask {
        id: "TASK-61".to_string(),
        title: "gh timeout retry".to_string(),
        status: "In Progress".to_string(),
        priority: "medium".to_string(),
        assignees: vec![],
        labels: vec![DISPATCH_FAILED_LABEL.to_string()],
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: None,
        updated_date: None,
        description: String::new(),
        implementation_plan: String::new(),
        implementation_notes: "Dispatch failed: claude exited with 1".to_string(),
        final_summary: String::new(),
        acceptance_criteria: vec![],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-61.md")),
    }
}

fn seed_two_dispatch_tasks(app: &HiveApp) {
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![task_in_flight(), task_failed()],
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
    let log_path = PathBuf::from(REPO_PATH).join("task-83-dispatch.log");
    let _ = std::fs::create_dir_all(PathBuf::from(REPO_PATH));
    let _ = std::fs::write(
        &log_path,
        "$ cargo test -p switchbard-core rank\ntest rank::create_places_at_tail ... ok\n",
    );
    let mut runs = app.dispatch_runs.lock().unwrap();
    runs.insert(
        (PathBuf::from(REPO_PATH), "TASK-83".to_string()),
        DispatchRun {
            task_id: "TASK-83".to_string(),
            branch: "dispatch/task-83-rank-verbs".to_string(),
            worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-task-83")),
            worktree_exists: true,
            log_path: Some(log_path),
            prompt_path: None,
            started_at_unix: Some(now_unix() - 840),
            log_bytes: 0,
            log_modified_unix: None,
            liveness: DispatchRunLiveness::Alive {
                pgid: 4242,
                supervised: true,
            },
            progress: RunProgress::default(),
        },
    );
    runs.insert(
        (PathBuf::from(REPO_PATH), "TASK-61".to_string()),
        DispatchRun {
            task_id: "TASK-61".to_string(),
            branch: "dispatch/task-61-gh-timeout".to_string(),
            worktree_path: PathBuf::from(format!("{REPO_PATH}/.worktrees/dispatch-task-61")),
            worktree_exists: false,
            log_path: Some(PathBuf::from("/tmp/switchbard-logs/task-61-dispatch.log")),
            prompt_path: None,
            started_at_unix: Some(now_unix() - 1_320),
            log_bytes: 900,
            log_modified_unix: Some(now_unix() - 1_260),
            liveness: DispatchRunLiveness::NoSidecar,
            progress: RunProgress::default(),
        },
    );
}

fn dispatches_running_app(theme: ThemeChoice) -> HiveApp {
    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Tasks;
    app.tasks_view = TasksView::Dispatches;
    seed_two_dispatch_tasks(&app);
    app.dispatches_view.selected = Some((PathBuf::from(REPO_PATH), "TASK-83".to_string()));
    app
}

/// The Dispatches view (mock §2b): one in-flight run selected, its detail
/// card open (log tail + AC chips + SITREP age).
#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn dispatches_running_snapshot() {
    let mut harness = harness(dispatches_running_app(ThemeChoice::Light));
    harness.run();
    harness.snapshot("dispatches_running_light");
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn dispatches_running_snapshot_dark() {
    let mut harness = harness(dispatches_running_app(ThemeChoice::Dark));
    harness.run();
    harness.snapshot("dispatches_running_dark");
}

fn command_fleet_mixed_app(theme: ThemeChoice) -> HiveApp {
    let mut app = seeded_app();
    app.config.ui.theme = theme;
    app.place = Place::Command;
    seed_two_dispatch_tasks(&app);
    *app.agent_sessions.lock().unwrap() = vec![AgentSession {
        pid: 5150,
        kind: AgentProcessKind::Claude,
        repo_name: Some(REPO_NAME.to_string()),
        worktree_path: Some(PathBuf::from(format!("{REPO_PATH}/.worktrees/feature-x"))),
        worktree_branch: Some("feature/stack-ranking-core".to_string()),
        started_unix: Some(now_unix() - 900),
    }];
    app.command_view.selected = Some(CommandRowKey::Dispatch((
        PathBuf::from(REPO_PATH),
        "TASK-61".to_string(),
    )));
    app
}

/// The Command place's Fleet section (mock §2c): a dispatch-in-flight row, a
/// dispatch-failed (needs-you) row with its support card open, and one
/// interactive session.
#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn command_fleet_mixed_snapshot() {
    let mut harness = harness(command_fleet_mixed_app(ThemeChoice::Light));
    harness.run();
    harness.snapshot("command_fleet_mixed_light");
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn command_fleet_mixed_snapshot_dark() {
    let mut harness = harness(command_fleet_mixed_app(ThemeChoice::Dark));
    harness.run();
    harness.snapshot("command_fleet_mixed_dark");
}

/// Sanity check for the kill-confirm banner's own layout (`ui::dispatch::
/// render_kill_confirm_banner`) — not part of the required evidence set,
/// but worth a one-off pixel check given how much back-and-forth this row's
/// `right_to_left` action cluster took to get right.
#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn dispatches_kill_confirm_armed_snapshot() {
    let mut app = dispatches_running_app(ThemeChoice::Light);
    app.dispatch_kill_confirm = Some((PathBuf::from(REPO_PATH), "TASK-83".to_string()));
    let mut harness = harness(app);
    harness.run();
    harness.snapshot("dispatches_kill_confirm_armed");
}

/// Narrow-width design-state check (mock §4/§7d: the sidebar collapses to
/// an icon rail below `NARROW_WIDTH_THRESHOLD`) — both new place bodies'
/// row layouts (facet bar wrapping, the right-aligned action cluster) at a
/// width narrow enough to matter.
#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn dispatches_running_narrow_snapshot() {
    let mut harness = harness(dispatches_running_app(ThemeChoice::Light));
    harness.set_size(eframe::egui::vec2(640.0, 700.0));
    harness.run();
    harness.snapshot("dispatches_running_narrow");
}

#[test]
#[ignore = "wgpu image snapshot: machine-specific, run explicitly with `-- --ignored` (see module docs)"]
fn command_fleet_mixed_narrow_snapshot() {
    let mut harness = harness(command_fleet_mixed_app(ThemeChoice::Light));
    harness.set_size(eframe::egui::vec2(640.0, 700.0));
    harness.run();
    harness.snapshot("command_fleet_mixed_narrow");
}
