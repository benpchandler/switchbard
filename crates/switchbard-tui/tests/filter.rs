//! `/` and `f`: the filter language and the value picker.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn slash_filters_live_and_esc_clears() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("dark");
    assert!(screen.contains("Add dark theme"), "{screen}");
    assert!(!screen.contains("Fix login"), "{screen}");
    assert!(screen.contains("1/3"), "{screen}");
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Esc);
    assert!(screen.contains("3/3"), "{screen}");
}

#[test]
fn f_then_column_number_picks_a_value_from_the_data() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("1 ✓To Do"), "{screen}");
    assert!(screen.contains("2 ✓In Progress"), "{screen}");
    let screen = h.press(KeyCode::Char('j'));
    let screen_after_enter = h.press(KeyCode::Enter);
    assert!(
        !screen_after_enter.contains("1 To Do"),
        "picker still open: {screen_after_enter}"
    );
    assert!(
        screen_after_enter.contains("status:inprogress · 1/3"),
        "{screen} {screen_after_enter}"
    );
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(
        screen.contains("status:todo · 2/3"),
        "replacing the status term: {screen}"
    );
}

#[test]
fn f_on_a_free_text_column_drops_into_search() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('4'));
    let screen = h.type_text("guide");
    assert!(screen.contains("/guide"), "{screen}");
    assert!(screen.contains("1/3"), "{screen}");
}

#[test]
fn space_in_picker_toggles_values_and_writes_the_shortest_filter() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    let screen = h.render();
    assert!(
        screen.contains("✓To Do") && screen.contains("✓In Progress"),
        "{screen}"
    );
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains(" To Do"), "unchecked: {screen}");
    assert!(screen.contains("status:!todo · 1/3"), "{screen}");
    h.press(KeyCode::Char('j'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        screen.contains("status:!todo status:!inprogress · 0/3"),
        "{screen}"
    );
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("status:!todo · 1/3"), "re-shown: {screen}");
    h.press(KeyCode::Char('k'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("v1 · 3/3"), "all shown again: {screen}");
    let screen = h.press(KeyCode::Esc);
    assert!(!screen.contains("┌ status"), "{screen}");
}

#[test]
fn space_widens_a_single_value_filter_instead_of_fighting_it() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('j'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("· 3/3"), "{screen}");
    assert!(screen.contains("✓In Progress"), "{screen}");
}

#[test]
fn editing_the_filter_relabels_the_view_as_custom() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("pri:medium");
    assert!(
        screen.contains("custom · status:inprogress pri:medium · 1/3"),
        "{screen}"
    );
}

#[test]
fn typing_in_the_picker_narrows_and_a_unique_match_applies_at_once() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    let screen = h.type_text("o");
    assert!(screen.contains("status: o▏"), "{screen}");
    assert!(
        screen.contains("To Do") && screen.contains("In Progress"),
        "{screen}"
    );
    let screen = h.type_text("d");
    assert!(screen.contains("status:todo · 2/3"), "{screen}");
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('3'));
    let screen = h.type_text("h");
    assert!(
        screen.contains("status:todo pri:high · 1/3"),
        "stacked: {screen}"
    );
}

#[test]
fn f_opens_the_column_list_with_shown_columns_numbered_as_in_the_header() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('f'));
    assert!(screen.contains("┌ filter by column ─"), "{screen}");
    for entry in [
        "1 ✓id",
        "2 ✓status",
        "3 ✓priority",
        "4 ✓title",
        "5  labels · hidden",
        "6  project · hidden",
        "7  ball · hidden",
    ] {
        assert!(screen.contains(entry), "{entry} missing: {screen}");
    }
    let screen = h.type_text("f");
    assert!(screen.contains("filter by column: f▏"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("nothing matches 'f'"), "{screen}");
}
