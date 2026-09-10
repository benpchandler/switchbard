//! `t s` opens the native status picker from the terminal, including its repeat-safe
//! state. The harness opens a real backlog and reparses it after every write.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn status_picker_marks_done_and_a_repeat_is_a_no_op() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('s'));
    h.type_text("Done");
    let screen = h.press(KeyCode::Enter);
    assert_eq!(h.app.status, format!("{id} is Done"));
    assert_eq!(
        h.app.selected_task().map(|task| task.id.as_str()),
        Some(id.as_str())
    );
    assert_eq!(
        h.app.selected_task().map(|task| task.status.as_str()),
        Some("Done")
    );
    assert!(screen.contains("Done"));

    let task_path = std::fs::read_dir(h.root.join("backlog/tasks"))
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with(&id.to_ascii_lowercase())
        })
        .unwrap()
        .path();
    let written = std::fs::read_to_string(&task_path).unwrap();
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('s'));
    h.type_text("Done");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.status, format!("{id} is Done"));
    assert_eq!(std::fs::read_to_string(task_path).unwrap(), written);
}

#[test]
fn status_picker_uses_declared_destinations_and_waits_for_commit() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();
    std::fs::write(
        h.root.join("backlog/config.yml"),
        "project_name: fixture\nstatuses: [To Do, In Progress, Blocked, Done]\ntask_prefix: task\n",
    )
    .unwrap();
    let screen = h.type_text("ts");
    assert!(screen.contains("Blocked"), "{screen}");
    assert!(screen.contains("In Progress"), "{screen}");
    assert!(!screen.contains("Mark Done"), "{screen}");
    h.type_text("Blocked");
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().status,
        "In Progress"
    );
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("Blocked"), "{screen}");
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().status,
        "Blocked"
    );
    h.type_text("tsDone");
    h.press(KeyCode::Esc);
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().status,
        "Blocked"
    );
}

#[test]
fn disappearing_status_target_cancels_without_editing_another_task() {
    let mut h = Harness::new();
    let path = h.app.selected_task().unwrap().path.clone();
    h.type_text("ts");
    std::fs::remove_file(path).unwrap();
    h.app.tick();
    assert!(h.app.picker.is_none(), "{}", h.render());
    assert!(h.app.tasks().iter().all(|t| t.status != "Done"));
}
