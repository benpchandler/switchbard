//! A digit in browse: everything that can be done with that column, on letters.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn digit_opens_the_columns_menu_and_letters_run_its_actions() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("┌ status ─"), "{screen}");
    assert!(screen.contains("f  filter by its values"), "{screen}");
    assert!(screen.contains("s  sort by it"), "{screen}");
    assert!(screen.contains("p  paint by it"), "{screen}");
    assert!(screen.contains("g  glyphs on/off"), "{screen}");
    assert!(screen.contains("x  hide it"), "{screen}");
    assert!(screen.contains("m  move columns"), "{screen}");

    let screen = h.press(KeyCode::Char('f'));
    assert!(
        screen.contains("┌ status ─") && screen.contains("To Do"),
        "{screen}"
    );
    h.press(KeyCode::Char('1'));
    assert!(
        h.app.state.filter.contains("status:"),
        "{}",
        h.app.state.filter
    );
    h.press(KeyCode::Esc);

    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Char('s'));
    assert!(screen.contains("┌ sort by status"), "{screen}");
    h.press(KeyCode::Esc);

    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('g'));
    assert!(h
        .app
        .state
        .glyph_columns
        .contains(&switchbard_tui::columns::Column::Status));

    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Char('x'));
    assert!(!header_line(&screen).contains("status"), "{screen}");
}

#[test]
fn free_text_columns_offer_no_glyphs_and_out_of_range_digits_say_so() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('4'));
    assert!(screen.contains("┌ title ─"), "{screen}");
    assert!(!screen.contains("glyphs on/off"), "{screen}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('9'));
    assert_eq!(
        h.app.status,
        "no column 9: the header numbers the shown ones"
    );
}
