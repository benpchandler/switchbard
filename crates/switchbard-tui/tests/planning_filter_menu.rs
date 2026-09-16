//! Planning filters remain available when existing saved views omit the column.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};
use switchbard_core::{set_task_planning, PlanningState};
use switchbard_tui::columns::Column;

fn saved_view(columns: &str) -> Harness {
    let mut h = Harness::new();
    let _ = set_task_planning(&h.root, "TASK-1", PlanningState::Considering).unwrap();
    let _ = set_task_planning(&h.root, "TASK-2", PlanningState::Planned).unwrap();
    let _ = set_task_planning(&h.root, "TASK-3", PlanningState::Planned).unwrap();
    std::fs::write(
        h.root.join("views.lua"),
        format!("return {{ {{ columns = '{columns}' }} }}\n"),
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    h
}

#[test]
fn hidden_planning_is_discoverable_and_filters_planned_and_considering() {
    let mut h = saved_view("id,status,priority,title,ball");
    h.terminal = Terminal::new(TestBackend::new(140, 43)).unwrap();
    let columns = h.app.state.columns.clone();
    assert!(!columns.contains(&Column::Planning));
    let statuses: Vec<_> = h
        .app
        .tasks()
        .iter()
        .map(|task| (task.id.clone(), task.status.clone()))
        .collect();
    let screen = h.press(KeyCode::Char('f'));
    assert!(screen.contains("planning · hidden"), "{screen}");
    let screen = h.type_text("pl");
    assert!(
        screen.contains("Considering") && screen.contains("Planned"),
        "{screen}"
    );
    let screen = h.press(KeyCode::Char('p'));
    assert!(screen.contains("2/3 shown"), "{screen}");
    assert!(!screen.contains("Fix login"), "{screen}");
    assert!(
        screen.contains("Add dark theme") && screen.contains("Write onboarding"),
        "{screen}"
    );
    assert_eq!(h.app.state.filter, "planning:planned");
    h.type_text("fpl");
    let screen = h.press(KeyCode::Char('c'));
    assert!(
        screen.contains("1/3 shown") && screen.contains("Fix login"),
        "{screen}"
    );
    assert!(
        !screen.contains("Add dark theme") && !screen.contains("Write onboarding"),
        "{screen}"
    );
    assert_eq!(h.app.state.filter, "planning:considering");
    assert_eq!(h.app.state.columns, columns);
    assert_eq!(
        statuses,
        h.app
            .tasks()
            .iter()
            .map(|task| (task.id.clone(), task.status.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn short_filter_menus_scroll_to_planning_and_cancel_preserves_the_saved_view() {
    for (width, height) in [(100, 12), (60, 12)] {
        let mut h = saved_view("id,status,priority,title,ball");
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let before = h.app.state.clone();
        let saved = std::fs::read_to_string(h.root.join("views.lua")).unwrap();
        let screen = h.press(KeyCode::Char('f'));
        assert!(
            screen.contains("↓") && screen.contains("/16"),
            "scroll affordance: {screen}"
        );
        let mut screen = screen;
        for _ in 0..16 {
            if screen.contains("planning · hidden") {
                break;
            }
            screen = h.press(KeyCode::Down);
        }
        assert!(
            screen.contains("planning · hidden"),
            "reachable without knowing its name: {screen}"
        );
        h.press(KeyCode::Esc);
        assert_eq!(h.app.state, before);
        h.type_text("fpl");
        let screen = h.render();
        assert!(
            screen.contains("Considering") && screen.contains("Planned"),
            "{screen}"
        );
        h.press(KeyCode::Esc);
        assert_eq!(h.app.state, before);
        assert_eq!(
            std::fs::read_to_string(h.root.join("views.lua")).unwrap(),
            saved
        );
    }
}

#[test]
fn shown_planning_has_one_filter_entry_and_cancel_keeps_columns() {
    let mut h = saved_view("id,planning,status,title");
    let before = h.app.state.clone();
    let screen = h.press(KeyCode::Char('f'));
    assert!(screen.contains("2 ✓planning"), "{screen}");
    assert!(!screen.contains("planning · hidden"), "{screen}");
    let screen = h.press(KeyCode::Char('2'));
    assert!(
        screen.contains("Considering") && screen.contains("Planned"),
        "{screen}"
    );
    h.press(KeyCode::Esc);
    assert_eq!(h.app.state, before);
    h.type_text("f2p");
    let screen = h.render();
    assert!(screen.contains("2/3 shown"), "{screen}");
    assert_eq!(h.app.state.filter, "planning:planned");
    assert_eq!(h.app.state.columns, before.columns);
}
