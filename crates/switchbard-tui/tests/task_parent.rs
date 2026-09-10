//! Parent assignment through real task-menu keys and a disk-backed backlog.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};
use switchbard_core::load_backlog_repo;
use switchbard_tui::app::Mode;

fn open_parent(h: &mut Harness) -> String {
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('a'))
}

#[test]
fn title_search_requires_enter_and_persists_canonical_parent_after_reload() {
    let mut h = Harness::new();
    let screen = open_parent(&mut h);
    assert!(screen.contains("TASK-2 · Add dark theme"), "{screen}");
    assert!(screen.contains("new task ID"), "{screen}");
    let screen = h.type_text("dark");
    assert!(screen.contains("Add dark theme"), "{screen}");
    assert_eq!(h.app.mode, Mode::PickValue);
    assert!(h.app.selected_task().unwrap().parent.is_none());
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("TASK-1 → TASK-2.1"), "{screen}");
    assert_eq!(h.app.selected_task().unwrap().id, "TASK-2.1");
    assert_eq!(
        h.app.selected_task().unwrap().parent.as_deref(),
        Some("TASK-2")
    );
    h.app = open_app(&h.root, &h.config_path);
    let repo = load_backlog_repo(&h.root).unwrap();
    let child = repo.tasks.iter().find(|t| t.id == "TASK-2.1").unwrap();
    assert!(std::fs::read_to_string(&child.path)
        .unwrap()
        .contains("parent_task_id: TASK-2"));
    h.press(KeyCode::Char('/'));
    h.type_text("id:TASK-2.1");
    h.press(KeyCode::Enter);
    let screen = open_parent(&mut h);
    assert!(screen.contains("✓TASK-2 · Add dark theme"), "{screen}");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.selected_task().unwrap().id, "TASK-2.1");
    assert!(h.app.status.contains("unchanged"));
}

#[test]
fn numeric_ids_are_search_text_not_immediate_row_actions_and_parent_can_be_removed() {
    let mut h = Harness::new();
    open_parent(&mut h);
    h.type_text("2");
    assert_eq!(h.app.mode, Mode::PickValue);
    assert_eq!(h.app.selected_task().unwrap().id, "TASK-1");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.selected_task().unwrap().id, "TASK-2.1");
    open_parent(&mut h);
    h.type_text("No parent");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.selected_task().unwrap().id, "TASK-4");
    assert!(h.app.selected_task().unwrap().parent.is_none());
}

#[test]
fn cancel_back_and_no_matches_never_change_task_files() {
    let mut h = Harness::new();
    let before = load_backlog_repo(&h.root)
        .unwrap()
        .tasks
        .into_iter()
        .map(|t| (t.path.clone(), std::fs::read_to_string(t.path).unwrap()))
        .collect::<Vec<_>>();
    open_parent(&mut h);
    let screen = h.type_text("nonexistent");
    assert!(screen.contains("No matching parent tasks"), "{screen}");
    h.press(KeyCode::Enter);
    assert!(h.app.status.contains("nothing matches"));
    assert_eq!(h.app.mode, Mode::PickValue);
    h.press(KeyCode::Esc);
    open_parent(&mut h);
    h.type_text("dark");
    h.press(KeyCode::Left);
    assert!(h.render().contains("Link parent task"));
    h.press(KeyCode::Esc);
    for (path, text) in before {
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    }
}

#[test]
fn stale_target_fails_and_reopening_refreshes_choices() {
    let mut h = Harness::new();
    open_parent(&mut h);
    h.type_text("dark");
    let target = load_backlog_repo(&h.root)
        .unwrap()
        .tasks
        .into_iter()
        .find(|t| t.id == "TASK-2")
        .unwrap();
    std::fs::remove_file(target.path).unwrap();
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("parent change failed"), "{screen}");
    assert_eq!(h.app.selected_task().unwrap().id, "TASK-1");
    open_parent(&mut h);
    assert!(!h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .any(|o| o.label.contains("Add dark theme")));
    h.type_text("guide");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.selected_task().unwrap().parent.as_deref(),
        Some("TASK-3")
    );
}

#[test]
fn self_and_subissues_are_excluded_and_tasks_with_children_explain_the_block() {
    let mut h = Harness::new();
    seed_in_project(&h.root, "Child of dark", "To Do", "UI", Some("2"));
    h.press(KeyCode::Char('r'));
    open_parent(&mut h);
    let options = &h.app.picker.as_ref().unwrap().options;
    assert!(!options.iter().any(|o| o.label.contains("Fix login")));
    assert!(!options.iter().any(|o| o.label.contains("Child of dark")));
    h.type_text("dark");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('/'));
    h.type_text("id:TASK-2");
    h.press(KeyCode::Enter);
    let screen = open_parent(&mut h);
    assert!(screen.contains("has sub-issues"), "{screen}");
}

#[test]
fn narrow_terminal_shows_titles_and_scrolls_many_duplicate_unicode_choices() {
    let mut h = Harness::new();
    for _ in 0..30 {
        seed(
            &h.root,
            "日本語 parent with a very long title beyond the terminal width",
            "To Do",
            &[],
        );
    }
    h.terminal = Terminal::new(TestBackend::new(42, 12)).unwrap();
    open_parent(&mut h);
    let screen = h.type_text("dark");
    assert!(screen.contains("Add dark theme"), "{screen}");
    h.press(KeyCode::Esc);
    open_parent(&mut h);
    h.type_text("日本語");
    for _ in 0..29 {
        h.press(KeyCode::Down);
    }
    let screen = h.render();
    let buffer = h.terminal.backend().buffer();
    let mut cells = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            cells.push_str(symbol);
            x += ratatui::text::Span::raw(symbol).width().max(1) as u16;
        }
        cells.push('\n');
    }
    assert!(cells.contains("日本語"), "{cells}");
    assert!(screen.contains("30/30"), "{screen}");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.selected_task().unwrap().id, "TASK-1");
    h.terminal = Terminal::new(TestBackend::new(0, 0)).unwrap();
    assert_eq!(open_parent(&mut h), "");
}

#[test]
fn only_task_has_no_eligible_parent_and_empty_repo_has_no_parent_action() {
    let mut h = Harness::new();
    for task in load_backlog_repo(&h.root).unwrap().tasks {
        if task.id != "TASK-1" {
            std::fs::remove_file(task.path).unwrap();
        }
    }
    h.press(KeyCode::Char('r'));
    let screen = open_parent(&mut h);
    assert!(screen.contains("No eligible parent tasks"), "{screen}");
    h.press(KeyCode::Esc);
    for task in load_backlog_repo(&h.root).unwrap().tasks {
        std::fs::remove_file(task.path).unwrap();
    }
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Char('t'));
    assert!(!h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .any(|o| o.label.contains("parent")));
}

#[test]
fn disappeared_source_cancels_on_reload_without_editing_another_task() {
    let mut h = Harness::new();
    open_parent(&mut h);
    h.type_text("dark");
    let selected = h.app.selected_task().unwrap().path.clone();
    std::fs::remove_file(selected).unwrap();
    h.app.tick();
    let screen = h.render();
    assert!(screen.contains("task action canceled"), "{screen}");
    assert!(h.app.picker.is_none());
    assert!(load_backlog_repo(&h.root)
        .unwrap()
        .tasks
        .iter()
        .all(|task| task.parent.is_none()));
}
