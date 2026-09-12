//! TASK-209.3: the parent's `[done/total]` roll-up badge, backed by
//! `switchbard_core::subtask_progress` (`tasks::TaskRelations::subtasks`).

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn a_parent_with_children_shows_a_done_total_badge_after_its_title() {
    let mut h = Harness::new();
    // TASK-2 ("Add dark theme") gets two direct sub-tasks, one done.
    seed_child(&h.root, "Pick a palette", "Done", "TASK-2");
    seed_child(&h.root, "Wire the toggle", "To Do", "TASK-2");
    h.press(KeyCode::Char('r'));
    let screen = h.render();

    assert!(
        screen.contains("Add dark theme  [1/2]"),
        "parent title carries a done/total badge: {screen}"
    );
}

#[test]
fn a_childless_task_shows_no_badge() {
    let h = Harness::new();
    let screen = h.app.last_screen.clone();
    assert!(
        !screen.contains("Fix login redirect loop  ["),
        "no children, no badge: {screen}"
    );
}

#[test]
fn completing_the_last_open_child_updates_the_badge() {
    let mut h = Harness::new();
    seed_child(&h.root, "Pick a palette", "Done", "TASK-2");
    let other = seed_child(&h.root, "Wire the toggle", "To Do", "TASK-2");
    h.press(KeyCode::Char('r'));
    let screen = h.render();
    assert!(screen.contains("Add dark theme  [1/2]"), "{screen}");

    switchbard_core::edit_backlog_task(
        &h.root,
        &other,
        &switchbard_core::BacklogTaskPatch {
            status: Some("Done".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    h.press(KeyCode::Char('r'));
    let screen = h.render();
    assert!(screen.contains("Add dark theme  [2/2]"), "{screen}");
}
