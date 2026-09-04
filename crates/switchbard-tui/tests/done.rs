//! `t d` is a native status mutation from the terminal, including its repeat-safe
//! state. The harness opens a real backlog and reparses it after every write.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn td_marks_the_selected_task_done_and_a_repeat_is_a_no_op() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();

    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('d'));
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
    h.press(KeyCode::Char('d'));
    assert_eq!(h.app.status, format!("{id} is already Done"));
    assert_eq!(std::fs::read_to_string(task_path).unwrap(), written);
}
