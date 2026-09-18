//! `s`: sort orders.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn s_then_column_offers_semantic_and_plain_orders() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('3'));
    // The title is the breadcrumb: which layer, and what is settled above it.
    assert!(screen.contains("sort pri"), "{screen}");
    assert!(
        screen.contains("1  semantic (high, medium, low)"),
        "{screen}"
    );
    assert!(screen.contains("2  ascending"), "{screen}");
    assert!(screen.contains("4 ✓none"), "{screen}");
    let screen = h.press(KeyCode::Char('1'));
    // TASK-235 moved the sort label into the footer's view-settings summary.
    assert!(
        screen
            .lines()
            .nth(1)
            .is_some_and(|context| context.contains("3/3 shown")),
        "{screen}"
    );
    assert!(screen.contains("≈pri"), "{screen}");
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
    assert!(
        !screen.contains("≈pri") && !screen.contains("↑pri") && !screen.contains("↓pri"),
        "sort cleared: {screen}"
    );
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
        screen
            .lines()
            .nth(1)
            .is_some_and(|context| context.contains("2/3 shown") && context.contains(" custom ")),
        "{screen}"
    );
    assert!(
        screen
            .lines()
            .nth(2)
            .is_some_and(|context| context.contains("/ status:todo")),
        "{screen}"
    );
    assert!(screen.contains("↑title"), "{screen}");
    assert_eq!(
        visible_titles(&h),
        ["Add dark theme", "Write onboarding guide"]
    );
}

#[test]
fn numeric_ids_and_equal_text_have_deterministic_order_through_real_keys() {
    let mut h = Harness::new();
    for _ in 0..9 {
        seed(&h.root, "Duplicate title", "To Do", &[]);
    }
    h.tick_until_tasks_settle();
    choose_sort(&mut h, "id", "ascending");
    let screen = h.render();
    assert!(screen.contains("↑id"), "{screen}");
    let ids: Vec<_> = h
        .app
        .visible
        .iter()
        .map(|&i| h.app.tasks()[i].id.clone())
        .collect();
    assert_eq!(&ids[8..12], ["TASK-9", "TASK-10", "TASK-11", "TASK-12"]);
    choose_sort(&mut h, "title", "ascending");
    let first = h.app.visible.clone();
    choose_sort(&mut h, "title", "descending");
    choose_sort(&mut h, "title", "ascending");
    assert_eq!(h.app.visible, first);
    assert!(h.render().contains("↑title"));
}

fn choose_sort(h: &mut Harness, column: &str, order: &str) {
    h.press(KeyCode::Char('s'));
    for label in [column, order] {
        let picker = h.app.picker.as_ref().expect("sort picker");
        let index = picker
            .matching()
            .iter()
            .position(|option| option.label.starts_with(label))
            .expect("sort choice");
        for _ in 0..picker.selected {
            h.press(KeyCode::Up);
        }
        for _ in 0..index {
            h.press(KeyCode::Down);
        }
        h.press(KeyCode::Enter);
    }
}

/// Picks the row with this exact label in whatever picker is open, by arrowing
/// to it - no reliance on a number that shifts as the catalog grows.
fn pick(h: &mut Harness, label: &str) {
    let picker = h.app.picker.as_ref().expect("picker open");
    let index = picker
        .matching()
        .iter()
        .position(|option| option.label.starts_with(label))
        .unwrap_or_else(|| panic!("no row {label:?} in {:?}", picker.row_keys()));
    for _ in 0..picker.selected {
        h.press(KeyCode::Up);
    }
    for _ in 0..index {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
}

/// The whole gesture the owner asked for: `s`, a column, an order - that layer
/// locks and the breadcrumb shows it - then Tab for a second layer that decides
/// the ties the first one leaves, then Enter.
#[test]
fn tab_adds_a_second_layer_that_breaks_the_first_ones_ties() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("sort ≈status"), "{screen}");
    // Semantic status alone leaves both To Do tasks tied, so id decides.
    assert_eq!(
        visible_titles(&h),
        [
            "Add dark theme",
            "Write onboarding guide",
            "Fix login redirect loop"
        ]
    );
    h.press(KeyCode::Tab);
    h.press(KeyCode::Char('4'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("sort ≈status › ↓title"), "{screen}");
    // Layer two reverses the tied pair without touching the status order.
    assert_eq!(
        visible_titles(&h),
        [
            "Write onboarding guide",
            "Add dark theme",
            "Fix login redirect loop"
        ]
    );
    let screen = h.press(KeyCode::Enter);
    assert_eq!(h.app.state.sort.len(), 2);
    assert!(screen.contains("≈status › ↓title"), "{screen}");
}

