//! TASK-209.2: dependency-derived blocked state, surfaced from
//! `switchbard_core::backlog_relations` — the `blocked` column and filter
//! field, a dimmed row, and the detail pane's "blocked by"/"blocks" lists.

mod harness;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_tui::config::Surface;

#[test]
fn blocked_filter_shows_only_tasks_with_an_open_dependency() {
    let mut h = Harness::new();
    // TASK-2 ("Add dark theme") is seeded "To Do" — still open — so this
    // dependent starts blocked.
    seed_with_deps(&h.root, "Ship checkout", "To Do", &["TASK-2"]);
    h.press(KeyCode::Char('r'));

    h.press(KeyCode::Char('/'));
    let screen = h.type_text("blocked:yes");
    assert!(screen.contains("Ship checkout"), "{screen}");
    assert!(!screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("1/4"), "{screen}");

    h.press(KeyCode::Enter); // commits, back to Browse
    h.press(KeyCode::Esc); // Browse-mode Esc clears the filter
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("blocked:no");
    assert!(!screen.contains("Ship checkout"), "{screen}");
    assert!(screen.contains("3/4"), "{screen}");
}

#[test]
fn blocked_filter_accepts_true_false_as_yes_no_aliases() {
    let mut h = Harness::new();
    seed_with_deps(&h.root, "Ship checkout", "To Do", &["TASK-2"]);
    h.press(KeyCode::Char('r'));

    h.press(KeyCode::Char('/'));
    let screen = h.type_text("blocked:true");
    assert!(
        screen.contains("Ship checkout") && screen.contains("1/4"),
        "{screen}"
    );

    h.press(KeyCode::Enter);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("blocked:false");
    assert!(
        !screen.contains("Ship checkout") && screen.contains("3/4"),
        "{screen}"
    );
}

#[test]
fn a_task_whose_dependency_is_done_is_not_blocked() {
    let mut h = Harness::new();
    // TASK-1 ("Fix login redirect loop") is "In Progress", not done.
    seed_with_deps(&h.root, "Ship checkout", "Done", &["TASK-1"]);
    h.press(KeyCode::Char('r'));

    h.press(KeyCode::Char('/'));
    let screen = h.type_text("blocked:yes");
    assert!(
        !screen.contains("Ship checkout"),
        "a done task is never blocked, even with an open dependency: {screen}"
    );
}

#[test]
fn blocked_row_renders_in_the_hint_style() {
    let mut h = Harness::new();
    seed_with_deps(&h.root, "Ship checkout", "To Do", &["TASK-2"]);
    h.press(KeyCode::Char('r'));
    h.render();

    let hint = h.app.config.theme.style(Surface::Hint).fg;
    assert_eq!(cell_fg(&h, "Ship checkout"), hint, "blocked row is dimmed");
    assert_ne!(
        cell_fg(&h, "Add dark theme"),
        hint,
        "an unblocked row keeps its ordinary style"
    );
}

#[test]
fn detail_pane_lists_blocked_by_and_blocks() {
    let mut h = Harness::new();
    seed_with_deps(&h.root, "Ship checkout", "To Do", &["TASK-2"]);
    h.press(KeyCode::Char('r'));

    h.press(KeyCode::Char('/'));
    h.type_text("blocked:yes");
    h.press(KeyCode::Enter);
    let detail = h.press(KeyCode::Enter);
    assert!(detail.contains("blocked by"), "{detail}");
    assert!(detail.contains("TASK-2 Add dark theme"), "{detail}");

    h.press(KeyCode::Esc); // closes the detail pane
    h.press(KeyCode::Esc); // clears the filter

    h.press(KeyCode::Char('/'));
    h.type_text("id:task-2");
    h.press(KeyCode::Enter);
    let detail = h.press(KeyCode::Enter);
    assert!(detail.contains("blocks"), "{detail}");
    assert!(detail.contains("Ship checkout"), "{detail}");
}
