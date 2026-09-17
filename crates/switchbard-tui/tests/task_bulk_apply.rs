//! Bulk apply on Tasks: with a non-empty selection, the next value picked
//! for status, project or ball applies to every marked task instead of only
//! the cursor row (the word-processor rule) — proved by reading the tasks
//! back off disk. With no marks, a single-row edit is unchanged.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_core::load_backlog_repo;

/// Mark the first two tasks (top of the default three-task harness) and
/// leave the cursor on the third, unmarked one — proving marks win even
/// when the cursor itself is not part of the selection.
fn mark_two_leave_cursor_on_third(h: &mut Harness) -> (String, String, String) {
    h.press(KeyCode::Char('g')); // top
    let ids: Vec<String> = h
        .app
        .rows
        .iter()
        .filter_map(|row| match row {
            switchbard_tui::group::Row::Task(index) => Some(h.app.tasks()[*index].id.clone()),
            switchbard_tui::group::Row::Heading { .. } => None,
        })
        .collect();
    h.press(KeyCode::Char(' ')); // marks ids[0], steps down to ids[1]
    h.press(KeyCode::Char(' ')); // marks ids[1], steps down to ids[2]
    assert_eq!(
        h.app.selected_task().map(|task| task.id.clone()),
        Some(ids[2].clone()),
        "cursor must land on the unmarked third task"
    );
    (ids[0].clone(), ids[1].clone(), ids[2].clone())
}

#[test]
fn bulk_status_applies_to_every_marked_task_and_not_the_unmarked_cursor() {
    let mut h = Harness::new();
    let (a, b, cursor) = mark_two_leave_cursor_on_third(&mut h);

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('s'));
    h.type_text("Done");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("status → Done: 2 tasks"), "{screen}");

    let repo = load_backlog_repo(&h.root).unwrap();
    let status = |id: &str| {
        repo.tasks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .status
            .clone()
    };
    assert_eq!(status(&a), "Done");
    assert_eq!(status(&b), "Done");
    assert_ne!(
        status(&cursor),
        "Done",
        "the unmarked cursor row must not change: marks win"
    );
}

#[test]
fn bulk_project_applies_to_every_marked_task() {
    let mut h = Harness::new();
    seed_project(&h.root, "Delivery", "In Progress", None);
    h.tick_until_tasks_settle();
    let (a, b, cursor) = mark_two_leave_cursor_on_third(&mut h);

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('p'));
    h.type_text("Delivery");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("project → Delivery: 2 tasks"), "{screen}");

    let repo = load_backlog_repo(&h.root).unwrap();
    let project = |id: &str| {
        repo.tasks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .project
            .clone()
    };
    assert_eq!(project(&a).as_deref(), Some("Delivery"));
    assert_eq!(project(&b).as_deref(), Some("Delivery"));
    assert_eq!(
        project(&cursor),
        None,
        "the unmarked cursor row must not change"
    );
}

#[test]
fn bulk_ball_applies_to_every_marked_task() {
    let mut h = Harness::new();
    let (a, b, cursor) = mark_two_leave_cursor_on_third(&mut h);

    let screen = h.press(KeyCode::Char('b'));
    assert!(screen.contains("ball → me: 2 tasks"), "{screen}");

    let repo = load_backlog_repo(&h.root).unwrap();
    let has_ball = |id: &str| {
        repo.tasks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .labels
            .iter()
            .any(|l| l == "ball:me")
    };
    assert!(has_ball(&a));
    assert!(has_ball(&b));
    assert!(
        !has_ball(&cursor),
        "the unmarked cursor row must not change"
    );
}

#[test]
fn with_no_marks_status_project_and_ball_still_apply_to_the_cursor_row_only() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();
    assert!(h.app.task_selection.marked.is_empty());

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('s'));
    h.type_text("Done");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.status, format!("{id} is Done"));

    let repo = load_backlog_repo(&h.root).unwrap();
    assert_eq!(
        repo.tasks.iter().find(|t| t.id == id).unwrap().status,
        "Done"
    );
}
