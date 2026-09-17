//! Word-processor-style range select on the Tasks list: `space` marks the
//! cursor row, shift-down/shift-up extend the mark from an anchor — the same
//! shared `Selection` shape and contract as `tests/pr_range_select.rs`.
mod harness;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness::*;
use switchbard_tui::columns::bare_id;
use switchbard_tui::group::Row;

fn shift_down(h: &mut Harness) -> String {
    h.app
        .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    h.render()
}

fn shift_up(h: &mut Harness) -> String {
    h.app
        .handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT));
    h.render()
}

/// Task ids in the order the rows currently show them, headings excluded.
fn ordered_ids(h: &Harness) -> Vec<String> {
    h.app
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Task(index) => Some(h.app.tasks()[*index].id.clone()),
            Row::Heading { .. } => None,
        })
        .collect()
}

/// The default three-task harness, cursor on the first row.
fn three_tasks() -> Harness {
    let mut h = Harness::new();
    h.press(KeyCode::Char('g')); // top
    h
}

#[test]
fn space_marks_a_task_row_and_the_screen_shows_it() {
    let mut h = three_tasks();
    let id = h.app.selected_task().unwrap().id.clone();
    let plain = h.render();
    assert!(!plain.contains("[x]") && !plain.contains("[ ]"), "{plain}");

    let screen = h.press(KeyCode::Char(' '));
    assert_eq!(h.app.task_selection.marked.len(), 1);
    assert!(h.app.task_selection.marked.contains(&id));
    assert!(h.app.status.contains("Marked (1)"), "{}", h.app.status);
    assert!(
        screen.contains(&format!("[x] {}", bare_id(&id))),
        "{screen}"
    );
    assert!(screen.contains("[ ]"), "{screen}");

    // Toggling again unmarks and the checkbox disappears once the last mark
    // is gone.
    h.press(KeyCode::Char('g'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(h.app.task_selection.marked.is_empty());
    assert!(
        !screen.contains("[x]") && !screen.contains("[ ]"),
        "{screen}"
    );
}

#[test]
fn esc_clears_task_marks_before_touching_anything_else() {
    let mut h = three_tasks();
    h.press(KeyCode::Char(' '));
    assert_eq!(h.app.task_selection.marked.len(), 1);
    h.press(KeyCode::Esc);
    assert!(h.app.task_selection.marked.is_empty());
    assert!(!h.render().contains("[ ]"));
}

#[test]
fn shift_down_extends_a_contiguous_range_and_the_screen_shows_it() {
    let mut h = three_tasks();
    let ids = ordered_ids(&h);

    let screen = shift_down(&mut h);
    assert_eq!(
        h.app.marked_tasks_in_view_order(),
        vec![ids[0].clone(), ids[1].clone()]
    );
    assert!(h.app.status.contains("Marked (2)"), "{}", h.app.status);
    assert!(
        screen.contains(&format!("[x] {}", bare_id(&ids[0]))),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("[x] {}", bare_id(&ids[1]))),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("[ ] {}", bare_id(&ids[2]))),
        "{screen}"
    );
}

#[test]
fn shift_up_contracts_the_range_and_unmarks_what_left_it() {
    let mut h = three_tasks();
    let ids = ordered_ids(&h);
    shift_down(&mut h); // anchor row0, cursor row1: marks row0, row1
    shift_down(&mut h); // cursor row2: marks row0..row2
    assert_eq!(h.app.marked_tasks_in_view_order(), ids);

    shift_up(&mut h); // cursor back to row1: row2 leaves the range
    assert_eq!(
        h.app.marked_tasks_in_view_order(),
        vec![ids[0].clone(), ids[1].clone()]
    );
}

#[test]
fn a_row_marked_outside_the_sweep_survives_a_contraction_that_passes_over_it() {
    let mut h = three_tasks();
    let ids = ordered_ids(&h);

    // Mark the last row by hand, independent of any sweep.
    h.press(KeyCode::Char('e')); // bottom
    h.press(KeyCode::Char(' '));
    assert_eq!(h.app.marked_tasks_in_view_order(), vec![ids[2].clone()]);

    // Start a fresh sweep from the top and extend down across it.
    h.press(KeyCode::Char('g'));
    shift_down(&mut h); // row0, row1
    shift_down(&mut h); // + row2 (already marked outside the sweep)
    assert_eq!(h.app.marked_tasks_in_view_order(), ids);

    // Contract past row2 entirely: the sweep never owned it, so it must
    // still be marked once the range no longer covers it.
    shift_up(&mut h); // cursor row1: range is now row0..row1
    assert_eq!(
        h.app.marked_tasks_in_view_order(),
        ids,
        "row2 was marked by space outside the sweep and must survive"
    );
}

#[test]
fn a_plain_down_between_two_extends_resets_the_anchor() {
    let mut h = three_tasks();
    let ids = ordered_ids(&h);
    shift_down(&mut h); // anchor row0, cursor row1
    assert_eq!(
        h.app.marked_tasks_in_view_order(),
        vec![ids[0].clone(), ids[1].clone()]
    );

    // A plain Down moves the cursor without marking further.
    h.press(KeyCode::Down);
    assert_eq!(
        h.app.marked_tasks_in_view_order(),
        vec![ids[0].clone(), ids[1].clone()],
        "a plain move must not mark anything"
    );
    assert_eq!(
        h.app.selected_task().map(|task| task.id.clone()),
        Some(ids[2].clone())
    );

    // Extending upward now must anchor fresh at row2 (where the plain move
    // left the cursor), sweeping it in on the way back to row1.
    shift_up(&mut h);
    assert_eq!(h.app.marked_tasks_in_view_order(), ids);
}

#[test]
fn a_sweep_crossing_a_group_heading_marks_the_tasks_not_the_heading() {
    let mut h = Harness::new();
    seed_project(&h.root, "Chase", "In Progress", None);
    seed_in_project(&h.root, "Chase intake", "To Do", "Chase", None);
    seed_in_project(&h.root, "Chase review", "To Do", "Chase", None);
    h.press(KeyCode::Char('o'));
    h.press(KeyCode::Char('1')); // organize by project
    assert!(
        screen_rows(&h).iter().any(|row| row.starts_with('#')),
        "expected a heading row: {:?}",
        screen_rows(&h)
    );
    h.press(KeyCode::Char('g')); // top: lands on the first task row, never a heading

    assert_eq!(h.app.marked_tasks_in_view_order().len(), 0);
    shift_down(&mut h);
    shift_down(&mut h);
    shift_down(&mut h);
    // Every marked id names a real task; no heading text ever entered the set.
    let marked = h.app.marked_tasks_in_view_order();
    assert!(!marked.is_empty(), "expected the sweep to mark tasks");
    for id in &marked {
        assert!(
            h.app.tasks().iter().any(|task| &task.id == id),
            "{id} must be a real task id, never a heading"
        );
    }
}

#[test]
fn help_lists_the_selection_keys_on_tasks_and_pull_requests() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("mark"), "Tasks help: {screen}");
    assert!(screen.contains("extend_down"), "Tasks help: {screen}");
    assert!(screen.contains("extend_up"), "Tasks help: {screen}");
    h.press(KeyCode::Esc);
    h.next_list_page();
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("mark"), "PR help: {screen}");
    assert!(screen.contains("extend_down"), "PR help: {screen}");
    assert!(screen.contains("extend_up"), "PR help: {screen}");
}
