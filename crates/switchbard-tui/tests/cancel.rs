mod harness;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use harness::{select_task_titled, Harness};
use switchbard_core::{edit_backlog_task, load_backlog_repo, BacklogTaskPatch, BacklogTaskSource};

fn selected(h: &Harness) -> switchbard_core::BacklogTask {
    h.app.selected_task().unwrap().clone()
}
fn open(h: &mut Harness) -> String {
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('c'))
}

#[test]
fn cancellation_requires_confirmation_and_retains_record() {
    let mut h = Harness::new();
    let task = selected(&h);
    let screen = open(&mut h);
    assert!(screen.contains(&format!("Cancel {}?", task.id)), "{screen}");
    assert!(screen.contains(&task.title), "{screen}");
    assert!(screen.contains("Keep task"), "{screen}");
    assert!(task.path.exists());
    let screen = h.press(KeyCode::Char('c'));
    assert!(
        screen.contains(&format!("Canceled {}", task.id)),
        "{screen}"
    );
    let repo = load_backlog_repo(&h.root).unwrap();
    let archived = repo.tasks.iter().find(|t| t.id == task.id).unwrap();
    assert_eq!(archived.source, BacklogTaskSource::Archived);
    assert!(!archived.is_done());
    assert_eq!(archived.acceptance_criteria, task.acceptance_criteria);
    assert!(!task.path.exists());
}

#[test]
fn enter_escape_and_repeat_keep_task() {
    let mut h = Harness::new();
    let task = selected(&h);
    for key in [KeyCode::Enter, KeyCode::Esc] {
        open(&mut h);
        h.press(key);
        assert!(task.path.exists());
    }
    open(&mut h);
    let mut repeat = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    repeat.kind = KeyEventKind::Repeat;
    h.app.handle_key(repeat);
    assert!(task.path.exists());
}

#[test]
fn concurrent_edit_refuses_cancel() {
    let mut h = Harness::new();
    let task = selected(&h);
    open(&mut h);
    edit_backlog_task(
        &h.root,
        &task.id,
        &BacklogTaskPatch {
            title: Some("Changed during confirmation".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let screen = h.press(KeyCode::Char('c'));
    assert!(screen.contains("task changed"), "{screen}");
    assert!(task.path.exists());
}

#[test]
fn missing_task_and_done_task_refuse_cancel() {
    let mut h = Harness::new();
    let task = selected(&h);
    open(&mut h);
    std::fs::remove_file(&task.path).unwrap();
    let screen = h.press(KeyCode::Char('c'));
    assert!(screen.contains("no longer exists"), "{screen}");
    let mut h = Harness::new();
    let task = selected(&h);
    edit_backlog_task(
        &h.root,
        &task.id,
        &BacklogTaskPatch {
            status: Some("Done".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let screen = open(&mut h);
    assert!(
        screen.contains("Done tasks should be completed"),
        "{screen}"
    );
    assert!(task.path.exists());
}

#[test]
fn canceled_parent_keeps_children_and_blocks_dependents() {
    let mut h = Harness::new();
    let parent = selected(&h);
    let child = harness::seed_child(&h.root, "Child remains", "To Do", &parent.id);
    let dependent = harness::seed_with_deps(&h.root, "Dependent remains", "To Do", &[&parent.id]);
    open(&mut h);
    h.press(KeyCode::Char('c'));
    let repo = load_backlog_repo(&h.root).unwrap();
    assert_eq!(
        repo.tasks.iter().find(|t| t.id == child).unwrap().source,
        BacklogTaskSource::Active
    );
    let dependent = repo.tasks.iter().find(|t| t.id == dependent).unwrap();
    assert!(switchbard_core::is_blocked(dependent, &repo));
}

#[test]
fn narrow_confirmation_cannot_cancel_hidden_identity() {
    let mut h = Harness::new();
    let task = selected(&h);
    h.terminal.backend_mut().resize(25, 8);
    h.terminal
        .resize(ratatui::layout::Rect::new(0, 0, 25, 8))
        .unwrap();
    let screen = open(&mut h);
    assert!(screen.contains("Enlarge terminal"), "{screen}");
    assert!(!screen.contains("confirm merge"), "{screen}");
    h.press(KeyCode::Char('c'));
    assert!(task.path.exists());
}

#[test]
fn changed_selection_refuses_cancel() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let task = selected(&h);
    open(&mut h);
    h.app.selected = usize::MAX;
    h.press(KeyCode::Char('c'));
    assert!(task.path.exists());
}
