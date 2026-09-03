//! `s`: sort orders.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn s_then_column_offers_semantic_and_plain_orders() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('3'));
    assert!(screen.contains("sort by pri"), "{screen}");
    assert!(
        screen.contains("1  semantic (high, medium, low)"),
        "{screen}"
    );
    assert!(screen.contains("2  ascending"), "{screen}");
    assert!(screen.contains("4 ✓none"), "{screen}");
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("≈pri · 3/3"), "{screen}");
    assert_eq!(
        visible_titles(&h),
        [
            "Write onboarding guide",
            "Fix login redirect loop",
            "Add dark theme"
        ]
    );
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('3'));
    h.type_text("d");
    assert!(h.render().contains("↓pri"), "{}", h.render());
    assert_eq!(visible_titles(&h)[0], "Fix login redirect loop");
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('3'));
    let screen = h.type_text("n");
    assert!(!screen.contains("pri ·"), "sort cleared: {screen}");
    assert_eq!(visible_titles(&h)[0], "Fix login redirect loop");
}

#[test]
fn sort_survives_filtering_and_title_sorts_alphabetically() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('4'));
    h.press(KeyCode::Char('1'));
    assert_eq!(visible_titles(&h)[0], "Add dark theme");
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    let screen = h.render();
    assert!(
        screen.contains("custom · status:todo · ↑title · 2/3"),
        "{screen}"
    );
    assert_eq!(
        visible_titles(&h),
        ["Add dark theme", "Write onboarding guide"]
    );
}
