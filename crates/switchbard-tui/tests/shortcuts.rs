//! Configured shortcuts exercise the real app, disk writes and rendered help.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn canonical_and_legacy_task_prefixes_create_tasks_and_advertise_the_same_action() {
    for alias in ["task", "rank"] {
        let mut h = Harness::new();
        std::fs::write(
            &h.config_path,
            format!("return {{ keys = {{ t = 'help', x = '{alias}' }} }}"),
        )
        .expect("write real Lua configuration");
        h.app.tick();
        let screen = h.press(KeyCode::Char('?'));
        assert!(screen.contains("x n"), "{screen}");
        assert!(screen.contains("new_task"), "{screen}");
        h.press(KeyCode::Esc);
        h.type_text("xn");
        h.type_text("Configured chord capture");
        let screen = h.press(KeyCode::Enter);
        assert!(screen.contains("created TASK-4"), "{screen}");
        let fresh = open_app(&h.root, &h.config_path);
        assert!(fresh
            .tasks()
            .iter()
            .any(|task| task.title == "Configured chord capture"));
    }
}

#[test]
fn direct_new_task_binding_is_discoverable_and_creates_on_disk() {
    let mut h = Harness::new();
    std::fs::write(&h.config_path, "return { keys = { x = 'new_task' } }")
        .expect("write real Lua configuration");
    h.app.tick();
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("t n x"), "{screen}");
    h.press(KeyCode::Esc);
    h.type_text("xDirect shortcut capture");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("created TASK-4"), "{screen}");
    let fresh = open_app(&h.root, &h.config_path);
    assert!(fresh
        .tasks()
        .iter()
        .any(|task| task.title == "Direct shortcut capture"));
}

#[test]
fn help_catalog_obeys_page_availability_and_survives_page_return() {
    let mut h = Harness::new();
    h.terminal = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("new_task"), "{screen}");
    assert!(screen.contains("settings"), "{screen}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Tab);
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("open_browser"), "{screen}");
    assert!(screen.contains("merge"), "{screen}");
    assert!(!screen.contains("new_task"), "{screen}");
    assert!(!screen.contains("settings"), "{screen}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Tab);
    assert!(h.press(KeyCode::Char('?')).contains("new_task"));
}

#[test]
fn narrow_help_keeps_report_instructions_accessible_without_moving_tasks() {
    for (width, height) in [(80, 24), (40, 8)] {
        let mut h = Harness::new();
        h.terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        let selected = h.selected_title();
        let mut screen = h.press(KeyCode::Char('?'));
        let mut found = false;
        for _ in 0..100 {
            let text = screen
                .replace('│', " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if text.contains("file an idea with this screen") {
                found = true;
                break;
            }
            screen = h.press(KeyCode::Char('j'));
        }
        assert!(found, "report instruction never accessible: {screen}");
        assert_eq!(h.selected_title(), selected);
        assert!(h.press(KeyCode::Char('g')).contains("new_task"));
        h.press(KeyCode::Char('G'));
        h.press(KeyCode::Esc);
        assert!(h.press(KeyCode::Char('?')).contains("new_task"));
    }
}
