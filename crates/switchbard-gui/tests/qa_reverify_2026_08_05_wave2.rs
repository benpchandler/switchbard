//! Independent re-verification (by the original QA auditor) of fix wave 2
//! (commits `bdef9f7`, `4c8cf94`, `8aa2c17` on `feature/integration`), which
//! addressed the two defects (5 and 7) found while re-verifying fix wave 1:
//! the real `backlog` CLI refuses `task archive` on a Done task, so both
//! "Clean Up Old Tasks" and the single-task Archive button now route a Done
//! task through the new `complete_backlog_task` instead.
//!
//! Specifically targets what the fix wave's own tests didn't check: that a
//! completed task's file physically lands in `backlog/completed/` (not just
//! that `BacklogTaskSource::Completed` comes back from the parser — a
//! reparse could theoretically report the right source from the wrong
//! directory if `load_backlog_project`'s own directory-to-source mapping
//! were ever wrong), that visibility of a completed task is governed by the
//! same `show_completed` toggle as any other Done task (there is no
//! separate "completed source" filter in this codebase — confirmed by
//! reading `sort::task_visible`, which has no `BacklogTaskSource::Completed`
//! branch at all), and that a **non**-Done task's Archive path is
//! unaffected (own data, driven through the real detail-pane button, not
//! just the core function).

mod common;

use std::time::{Duration, Instant};

use common::{harness, isolated_config_save_path};
use egui_kittest::kittest::Queryable;
use switchbard_core::config::Config;
use switchbard_core::{BacklogTaskSource, Repo, WorktreeRef};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{BacklogLens, ViewTab};

