//! `t`: the Top 5, a repo-wide ordered short list that doubles as the queue.

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
            .starts_with("rank: 1-5 places this task in the top 5"),
        "{}",
        h.app.status
    );
    let screen = h.press(KeyCode::Char('1'));
    assert_eq!(h.app.status, "TASK-2 is #1");
    assert!(screen.contains("▸ top 5 · 1/5"), "{screen}");
    assert!(
        header_line(&screen).contains("5 #"),
        "rank column appeared: {screen}"
    );
    assert_eq!(screen_rows(&h)[0], "# top 5 · 1/5");
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
fn ranks_are_ordered_capped_at_five_and_droppable() {
    let mut h = Harness::new();
    for n in 4..=9 {
        seed(&h.root, &format!("Extra {n}"), "To Do", &[]);
    }
    h.press(KeyCode::Char('r'));
    for (title, place) in [
        ("Extra 4", '1'),
        ("Extra 5", '1'),
        ("Extra 6", '2'),
        ("Extra 7", '5'),
        ("Extra 8", '5'),
    ] {
        goto(&mut h, title);
        h.press(KeyCode::Char('t'));
        h.press(KeyCode::Char(place));
    }
    assert_eq!(
        h.app.top,
        ["TASK-5", "TASK-6", "TASK-4", "TASK-7", "TASK-8"],
        "insert-at semantics"
    );
    goto(&mut h, "Extra 9");
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('3'));
    assert_eq!(h.app.status, "TASK-9 is #3 · TASK-8 fell off the top 5");
    assert_eq!(h.app.top.len(), 5);
    goto(&mut h, "Extra 9");
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('d'));
    assert_eq!(h.app.status, "TASK-9 left the top 5");
    assert!(!h.app.top.contains(&"TASK-9".to_string()));
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('d'));
    assert_eq!(h.app.status, "TASK-9 was not in the top 5");
}

#[test]
fn tp_unpins_so_ranked_tasks_sit_in_their_own_sections_and_the_view_remembers() {
    let mut h = grouped();
    goto(&mut h, "Chase portal login");
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('o'));
    let rows = screen_rows(&h);
    assert_eq!(rows[0], "# top 5 · 1/5");
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
