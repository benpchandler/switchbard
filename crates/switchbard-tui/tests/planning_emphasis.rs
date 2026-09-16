//! Combined installation preserves planned work and heading emphasis.
mod harness;

use crossterm::event::KeyCode;
use harness::{open_app, Harness};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};
use switchbard_core::{set_task_planning, PlanningState};

fn assert_struck(h: &Harness, needle: &str) {
    let buffer = h.terminal.backend().buffer();
    for row in buffer.content.chunks(usize::from(buffer.area.width).max(1)) {
        let text: String = row.iter().map(|cell| cell.symbol()).collect();
        if let Some(offset) = text.find(needle) {
            assert!(row[text[..offset].chars().count()]
                .modifier
                .contains(Modifier::CROSSED_OUT));
            return;
        }
    }
    panic!("missing {needle}");
}

#[test]
fn planned_and_other_headings_keep_emphasis_with_checklist_and_details() {
    let mut h = Harness::new();
    std::fs::remove_file(h.root.join("views.lua")).unwrap();
    let _ = set_task_planning(&h.root, "TASK-1", PlanningState::Considering).unwrap();
    let _ = set_task_planning(&h.root, "TASK-2", PlanningState::Planned).unwrap();
    let _ = set_task_planning(&h.root, "TASK-3", PlanningState::Planned).unwrap();
    switchbard_core::set_backlog_acceptance_checked(&h.root, "TASK-2", 1, true).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.type_text(":paint heading:Planned=struck;heading:Other tasks=struck");
    h.press(KeyCode::Enter);
    for (width, height) in [(140, 28), (60, 14)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        assert!(screen.contains("Review"), "{screen}");
        assert_struck(&h, "Planned ·");
        assert_struck(&h, "Other tasks");
        println!("EVIDENCE {width}x{height}\n{screen}\nEND EVIDENCE");
    }
    h.terminal = Terminal::new(TestBackend::new(140, 28)).unwrap();
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("planning: Planned"), "{screen}");
    println!("EVIDENCE detail\n{screen}\nEND EVIDENCE");
    assert_eq!(h.app.selected_task().unwrap().status, "To Do");
    h.press(KeyCode::Esc);
    h.type_text("p");
    let screen = h.render();
    assert!(screen.contains("column headings"), "{screen}");
}