fn run_cmd(cwd: &std::path::Path, cmd: &str, args: &[&str]) {
    let output = std::process::Command::new(cmd)
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {cmd}: {e}"));
    assert!(
        output.status.success(),
        "{cmd} {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_fixture_repo() -> tempfile::TempDir {
    let fixture = tempfile::tempdir().expect("create temp dir");
    let root = fixture.path();
    run_cmd(root, "git", &["init", "-q"]);
    run_cmd(root, "git", &["config", "user.email", "qa@example.com"]);
    run_cmd(root, "git", &["config", "user.name", "QA Fixture"]);
    run_cmd(
        root,
        "backlog",
        &["init", "--defaults", "--agent-instructions", "none", "qa"],
    );
    fixture
}

fn single_repo_app(root: &std::path::Path) -> HiveApp {
    let repos = vec![Repo {
        name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
    }];
    let worktrees = vec![WorktreeRef {
        repo_name: "qa-fixture".to_string(),
        path: root.to_path_buf(),
        branch: Some("main".to_string()),
        head: "abc1234".to_string(),
    }];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_project = Some(root.to_path_buf());
    app.backlog_projects.lock().unwrap().insert(
        root.to_path_buf(),
        switchbard_core::load_backlog_project(root).expect("load real fixture project"),
    );
    app
}

/// File location: `backlog task complete` on a Done task must land the
/// file physically under `backlog/completed/`, not just report the right
/// `BacklogTaskSource` from the parser (which could be right for the wrong
/// reason if the directory scan itself were the thing under test — it
/// isn't here, this checks the raw filesystem independent of
/// `load_backlog_project`).
#[test]
fn completing_a_done_task_moves_its_file_to_the_completed_directory_on_disk() {
    let fixture = init_fixture_repo();
    let root = fixture.path();
    run_cmd(root, "backlog", &["task", "create", "Finished work"]);
    run_cmd(root, "backlog", &["task", "edit", "TASK-1", "-s", "Done"]);

    let before_path = root.join("backlog/tasks/task-1 - Finished-work.md");
    assert!(
        before_path.is_file(),
        "sanity: the task file should exist under backlog/tasks/ before completing"
    );

    run_cmd(root, "backlog", &["task", "complete", "TASK-1"]);

    assert!(
        !before_path.exists(),
        "the file should no longer be under backlog/tasks/ after completing"
    );
    let completed_dir = root.join("backlog/completed");
    let completed_files: Vec<_> = std::fs::read_dir(&completed_dir)
        .unwrap_or_else(|e| panic!("backlog/completed/ should exist: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    assert_eq!(
        completed_files.len(),
        1,
        "expected exactly one completed task file on disk, found {completed_files:?}"
    );
    assert!(
        completed_files[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase()
            .contains("task-1"),
        "the completed file should be TASK-1's, got {:?}",
        completed_files[0]
    );
}

/// Visibility: a Completed-source task is governed by the same
/// `show_completed` toggle as any Done-status task — there is no separate
/// "completed source" filter (`sort::task_visible` has no
/// `BacklogTaskSource::Completed` branch; it hides on `task_is_completed()`,
/// which reads status, not source). Proves both directions: hidden by
/// default, visible once `show_completed` is on — with the actual real,
/// CLI-completed task, not a struct-constructed one.
#[test]
fn a_completed_task_is_visible_only_when_show_completed_is_on_same_as_any_done_task() {
    let fixture = init_fixture_repo();
    let root = fixture.path();
    run_cmd(root, "backlog", &["task", "create", "Finished work"]);
    run_cmd(root, "backlog", &["task", "edit", "TASK-1", "-s", "Done"]);
    run_cmd(root, "backlog", &["task", "complete", "TASK-1"]);
    run_cmd(root, "backlog", &["task", "create", "Still open"]);

    let project = switchbard_core::load_backlog_project(root).expect("reload");
    let completed_task = project
        .tasks
        .iter()
        .find(|t| t.id == "TASK-1")
        .expect("TASK-1 should still parse from backlog/completed/");
    assert_eq!(completed_task.source, BacklogTaskSource::Completed);

    let mut h = harness(single_repo_app(root));
    h.run();
    assert!(
        h.query_by_label("TASK-1  Finished work").is_none(),
        "a completed task should be hidden by default, same as any Done task"
    );
    assert!(h.query_by_label("TASK-2  Still open").is_some());

    h.state_mut().backlog_view.show_completed = true;
    h.run();
    assert!(
        h.query_by_label("TASK-1  Finished work").is_some(),
        "checking Done/show_completed should reveal the completed task, \
         same toggle as any other Done task — there is no separate \
         completed-source filter in this codebase"
    );
}

/// No regression: a non-Done task still gets a working Archive, driven
/// through the real detail-pane button (not the core function directly),
/// with the auditor's own fresh fixture — confirms the fix wave's
/// `task.is_done()` branch correctly falls through to the original Archive
/// path rather than always routing through Complete.
#[test]
fn a_non_done_task_still_gets_a_working_archive_button_no_regression() {
    let fixture = init_fixture_repo();
    let root = fixture.path();
    run_cmd(root, "backlog", &["task", "create", "Abandoned idea"]);
    run_cmd(
        root,
        "backlog",
        &["task", "edit", "TASK-1", "-s", "In Progress"],
    );

    let mut app = single_repo_app(root);
    app.backlog_view.selected_task = Some((root.to_path_buf(), "TASK-1".to_string()));
    let mut h = harness(app);
    h.run();

    // The non-Done task must offer "Archive", not "Complete".
    assert!(h.query_by_label("Archive").is_some());
    assert!(h.query_by_label("Complete").is_none());

    h.get_by_label("Archive").click();
    h.run();
    assert!(h.query_by_label("Archive TASK-1?").is_some());
    h.get_by_label("Confirm archive").click();
    h.run();
    assert_eq!(
        h.state().backlog_status.snapshot().as_deref(),
        Some("archiving TASK-1")
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.run();
        if h.state().backlog_status.snapshot().as_deref() == Some("archived TASK-1") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "archive's background thread did not report completion in time; last status: {:?}",
            h.state().backlog_status.snapshot()
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let project = switchbard_core::load_backlog_project(root).expect("reload");
    let task = project.tasks.iter().find(|t| t.id == "TASK-1").unwrap();
    assert_eq!(
        task.source,
        BacklogTaskSource::Archived,
        "a non-Done task's Archive button should still land it as Archived, not Completed"
    );
    assert!(
        root.join("backlog/archive/tasks")
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains("task-1")),
        "the file should physically be under backlog/archive/tasks/"
    );
}

/// Clean Up Old Tasks, real 2-repo fixture with the auditor's own mix of
/// Done/non-Done tasks (3 Done + 1 open in repo A, 1 Done + 1 open in repo
/// B — the fix-wave's own re-verification test uses exactly one Done task
/// per repo; this stresses more than one candidate per project), confirming
/// every Done task across both repos completes (not archives) and every
/// non-Done task is untouched.
#[test]
fn clean_up_old_tasks_completes_multiple_done_tasks_per_repo_across_two_real_repos() {
    let repo_a = init_fixture_repo();
    let repo_b = init_fixture_repo();

    for title in ["Done one", "Done two", "Done three"] {
        run_cmd(repo_a.path(), "backlog", &["task", "create", title]);
    }
    run_cmd(repo_a.path(), "backlog", &["task", "create", "Open in A"]);
    for id in ["TASK-1", "TASK-2", "TASK-3"] {
        run_cmd(
            repo_a.path(),
            "backlog",
            &["task", "edit", id, "-s", "Done"],
        );
    }

    run_cmd(repo_b.path(), "backlog", &["task", "create", "Done in B"]);
    run_cmd(repo_b.path(), "backlog", &["task", "create", "Open in B"]);
    run_cmd(
        repo_b.path(),
        "backlog",
        &["task", "edit", "TASK-1", "-s", "Done"],
    );

    let repos = vec![
        Repo {
            name: "qa-fixture-a".to_string(),
            path: repo_a.path().to_path_buf(),
        },
        Repo {
            name: "qa-fixture-b".to_string(),
            path: repo_b.path().to_path_buf(),
        },
    ];
    let worktrees = vec![
        WorktreeRef {
            repo_name: "qa-fixture-a".to_string(),
            path: repo_a.path().to_path_buf(),
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        },
        WorktreeRef {
            repo_name: "qa-fixture-b".to_string(),
            path: repo_b.path().to_path_buf(),
            branch: Some("main".to_string()),
            head: "def5678".to_string(),
        },
    ];
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    app.config_save_path = Some(isolated_config_save_path());
    app.view_tab = ViewTab::Backlog;
    app.backlog_view.lens = BacklogLens::List;
    for root in [repo_a.path(), repo_b.path()] {
        app.backlog_projects.lock().unwrap().insert(
            root.to_path_buf(),
            switchbard_core::load_backlog_project(root).expect("load real fixture project"),
        );
    }

    let mut h = harness(app);
    h.run();

    h.get_by_label("Clean Up Old Tasks").click();
    h.run();
    assert!(h.query_by_label("Complete 4 Done tasks?").is_some());
    h.get_by_label("Confirm cleanup").click();
    h.run();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.run();
        if h.state().backlog_status.snapshot().as_deref()
            == Some("cleaned up 4/4 Done tasks across 2 projects")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cleanup's background thread did not report completion in time; last status: {:?}",
            h.state().backlog_status.snapshot()
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let project_a = switchbard_core::load_backlog_project(repo_a.path()).unwrap();
    for id in ["TASK-1", "TASK-2", "TASK-3"] {
        let t = project_a.tasks.iter().find(|t| t.id == id).unwrap();
        assert_eq!(
            t.source,
            BacklogTaskSource::Completed,
            "{id} in repo A should be completed"
        );
    }
    let open_a = project_a.tasks.iter().find(|t| t.id == "TASK-4").unwrap();
    assert_eq!(
        open_a.source,
        BacklogTaskSource::Active,
        "the non-Done task in repo A must be untouched"
    );

    let project_b = switchbard_core::load_backlog_project(repo_b.path()).unwrap();
    let done_b = project_b.tasks.iter().find(|t| t.id == "TASK-1").unwrap();
    assert_eq!(done_b.source, BacklogTaskSource::Completed);
    let open_b = project_b.tasks.iter().find(|t| t.id == "TASK-2").unwrap();
    assert_eq!(open_b.source, BacklogTaskSource::Active);
}
