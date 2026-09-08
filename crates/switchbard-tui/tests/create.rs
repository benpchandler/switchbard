//! Ordinary task capture through real keys, rendering and on-disk persistence.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn creates_selects_and_persists_without_report_metadata() {
    let mut h = Harness::new();
    h.type_text("tn");
    h.type_text("Capture a task");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("created TASK-4"), "{screen}");
    assert_eq!(h.selected_title(), "Capture a task");
    let task = h.app.selected_task().unwrap();
    assert_eq!(task.status, "To Do");
    assert!(task.labels.is_empty());
    h.press(KeyCode::Enter);
    assert_eq!(h.app.total_tasks(), 4);
    let fresh = open_app(&h.root, &h.config_path);
    assert!(fresh.tasks().iter().any(|t| t.title == "Capture a task"));
}

#[test]
fn blank_is_recoverable_and_escape_cancels() {
    let mut h = Harness::new();
    h.type_text("tn");
    h.type_text("   ");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.to_lowercase().contains("title"), "{screen}");
    assert_eq!(h.app.total_tasks(), 3);
    h.type_text("cancel this");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.total_tasks(), 3);
    h.type_text("tn");
    h.type_text("Keep meX");
    h.press(KeyCode::Backspace);
    h.press(KeyCode::Enter);
    assert_eq!(h.selected_title(), "Keep me");
}

#[test]
fn failure_retains_draft_and_retry_creates_once() {
    let mut h = Harness::new();
    let tasks = h.root.join("backlog/tasks");
    let saved = h.root.join("saved-tasks");
    std::fs::rename(&tasks, &saved).unwrap();
    std::fs::write(&tasks, "blocked").unwrap();
    h.type_text("tn");
    h.type_text("Retry this title");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("Retry this title"), "{screen}");
    assert_eq!(h.app.input, "Retry this title");
    std::fs::remove_file(&tasks).unwrap();
    std::fs::rename(saved, tasks).unwrap();
    h.press(KeyCode::Enter);
    assert_eq!(h.selected_title(), "Retry this title");
    assert_eq!(h.app.total_tasks(), 4);
}

#[test]
fn filtered_creation_reports_hidden_without_changing_filter() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("label:auth");
    h.press(KeyCode::Enter);
    let filter = h.app.state.filter.clone();
    h.type_text("tn");
    h.type_text("Outside this filter");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("hidden"), "{screen}");
    assert_eq!(h.app.state.filter, filter);
    assert_eq!(h.app.total_tasks(), 4);
}

#[test]
fn empty_repo_custom_prefix_and_duplicate_titles() {
    let mut h = Harness::new();
    for entry in std::fs::read_dir(h.root.join("backlog/tasks")).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    std::fs::write(
        h.root.join("backlog/config.yml"),
        "project_name: fixture\nstatuses: [To Do, Done]\ntask_prefix: item\n",
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    for _ in 0..2 {
        h.type_text("tn");
        h.type_text("Same title");
        let screen = h.press(KeyCode::Enter);
        assert!(screen.contains("Same title"), "{screen}");
    }
    assert_eq!(h.app.total_tasks(), 2);
    assert_eq!(h.app.selected_task().unwrap().status, "To Do");
    assert_ne!(h.app.tasks()[0].id, h.app.tasks()[1].id);
}

#[test]
fn configurable_shortcut_and_help() {
    let mut h = Harness::new();
    std::fs::write(&h.config_path, "return { keys = { x = 'new_task' } }").unwrap();
    h.app.tick();
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("new_task"), "{screen}");
    h.press(KeyCode::Char('x'));
    h.type_text("Remapped capture");
    h.press(KeyCode::Enter);
    assert_eq!(h.selected_title(), "Remapped capture");
}

#[test]
fn unicode_input_survives_resize_tick_and_long_titles() {
    let mut h = Harness::new();
    h.type_text("tn");
    h.type_text(&format!("{}e\u{301}終", "界".repeat(80)));
    for (width, height) in [(40, 8), (100, 20), (160, 30)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        h.app.tick();
        let screen = h.render();
        assert!(screen.contains('終'), "{screen}");
    }
    h.press(KeyCode::Enter);
    assert!(h.selected_title().ends_with('終'));
    h.terminal = Terminal::new(TestBackend::new(0, 0)).unwrap();
    h.type_text("tn");
    assert_eq!(h.render(), "");
}

#[test]
fn bounded_input_and_external_creation_keep_capture_safe() {
    let mut h = Harness::new();
    h.type_text("tn");
    let screen = h.type_text(&"a".repeat(1100));
    assert!(screen.contains("limit"), "{screen}");
    assert!(h.app.input.len() <= 1024);
    seed(&h.root, "External addition", "To Do", &[]);
    h.app.tick();
    h.press(KeyCode::Enter);
    assert_eq!(h.app.total_tasks(), 5);
    assert_eq!(h.selected_title().len(), 1024);
}

#[test]
fn standing_hidden_status_reports_created_task() {
    let mut h = Harness::new();
    h.app
        .settings
        .edit_repo(|settings| settings.toggle_hidden("To Do"))
        .unwrap();
    h.type_text("tn");
    h.type_text("Hidden by settings");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("hidden"), "{screen}");
    assert_eq!(h.app.total_tasks(), 4);
}

#[test]
fn held_enter_does_not_open_detail_after_creating() {
    use crossterm::event::{KeyEvent, KeyEventKind, KeyModifiers};
    use switchbard_tui::app::Pane;
    let mut h = Harness::new();
    h.type_text("tn");
    h.type_text("One capture");
    h.press(KeyCode::Enter);
    h.app.handle_key(KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    ));
    let screen = h.render();
    assert!(screen.contains("created TASK-4"), "{screen}");
    assert_eq!(h.app.total_tasks(), 4);
    assert_eq!(h.app.pane, Pane::None);
}

#[test]
fn capture_and_pr_tab_coexist_without_losing_new_task() {
    let mut h = Harness::new();
    assert!(h.render().contains("Pull Requests"));
    h.type_text("tn");
    h.type_text("Capture alongside PRs");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("created TASK-4"), "{screen}");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("[Pull Requests]"), "{screen}");
    assert!(screen.contains("Loading pull requests"), "{screen}");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("[Tasks]"), "{screen}");
    assert_eq!(h.selected_title(), "Capture alongside PRs");
}

#[test]
fn task_new_chord_is_discoverable_and_old_a_is_unbound() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('a'));
    assert!(screen.contains("a is not bound"), "{screen}");
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("t n"), "{screen}");
    h.press(KeyCode::Esc);
    let screen = h.press(KeyCode::Char('t'));
    assert!(screen.contains("New task"), "{screen}");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.total_tasks(), 3);
    std::fs::write(&h.config_path, "return { keys = { x = 'task' } }").unwrap();
    h.app.tick();
    h.type_text("xnRebound prefix");
    h.press(KeyCode::Enter);
    assert_eq!(h.selected_title(), "Rebound prefix");
}
