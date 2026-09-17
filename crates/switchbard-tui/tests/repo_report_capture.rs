//! Real-key repository capture preserves scope through redirects, saving and retry.
mod harness;
use crossterm::event::KeyCode;
use harness::Harness;
use std::time::{Duration, Instant};
use switchbard_tui::app::Mode;

fn settle(h: &mut Harness, expected: usize) -> String {
    for _ in 0..600 {
        h.app.tick();
        if !h.app.report.is_pending() && h.app.total_tasks() == expected {
            return h.render();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("capture did not settle: {}", h.render());
}

fn redirected(h: &mut Harness) -> tempfile::TempDir {
    let tool = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tool.path().join("backlog/tasks")).unwrap();
    std::fs::write(
        tool.path().join("backlog/config.yml"),
        "project_name: tool\nstatuses: [\"To Do\", \"Done\"]\ntask_prefix: task\n",
    )
    .unwrap();
    std::fs::write(
        &h.config_path,
        format!(
            "return {{ report_repo = {:?} }}",
            tool.path().display().to_string()
        ),
    )
    .unwrap();
    h.app = harness::open_app(&h.root, &h.config_path);
    tool
}

#[test]
fn repository_keys_work_on_every_page_and_ignore_tool_redirect() {
    let mut h = Harness::new();
    let tool = redirected(&mut h);
    for page in 0..4 {
        for (key, kind) in [('i', "idea"), ('b', "bug")] {
            let screen = h.press(KeyCode::Char(key));
            assert_eq!(h.app.mode, Mode::Command, "{screen}");
            assert_eq!(h.app.input, format!("{kind} "));
            h.type_text(&format!("café 界 request page {page}"));
            h.press(KeyCode::Enter);
            let expected = 4 + page * 2 + usize::from(key == 'b');
            let screen = settle(&mut h, expected);
            assert!(screen.contains("filed TASK-"), "{screen}");
            let title = format!("{kind}: café 界 request page {page}");
            let task = h
                .app
                .tasks()
                .iter()
                .find(|task| task.title == title)
                .unwrap();
            assert_eq!(task.priority, "medium");
            assert_eq!(task.labels, vec![kind]);
            assert!(task.project.is_none());
            assert!(!task.acceptance_criteria[0]
                .text
                .contains("behaviour in sbt"));
        }
        h.press(KeyCode::Tab);
    }
    assert_eq!(
        std::fs::read_dir(tool.path().join("backlog/tasks"))
            .unwrap()
            .count(),
        0
    );
    h.type_text("tnordinary task");
    h.press(KeyCode::Enter);
    h.type_text("tb1");
    assert!(h
        .app
        .selected_task()
        .unwrap()
        .labels
        .iter()
        .any(|label| label == "ball:me"));
}

#[test]
fn repository_capture_uses_native_status_and_existing_bugs_project_only() {
    for has_bugs in [false, true] {
        let mut h = Harness::new();
        std::fs::write(h.root.join("backlog/config.yml"),
            "project_name: fixture\nstatuses: [\"Inbox\", \"Review\", \"Done\"]\ntask_prefix: task\n").unwrap();
        if has_bugs {
            harness::seed_project(&h.root, "Bugs", "Active", None);
        }
        h.app = harness::open_app(&h.root, &h.config_path);
        h.press(KeyCode::Char('b'));
        h.type_text("native repository defect");
        h.press(KeyCode::Enter);
        settle(&mut h, 4);
        let task = h
            .app
            .tasks()
            .iter()
            .find(|task| task.title == "bug: native repository defect")
            .unwrap();
        assert_eq!(task.status, "Inbox");
        assert_eq!(task.project.as_deref(), has_bugs.then_some("Bugs"));
        h.press(KeyCode::Char('i'));
        h.type_text("native repository idea");
        h.press(KeyCode::Enter);
        settle(&mut h, 5);
        let task = h
            .app
            .tasks()
            .iter()
            .find(|task| task.title == "idea: native repository idea")
            .unwrap();
        assert_eq!(task.status, "Inbox");
        assert!(task.project.is_none());
    }
}

#[test]
fn empty_cancelled_and_remapped_captures_do_not_write() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('i'));
    h.press(KeyCode::Enter);
    assert!(!h.app.report.is_pending());
    assert_eq!(h.app.total_tasks(), 3);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('b'));
    h.type_text("cancel this");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.mode, Mode::Browse);
    assert_eq!(h.app.total_tasks(), 3);
    std::fs::write(
        &h.config_path,
        "return { keys = { x = 'repo_idea', y = 'repo_bug' } }",
    )
    .unwrap();
    h.app = harness::open_app(&h.root, &h.config_path);
    h.press(KeyCode::Char('x'));
    assert_eq!(h.app.input, "idea ");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('y'));
    assert_eq!(h.app.input, "bug ");
    h.press(KeyCode::Esc);
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("repo_idea"), "{screen}");
    assert!(screen.contains("repo_bug"), "{screen}");
}

#[test]
fn failed_repository_draft_retries_same_target_despite_redirect_reload() {
    let mut h = Harness::new();
    let tool = redirected(&mut h);
    let lock = h.root.join(".switchbard-storage.lock");
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    h.press(KeyCode::Char('b'));
    h.type_text("retry café 界 here");
    h.press(KeyCode::Enter);
    for _ in 0..600 {
        h.app.tick();
        if !h.app.report.is_pending() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.render().contains("Report failed"));
    std::fs::remove_dir(&lock).unwrap();
    let second_tool = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(second_tool.path().join("backlog/tasks")).unwrap();
    std::fs::write(
        second_tool.path().join("backlog/config.yml"),
        "project_name: second tool\nstatuses: [\"To Do\", \"Done\"]\ntask_prefix: task\n",
    )
    .unwrap();
    std::fs::write(
        &h.config_path,
        format!(
            "return {{ report_repo = {:?} }}",
            second_tool.path().display().to_string()
        ),
    )
    .unwrap();
    h.app.tick();
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
    h.press(KeyCode::Char(':'));
    assert_eq!(h.app.input, "bug retry café 界 here");
    h.press(KeyCode::Enter);
    let screen = settle(&mut h, 4);
    assert!(screen.contains("filed TASK-4"), "{screen}");
    assert_eq!(
        std::fs::read_dir(tool.path().join("backlog/tasks"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        std::fs::read_dir(second_tool.path().join("backlog/tasks"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn queued_repository_draft_keeps_scope_while_first_save_completes() {
    let mut h = Harness::new();
    let tool = redirected(&mut h);
    h.press(KeyCode::Char('i'));
    h.type_text("first local idea");
    let root = h.root.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _lock = switchbard_core::storage::RepositoryLock::acquire(&root).unwrap();
        tx.send(()).unwrap();
        std::thread::sleep(Duration::from_secs(2));
    });
    rx.recv().unwrap();
    let started = Instant::now();
    h.press(KeyCode::Enter);
    assert!(started.elapsed() < Duration::from_millis(500));
    h.press(KeyCode::Tab);
    h.press(KeyCode::Char('b'));
    h.type_text("second local bug");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::Command);
    assert_eq!(h.app.input, "bug second local bug");
    holder.join().unwrap();
    settle(&mut h, 4);
    assert_eq!(h.app.input, "bug second local bug");
    h.press(KeyCode::Enter);
    settle(&mut h, 5);
    assert_eq!(
        std::fs::read_dir(tool.path().join("backlog/tasks"))
            .unwrap()
            .count(),
        0
    );
    assert!(h
        .app
        .tasks()
        .iter()
        .any(|task| task.title == "bug: second local bug"));
}