/// A single letter names a column and an order, so the whole two-layer sort is
/// `s i a tab t d ⏎`.
#[test]
fn letters_name_the_column_and_the_order() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.type_text("ia");
    let screen = h.render();
    assert!(screen.contains("sort ↑id"), "{screen}");
    h.press(KeyCode::Tab);
    h.type_text("td");
    let screen = h.render();
    assert!(screen.contains("sort ↑id › ↓title"), "{screen}");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app
            .state
            .sort
            .iter()
            .map(|sort| sort.to_text(h.app.registry()))
            .collect::<Vec<_>>(),
        ["id:ascending", "title:descending"]
    );
}

/// Undo walks back out of the breadcrumb exactly the way it was built: the
/// order first, then the column, then the layer above it, then the entry.
#[test]
fn left_backspace_and_shift_tab_each_step_back_one_breadcrumb_crumb() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.type_text("ia");
    h.press(KeyCode::Tab);
    h.type_text("td");
    assert_eq!(h.app.state.sort.len(), 2);

    // Undo the order of layer two: back to choosing title's order.
    h.press(KeyCode::Left);
    assert_eq!(h.app.state.sort.len(), 1);
    let screen = h.render();
    assert!(screen.contains("sort ↑id › title"), "{screen}");

    // Undo the column of layer two: back to choosing a column for it.
    h.press(KeyCode::Backspace);
    let screen = h.render();
    assert!(screen.contains("sort ↑id › "), "{screen}");

    // Undo layer one's order, then its column, then the entry itself.
    h.press(KeyCode::BackTab);
    assert!(h.app.state.sort.is_empty());
    h.press(KeyCode::Left);
    h.press(KeyCode::Left);
    assert!(h.app.picker.is_none());
    assert!(h.app.state.sort.is_empty());
    assert!(h.app.sort_breadcrumb().is_empty());
}

/// Esc settles, it does not take anything back - the same thing it does in every
/// other sbt picker, which all apply as you pick.
#[test]
fn esc_settles_the_stack_the_way_enter_does() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.type_text("ia");
    h.press(KeyCode::Tab);
    h.type_text("td");
    let shown = h.app.state.sort.clone();
    assert_eq!(shown.len(), 2);
    h.press(KeyCode::Esc);
    assert_eq!(h.app.state.sort, shown);
    assert!(h.app.sort_breadcrumb().is_empty());
    assert!(h.app.picker.is_none());
}

/// Undoing every layer is a deliberate "no sort", so walking off the front of the
/// breadcrumb leaves the list unsorted rather than resurrecting what `s` opened over.
#[test]
fn undoing_past_the_first_crumb_leaves_the_list_unsorted() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.type_text("ia");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.sort.len(), 1);

    // Reopening seeds the breadcrumb with the stack already in force, so undo
    // walks back through it: the order, the column, then off the front.
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Left);
    assert!(h.app.state.sort.is_empty(), "the order is undone first");
    h.press(KeyCode::Left);
    h.press(KeyCode::Left);
    assert!(h.app.picker.is_none());
    assert!(h.app.sort_breadcrumb().is_empty());
}

/// Five is the ceiling, and Tab says so rather than silently doing nothing.
#[test]
fn a_sixth_layer_is_refused_with_the_limit_named() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    for (column, order) in [
        ("id", "ascending"),
        ("status", "semantic"),
        ("priority", "semantic"),
        ("title", "ascending"),
        ("labels", "ascending"),
    ] {
        pick(&mut h, column);
        pick(&mut h, order);
        if h.app.state.sort.len() < 5 {
            h.press(KeyCode::Tab);
        }
    }
    assert_eq!(h.app.state.sort.len(), 5);
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("5 sort layers is the limit"), "{screen}");
    assert_eq!(h.app.state.sort.len(), 5);
}

/// A column sorts at one depth or another, never two: re-picking it moves it.
#[test]
fn re_picking_a_column_moves_its_layer_instead_of_repeating_it() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.type_text("ia");
    h.press(KeyCode::Tab);
    h.type_text("td");
    h.press(KeyCode::Tab);
    h.type_text("ia");
    let screen = h.render();
    assert!(screen.contains("sort ↓title › ↑id"), "{screen}");
    assert_eq!(h.app.state.sort.len(), 2);
}

/// The whole stack has to survive the round trip through a saved view, not just
/// its first layer.
#[test]
fn a_saved_view_keeps_every_layer() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.type_text("ia");
    h.press(KeyCode::Tab);
    h.type_text("td");
    h.press(KeyCode::Enter);
    let stack = h.app.state.sort.clone();
    h.type_text("vs1");
    h.app = harness::open_app(&h.root, &h.config_path);
    assert_eq!(h.app.state.sort, stack);
}
