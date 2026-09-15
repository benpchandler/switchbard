//! Real-key/render coverage for complete, collapsible task details.
mod harness;
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use harness::*;
use ratatui::{backend::TestBackend, Terminal};
use switchbard_tui::app::Mode;
const SECTIONS: [&str; 10] = [
    "Properties",
    "Description",
    "Acceptance criteria",
    "Relations",
    "Implementation plan",
    "Implementation notes",
    "Final summary",
    "Definition of Done",
    "References",
    "Metadata (read-only)",
];

fn open(h: &mut Harness) {
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
}

#[test]
fn all_sections_show_empty_values_and_collapse_without_changing_task() {
    let mut h = Harness::new();
    h.terminal = Terminal::new(TestBackend::new(180, 100)).unwrap();
    let original = h.app.selected_task().unwrap().clone();
    open(&mut h);
    let screen = h.render();
    for section in SECTIONS {
        assert!(screen.contains(section), "{screen}");
    }
    for label in [
        "parent: Not set",
        "dependencies: Not set",
        "subtasks: Not set",
        "assignees: Not set",
        "repository ID: Not set",
        "record ID: Not set",
        "revision: Not set",
        "custom fields: Not set",
        "source path:",
        "created:",
        "updated:",
    ] {
        assert!(screen.contains(label), "missing {label}: {screen}");
    }
    let collapsed = h.press(KeyCode::Char('Z'));
    assert!(!collapsed.contains("source path:"), "{collapsed}");
    assert_eq!(h.app.detail_rows().len(), SECTIONS.len());
    h.press(KeyCode::Enter);
    assert!(h.render().contains("status:"));
    h.press(KeyCode::Char('z'));
    assert!(!h.render().contains("status:"));
    h.press(KeyCode::Char(' '));
    assert!(h.render().contains("status:"));
    h.press(KeyCode::Char('A'));
    assert!(h.render().contains("source path:"));
    assert_eq!(*h.app.selected_task().unwrap(), original);
}

#[test]
fn mouse_header_toggle_and_dirty_input_preservation() {
    let mut h = Harness::new();
    open(&mut h);
    let header = h.app.detail_hit.section_starts[0].0;
    let inner = h.app.detail_hit.detail_inner();
    h.mouse(
        MouseEventKind::Down(MouseButton::Left),
        inner.x + 1,
        inner.y + header,
    );
    assert!(!h.app.detail_collapsed.is_empty());
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    assert!(matches!(h.app.mode, Mode::DetailInput(_)));
    h.type_text(" draft 日本語");
    let draft = h.app.input.clone();
    h.mouse(
        MouseEventKind::Down(MouseButton::Left),
        inner.x + 1,
        inner.y + header,
    );
    assert_eq!(h.app.input, draft);
    assert!(h.app.detail_collapsed.is_empty());
    h.press(KeyCode::Esc);
    assert!(!h.app.selected_task().unwrap().title.contains("draft"));
}

#[test]
fn full_record_contents_remain_reachable_in_narrow_and_short_viewports() {
    let mut h = Harness::new();
    let task = h.app.selected_task().unwrap().clone();
    declare_field(
        &h.root,
        "reviewer",
        switchbard_core::FieldKind::Text,
        &[],
        false,
    );
    switchbard_core::edit_backlog_task(
        &h.root,
        &task.id,
        &switchbard_core::BacklogTaskPatch {
            references: Some(vec!["https://example.test/review".to_string()]),
            assignees: Some(vec!["Alice".to_string()]),
            set_custom: vec![("reviewer".to_string(), "Review team".to_string())],
            ..Default::default()
        },
    )
    .unwrap();
    let mut text = std::fs::read_to_string(&task.path).unwrap();
    text.push_str("\n## Implementation Plan\n\nPlan 日本語\n\n## Implementation Notes\n\nNotes café\n\n## Final Summary\n\nSummary approved\n\n## Definition of Done\n\n- [ ] #1 Result usable\n");
    std::fs::write(&task.path, text).unwrap();
    h.app.tick();
    open(&mut h);
    for _ in 0..6 {
        h.press(KeyCode::Char('j'));
    }
    for (width, height) in [(180, 100), (60, 12), (32, 8)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut observed = h.render();
        if width == 180 {
            for needle in [
                "assignees: Alice",
                "reviewer: Review team",
                "https://example.test/review",
                "Plan 日本語",
                "Notes café",
                "Summary approved",
                "created:",
                "updated:",
            ] {
                assert!(observed.contains(needle), "missing {needle}: {observed}");
            }
            assert!(h.app.selected_task().unwrap().created_date.is_some());
            assert!(h.app.selected_task().unwrap().updated_date.is_some());
        }
        for _ in 0..90 {
            observed.push_str(&h.press(KeyCode::PageDown));
        }
        for needle in ["Plan", "Notes", "Summary", "Result", "source path"] {
            assert!(
                observed.contains(needle),
                "missing {needle} at {width}x{height}"
            );
        }
        for _ in 0..90 {
            h.press(KeyCode::PageUp);
        }
    }
}

#[test]
fn historical_subtasks_remain_visible_in_parent_details() {
    let mut h = Harness::new();
    let completed = seed_child(&h.root, "Finished design", "Done", "TASK-2");
    let archived = seed_child(&h.root, "Superseded sketch", "To Do", "TASK-2");
    switchbard_core::complete_backlog_task(&h.root, &completed).unwrap();
    switchbard_core::archive_backlog_task(&h.root, &archived).unwrap();
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Char('/'));
    h.type_text("Add dark theme");
    h.press(KeyCode::Enter);
    h.terminal = Terminal::new(TestBackend::new(220, 65)).unwrap();
    open(&mut h);
    let screen = h.render();
    assert!(
        screen.contains(&format!("{completed} Finished design (Done; completed)")),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("{archived} Superseded sketch (To Do; archived)")),
        "{screen}"
    );
}
