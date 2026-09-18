//! `/` and `f`: the filter language and the value picker.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn column_menu_badges_count_values_independently_of_other_filters() {
    for (query, expected) in [
        ("status:todo", "status (1/2 shown)"),
        ("status:!todo", "status (1/2 shown)"),
        ("status:missing", "status (0/2 shown)"),
        ("status:todo,inprogress", "status (2/2 shown)"),
        ("status:todo pri:high", "status (1/2 shown)"),
        ("label:bug", "labels · hidden (1/4 shown)"),
    ] {
        let mut h = Harness::new();
        h.press(KeyCode::Char('/'));
        h.type_text(query);
        h.press(KeyCode::Enter);
        let screen = h.press(KeyCode::Char('f'));
        assert!(screen.contains(expected), "{query}: {screen}");
    }
}

#[test]
fn column_badge_updates_after_value_toggle_and_disappears_when_cleared() {
    let mut h = Harness::new();
    assert!(!h.press(KeyCode::Char('f')).contains("shown)"));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char(' '));
    let screen = h.press(KeyCode::Left);
    assert!(screen.contains("status (1/2 shown)"), "{screen}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Esc);
    assert!(!h.press(KeyCode::Char('f')).contains("shown)"));
}

#[test]
fn column_filter_badge_fits_narrow_and_wide_terminals() {
    for (width, height) in [(40, 12), (80, 24), (180, 50)] {
        let mut h = Harness::new();
        h.terminal.backend_mut().resize(width, height);
        h.press(KeyCode::Char('/'));
        h.type_text("status:todo");
        h.press(KeyCode::Enter);
        let screen = h.press(KeyCode::Char('f'));
        assert!(
            screen.contains("status (1/2 shown)"),
            "{width}x{height}: {screen}"
        );
        h.press(KeyCode::Char('2'));
        assert!(h.render().contains("✓To Do"));
    }
}

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
    // TASK-235: the filter renders on its own line inside the frame (line 2),
    // not the title line (line 1).
    assert!(
        Some(title_border(&screen_after_enter))
            .is_some_and(|context| context.contains("1/3 shown")),
        "{screen} {screen_after_enter}"
    );
    assert!(
        under_title_border(&screen_after_enter)
            .is_some_and(|context| context.contains("/ status:inprogress")),
        "{screen} {screen_after_enter}"
    );
    assert_eq!(h.app.state.filter, "status:inprogress");
    assert_eq!(visible_titles(&h), ["Fix login redirect loop"]);
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(
        Some(title_border(&screen)).is_some_and(|context| context.contains("2/3 shown")),
        "replacing the status term: {screen}"
    );
    assert!(
        under_title_border(&screen).is_some_and(|context| context.contains("/ status:todo")),
        "replacing the status term: {screen}"
    );
    assert_eq!(h.app.state.filter, "status:todo");
    assert_eq!(visible_titles(&h).len(), 2);
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
    // TASK-235: the filter is on its own line (line 2) below the title (line 1).
    assert!(
        Some(title_border(&screen)).is_some_and(|context| context.contains("1/3 shown")),
        "{screen}"
    );
    assert!(
        under_title_border(&screen).is_some_and(|context| context.contains("/ status:!todo")),
        "{screen}"
    );
    h.press(KeyCode::Char('j'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        Some(title_border(&screen)).is_some_and(|context| context.contains("0/3 shown")),
        "{screen}"
    );
    assert!(
        under_title_border(&screen)
            .is_some_and(|context| context.contains("/ status:!todo status:!inprogress")),
        "{screen}"
    );
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        Some(title_border(&screen)).is_some_and(|context| context.contains("1/3 shown")),
        "re-shown: {screen}"
    );
    assert!(
        under_title_border(&screen).is_some_and(|context| context.contains("/ status:!todo")),
        "re-shown: {screen}"
    );
    h.press(KeyCode::Char('k'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        Some(title_border(&screen))
            .is_some_and(|context| context.contains("3/3 shown") && context.contains(" v1 ")),
        "all shown again: {screen}"
    );
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
    assert!(
        Some(title_border(&screen)).is_some_and(|context| context.contains("3/3 shown")),
        "{screen}"
    );
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
        Some(title_border(&screen))
            .is_some_and(|context| context.contains("1/3 shown") && context.contains(" custom ")),
        "{screen}"
    );
    assert!(
        under_title_border(&screen)
            .is_some_and(|context| context.contains("/ status:inprogress pri:medium")),
        "{screen}"
    );
    assert_eq!(h.app.state.filter, "status:inprogress pri:medium");
    assert_eq!(visible_titles(&h), ["Fix login redirect loop"]);
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
    assert!(
        Some(title_border(&screen)).is_some_and(|context| context.contains("2/3 shown")),
        "{screen}"
    );
    assert!(
        under_title_border(&screen).is_some_and(|context| context.contains("/ status:todo")),
        "{screen}"
    );
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('3'));
    let screen = h.type_text("h");
    assert!(
        Some(title_border(&screen)).is_some_and(|context| context.contains("1/3 shown")),
        "stacked: {screen}"
    );
    assert!(
        under_title_border(&screen)
            .is_some_and(|context| context.contains("/ status:todo pri:high")),
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
    let screen = h.type_text("z");
    assert!(screen.contains("filter by column: z▏"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("nothing matches 'z'"), "{screen}");
}

#[test]
fn composed_multivalue_missing_and_unknown_fields_keep_query_semantics() {
    let mut h = Harness::new();
    for (query, count) in [
        ("login label:bug,docs status:!done", "1/3"),
        ("label:auth label:bug", "1/3"),
        ("project:missing", "0/3"),
        ("project:!missing", "3/3"),
        ("checks:passed", "0/3"),
        ("checks:!passed", "3/3"),
        ("unknown:value", "0/3"),
        ("status:", "3/3"),
        ("id:task-1", "1/3"),
    ] {
        h.press(KeyCode::Esc);
        h.press(KeyCode::Char('/'));
        h.type_text(query);
        let screen = h.press(KeyCode::Enter);
        assert!(screen.contains(count), "{query}: {screen}");
    }
    let screen = h.press(KeyCode::Esc);
    assert!(screen.contains("3/3"), "{screen}");
}
