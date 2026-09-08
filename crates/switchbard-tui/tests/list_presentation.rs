//! Real backlog/key/render stress journeys through the shared list presentation.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn large_list_keeps_last_selection_visible_after_resize_and_filter() {
    let mut h = Harness::new();
    for index in 0..250 {
        seed(&h.root, &format!("Scale item {index:03}"), "To Do", &[]);
    }
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    for (width, height) in [(100, 20), (48, 8), (160, 30)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let screen = h.press(KeyCode::Char('G'));
        let selected = h.selected_title();
        assert!(
            screen.contains(&selected),
            "selection hidden at {width}x{height}: {screen}"
        );
        h.press(KeyCode::Char('k'));
        let selected = h.selected_title();
        assert!(h.render().contains(&selected));
    }
    h.type_text("/unmatched-filter");
    h.press(KeyCode::Enter);
    assert!(h.app.selected_task().is_none());
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('k'));
    h.type_text("/");
    h.press(KeyCode::Esc);
}

#[test]
fn long_unicode_picker_labels_are_clipped_and_navigation_retains_focus() {
    let mut h = Harness::new();
    let name = format!("東京{}", "LongProject".repeat(20));
    seed_project(&h.root, &name, "Planned", None);
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.terminal = Terminal::new(TestBackend::new(42, 9)).unwrap();
    h.type_text("tp");
    assert!(h.app.picker.is_some());
    let screen = h.press(KeyCode::Down);
    assert!(screen.contains("project"), "{screen}");
    h.press(KeyCode::Esc);
    assert!(h.app.picker.is_none());
    let before = h.selected_title();
    h.press(KeyCode::Char('j'));
    assert_ne!(h.selected_title(), before);
}

#[test]
fn one_body_slot_keeps_grouped_selected_task_visible_instead_of_heading() {
    let mut h = Harness::new();
    seed_project(&h.root, "Tiny project", "Planned", None);
    seed_in_project(
        &h.root,
        "Visible selected task",
        "To Do",
        "Tiny project",
        None,
    );
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.type_text("o1");
    h.press(KeyCode::Char('g'));
    assert_eq!(h.selected_title(), "Visible selected task");
    h.terminal = Terminal::new(TestBackend::new(100, 6)).unwrap();
    let screen = h.render();
    assert!(
        screen.contains("Visible selected task"),
        "selected task must occupy the only body slot: {screen}"
    );
    h.press(KeyCode::Char('j'));
    let selected = h.selected_title();
    assert!(h.render().contains(&selected));
    h.press(KeyCode::Char('k'));
    assert!(h.render().contains("Visible selected task"));
    h.terminal = Terminal::new(TestBackend::new(100, 7)).unwrap();
    let screen = h.render();
    assert!(
        screen.contains("Tiny project"),
        "heading should return with two body slots: {screen}"
    );
    assert!(screen.contains("Visible selected task"));
}
