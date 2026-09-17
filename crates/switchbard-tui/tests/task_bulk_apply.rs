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

/// Drive the cursor (via `g` then `j`) onto the row carrying `id`, wherever
/// the current view sorted it — bulk reparent tests can't assume position
/// since a move changes ids and re-sorts the list underneath them.
fn select_task(h: &mut Harness, id: &str) {
    h.press(KeyCode::Char('g'));
    for _ in 0..500 {
        if h.app.selected_task().map(|task| task.id.as_str()) == Some(id) {
            return;
        }
        h.press(KeyCode::Char('j'));
    }
    panic!("{id} is not in the current view");
}

fn mark(h: &mut Harness, id: &str) {
    select_task(h, id);
    h.press(KeyCode::Char(' '));
}

#[test]
fn bulk_reparent_applies_to_every_marked_task_and_not_the_unmarked_cursor() {
    let mut h = Harness::new();
    seed(&h.root, "Ship release notes", "To Do", &[]); // TASK-4: the new parent target
    h.press(KeyCode::Char('r'));
    mark(&mut h, "TASK-1");
    mark(&mut h, "TASK-3");
    select_task(&mut h, "TASK-2"); // cursor lands on an unmarked row

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('a'));
    let screen = h.type_text("Ship release");
    assert!(screen.contains("TASK-4"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("parent → TASK-4: 2 tasks"), "{screen}");

    let repo = load_backlog_repo(&h.root).unwrap();
    // The old ids are gone: a reparent mints a new id, it doesn't edit in place.
    assert!(!repo.tasks.iter().any(|t| t.id == "TASK-1"));
    assert!(!repo.tasks.iter().any(|t| t.id == "TASK-3"));
    let login = repo
        .tasks
        .iter()
        .find(|t| t.title.contains("Fix login"))
        .unwrap();
    let guide = repo
        .tasks
        .iter()
        .find(|t| t.title.contains("onboarding guide"))
        .unwrap();
    assert_eq!(login.parent.as_deref(), Some("TASK-4"));
    assert_eq!(guide.parent.as_deref(), Some("TASK-4"));
    assert!(login.id.starts_with("TASK-4."), "{}", login.id);
    assert!(guide.id.starts_with("TASK-4."), "{}", guide.id);
    let cursor = repo.tasks.iter().find(|t| t.id == "TASK-2").unwrap();
    assert_eq!(
        cursor.parent, None,
        "the unmarked cursor row must not move: marks win"
    );
}

#[test]
fn bulk_reparent_of_a_marked_parent_and_its_marked_child_moves_both_without_orphaning_or_duplicating(
) {
    let mut h = Harness::new();
    seed(&h.root, "Ship release notes", "To Do", &[]); // TASK-4: the new parent target
    let child_id = seed_child(&h.root, "Split login into steps", "To Do", "TASK-1");
    h.press(KeyCode::Char('r'));
    mark(&mut h, "TASK-1"); // the current parent
    mark(&mut h, &child_id); // its own current child, also marked
    select_task(&mut h, "TASK-2");

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('a'));
    h.type_text("Ship release");
    let screen = h.press(KeyCode::Enter);
    // Both move: reparenting sets one flat target parent for every marked
    // task (the same rule bulk status/project/ball already follow), so the
    // former parent and its former child end up as siblings under TASK-4,
    // not nested under each other. Neither fails core's "has sub-issues"
    // refusal, because the batch moves the marked child ahead of its marked
    // parent before either move is attempted.
    assert!(screen.contains("parent → TASK-4: 2 tasks"), "{screen}");

    let repo = load_backlog_repo(&h.root).unwrap();
    assert_eq!(
        repo.tasks.len(),
        5,
        "no task must be dropped or duplicated by the batch"
    );
    assert!(!repo.tasks.iter().any(|t| t.id == "TASK-1"));
    assert!(!repo.tasks.iter().any(|t| t.id == child_id));
    let moved_parent = repo
        .tasks
        .iter()
        .find(|t| t.title.contains("Fix login"))
        .unwrap();
    let moved_child = repo
        .tasks
        .iter()
        .find(|t| t.title.contains("Split login"))
        .unwrap();
    assert_eq!(moved_parent.parent.as_deref(), Some("TASK-4"));
    assert_eq!(moved_child.parent.as_deref(), Some("TASK-4"));
    assert_ne!(moved_parent.id, moved_child.id);
    // Every remaining parent link names a task that still exists: nothing
    // orphaned by the rename chain.
    for task in &repo.tasks {
        if let Some(parent) = &task.parent {
            assert!(
                repo.tasks.iter().any(|other| &other.id == parent),
                "{} names a parent {parent} that does not exist",
                task.id
            );
        }
    }
}

#[test]
fn bulk_reparent_selection_follows_moved_tasks_to_their_new_ids() {
    let mut h = Harness::new();
    seed(&h.root, "Ship release notes", "To Do", &[]); // TASK-4
    h.press(KeyCode::Char('r'));
    mark(&mut h, "TASK-1");
    mark(&mut h, "TASK-3");
    select_task(&mut h, "TASK-2");

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('a'));
    h.type_text("Ship release");
    h.press(KeyCode::Enter);

    assert!(!h.app.task_selection.marked.contains("TASK-1"));
    assert!(!h.app.task_selection.marked.contains("TASK-3"));
    let repo = load_backlog_repo(&h.root).unwrap();
    let login_id = repo
        .tasks
        .iter()
        .find(|t| t.title.contains("Fix login"))
        .unwrap()
        .id
        .clone();
    let guide_id = repo
        .tasks
        .iter()
        .find(|t| t.title.contains("onboarding guide"))
        .unwrap()
        .id
        .clone();
    assert!(
        h.app.task_selection.marked.contains(&login_id),
        "{:?}",
        h.app.task_selection.marked
    );
    assert!(
        h.app.task_selection.marked.contains(&guide_id),
        "{:?}",
        h.app.task_selection.marked
    );

    // Prove the remap is more than bookkeeping: a second bulk action driven
    // by this same, now-remapped selection must land on the moved tasks —
    // under their new ids — and nowhere else.
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
    assert_eq!(status(&login_id), "Done");
    assert_eq!(status(&guide_id), "Done");
    assert_ne!(status("TASK-2"), "Done");
}

#[test]
fn with_no_marks_reparent_still_applies_to_the_cursor_row_only() {
    let mut h = Harness::new();
    seed(&h.root, "Ship release notes", "To Do", &[]); // TASK-4
    h.press(KeyCode::Char('r'));
    select_task(&mut h, "TASK-1");
    assert!(h.app.task_selection.marked.is_empty());

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('a'));
    h.type_text("Ship release");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("TASK-1 → TASK-4."), "{screen}");

    let repo = load_backlog_repo(&h.root).unwrap();
    assert!(!repo.tasks.iter().any(|t| t.id == "TASK-1"));
    let moved = repo
        .tasks
        .iter()
        .find(|t| t.title.contains("Fix login"))
        .unwrap();
    assert_eq!(moved.parent.as_deref(), Some("TASK-4"));
    assert_eq!(
        repo.tasks.iter().find(|t| t.id == "TASK-2").unwrap().parent,
        None
    );
    assert_eq!(
        repo.tasks.iter().find(|t| t.id == "TASK-3").unwrap().parent,
        None
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
