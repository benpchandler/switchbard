//! Combined installed line wrapping and newly delivered automatic view history.
mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn line_wrap_and_history_shortcuts_coexist_and_restore_wrapped_details() {
    let mut h = Harness::new();
    h.type_text("vl");
    assert!(h.render().contains("Line wrap: up to 2 lines"));
    h.press(KeyCode::Esc);
    h.app.checkpoint_session().expect("checkpoint wrapped view");
    h.type_text("vl");
    assert!(h.render().contains("Line wrap: up to 3 lines"));
    h.press(KeyCode::Esc);
    h.type_text("vh");
    assert!(h.render().contains("view history"));
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.row_layout.lines(), 2);
    assert!(h.render().contains("history restored"));
    h.press(KeyCode::Enter);
    assert!(h.app.detail.focused);
    assert!(h.render().contains("Description of"));
}
