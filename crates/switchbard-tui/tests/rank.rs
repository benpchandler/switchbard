//! `t`: the top list, a repo-wide ordered queue of any length.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn t_digit_ranks_the_selected_task_shows_the_rank_column_and_pins_a_top_section() {
    let mut h = grouped();
    goto(&mut h, "Add dark theme");
    h.press(KeyCode::Char('t'));
    assert!(
        h.app
            .status
            .starts_with("task: a number ranks it (1 is top, 1 last)"),
        "{}",
        h.app.status
    );
    let screen = h.press(KeyCode::Char('1'));
    assert_eq!(h.app.status, "TASK-2 is #1 of 1");
    assert!(screen.contains("▸ top · 1"), "{screen}");
    assert!(
        header_line(&screen).contains("5 #"),
        "rank column appeared: {screen}"
    );
    assert_eq!(screen_rows(&h)[0], "# top · 1");
    assert_eq!(screen_rows(&h)[1], "Add dark theme");
    assert_eq!(
        h.selected_title(),
        "Add dark theme",
        "cursor followed the task"
    );
    let row = screen
        .lines()
        .find(|l| l.contains("Add dark theme"))
        .unwrap();
    assert!(
        row.trim_end_matches(['│', ' ']).ends_with(" 1"),
        "rank cell: {row}"
    );
}

#[test]
fn ranks_are_ordered_open_ended_appendable_and_droppable() {
    let mut h = Harness::new();
    for n in 4..=15 {
        seed(&h.root, &format!("Extra {n}"), "To Do", &[]);
    }
    h.press(KeyCode::Char('r'));
    for (title, keys) in [
        ("Extra 4", "1"),
        ("Extra 5", "1"),
        ("Extra 6", "2"),
        ("Extra 7", "9"),
        ("Extra 8", "t"),
    ] {
        goto(&mut h, title);
        h.press(KeyCode::Char('t'));
        h.type_text(keys);
    }
    assert_eq!(
        h.app.top,
        ["TASK-5", "TASK-6", "TASK-4", "TASK-7", "TASK-8"],
        "insert-at semantics; past the end appends; tt appends"
    );
    assert_eq!(h.app.status, "TASK-8 is #5 of 5");
    for n in 9..=13 {
        goto(&mut h, &format!("Extra {n}"));
        h.press(KeyCode::Char('t'));
        h.press(KeyCode::Char('t'));
    }
    assert_eq!(h.app.top.len(), 10, "no cap");
    goto(&mut h, "Extra 14");
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('1'));
    assert_eq!(
        h.app.status, "rank: 1▏ (another digit, or enter)",
        "with ten ranked, a 1 could be 10 or 11"
    );
    h.press(KeyCode::Char('1'));
    assert_eq!(h.app.status, "TASK-14 is #11 of 11");
    goto(&mut h, "Extra 15");
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.status, "TASK-15 is #1 of 12",
        "enter commits a pending digit"
    );
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('d'));
    assert_eq!(h.app.status, "TASK-15 left the top list");
    assert_eq!(h.app.top.len(), 11);
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('d'));
    assert_eq!(h.app.status, "TASK-15 was not in the top list");
}

#[test]
fn tp_unpins_so_ranked_tasks_sit_in_their_own_sections_and_the_view_remembers() {
    let mut h = grouped();
    goto(&mut h, "Chase portal login");
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('o'));
    h.press(KeyCode::Char('1'));
    let rows = screen_rows(&h);
    assert_eq!(rows[0], "# top · 1");
    assert_eq!(rows[1], "Chase portal login");
    assert!(
        !rows[2..].contains(&"Chase portal login".to_string()),
        "left its project: {rows:?}"
    );
    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('p'));
    assert!(screen.contains("nopin"), "{screen}");
    let rows = screen_rows(&h);
    assert!(!rows.iter().any(|r| r.starts_with("# top")), "{rows:?}");
    let chase = rows
        .iter()
        .position(|r| r == "# Chase · In Progress · 1/3")
        .unwrap();
    assert_eq!(
        rows[chase + 1],
        "Chase portal login",
        "back under its project: {rows:?}"
    );
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('2'));
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(file.contains("pin = false"), "{file}");
    let fresh = open_app(&h.root, &h.config_path);
    assert!(!fresh.views.get(1).unwrap().pin_top);
}

fn grouped() -> Harness {
    let mut h = Harness::new();
    seed_project(&h.root, "Chase", "In Progress", Some("Lenders"));
    seed_in_project(&h.root, "Chase rate sheet", "Done", "Chase", None);
    seed_in_project(&h.root, "Chase portal login", "To Do", "Chase", None);
    seed_in_project(&h.root, "Chase portal MFA", "To Do", "Chase", None);
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h
}

fn goto(h: &mut Harness, title: &str) {
    h.press(KeyCode::Char('g'));
    for _ in 0..40 {
        if h.selected_title() == title {
            return;
        }
        h.press(KeyCode::Char('j'));
    }
    panic!("no row titled {title}: {:?}", screen_rows(h));
}
