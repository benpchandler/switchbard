//! Page navigation through real keys and rendered screens.
mod harness;
use crossterm::event::KeyCode;
use harness::*;

#[test]
fn toggles_pages_without_losing_task_context() {
    let mut h = Harness::new();
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    let selected = h.selected_title();
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("[Pull Requests]"), "{screen}");
    assert!(screen.contains("Loading pull requests"), "{screen}");
    assert!(!screen.contains("Add dark theme"), "{screen}");
    h.type_text("td");
    h.press(KeyCode::Esc);
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("[Tasks]"), "{screen}");
    assert_eq!(h.selected_title(), selected);
    assert_eq!(h.app.state.filter, "theme");
    assert_ne!(h.app.selected_task().unwrap().status, "Done");
}

#[test]
fn page_binding_is_configurable_and_help_is_available_on_both_pages() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        "return { keys = { tab = 'help', x = 'page' } }",
    )
    .unwrap();
    h.app.tick();
    let screen = h.press(KeyCode::Char('x'));
    assert!(screen.contains("[Pull Requests]"));
    assert!(screen.contains("x switch page"));
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("page"));
    assert!(screen.contains(" keys "));
    h.press(KeyCode::Esc);
    let screen = h.press(KeyCode::Char('x'));
    assert!(screen.contains("[Tasks]"));
}

#[test]
fn page_survives_self_restart_and_hidden_task_commands_do_nothing() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('j'));
    let selected = h.selected_title();
    let before = h.app.state.clone();
    h.press(KeyCode::Tab);
    h.type_text(":group status");
    h.press(KeyCode::Enter);
    h.type_text("bw");
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, before);
    h.press(KeyCode::Tab);
    let resume = h.app.resume_state();
    h.app = open_app(&h.root, &h.config_path);
    h.app.resume_from(Some(&resume));
    assert!(h.render().contains("[Pull Requests]"));
    assert!(h.press(KeyCode::Tab).contains("[Tasks]"));
    assert_eq!(h.selected_title(), selected);
}

#[test]
fn pages_render_at_small_and_large_terminal_sizes_with_no_tasks() {
    let mut h = Harness::new();
    for file in std::fs::read_dir(h.root.join("backlog/tasks")).unwrap() {
        std::fs::remove_file(file.unwrap().path()).unwrap();
    }
    h.press(KeyCode::Char('r'));
    for (width, height) in [(80, 24), (120, 40), (180, 50), (40, 8), (0, 0)] {
        h.terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        if width > 0 {
            assert!(screen.contains("[Tasks]"), "{screen}");
        }
        let screen = h.press(KeyCode::Tab);
        if width > 0 {
            assert!(screen.contains("[Pull Requests]"), "{screen}");
            assert!(screen.contains("Loading pull requests"), "{screen}");
        }
        h.press(KeyCode::Tab);
    }
}
