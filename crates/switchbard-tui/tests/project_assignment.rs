//! Task-to-project membership is an explicit native write through a picker.
mod harness;
use crossterm::event::KeyCode;
use harness::*;

#[test]
fn project_picker_assigns_cancels_and_clears_membership() {
    let mut h = Harness::new();
    seed_project(&h.root, "Delivery", "In Progress", None);
    h.app.tick();
    let id = h.app.selected_task().unwrap().id.clone();
    let screen = h.type_text("tp");
    assert!(screen.contains("Delivery"), "{screen}");
    h.type_text("Delivery");
    assert!(h
        .app
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .project
        .is_none());
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .project
            .as_deref(),
        Some("Delivery")
    );
    h.type_text("tp");
    h.press(KeyCode::Esc);
    assert_eq!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .project
            .as_deref(),
        Some("Delivery")
    );
    h.type_text("tpUnassigned");
    h.press(KeyCode::Enter);
    assert!(h
        .app
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .project
        .is_none());
    let fresh = open_app(&h.root, &h.config_path);
    assert!(fresh
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .project
        .is_none());
}
