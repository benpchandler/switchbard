//! `b`: who holds the ball.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn b_passes_the_ball_me_agent_nobody_and_writes_the_label() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Char('c'));
    h.type_text("b");
    h.press(KeyCode::Esc);
    assert!(header_line(&h.render()).contains("5 ball"));
    let screen = h.press(KeyCode::Char('b'));
    assert!(screen.contains(&format!("{id}: ball → me")), "{screen}");
    let file = std::fs::read_dir(h.root.join("backlog/tasks"))
        .unwrap()
        .flatten()
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .find(|t| t.contains(&format!("id: {id}")))
        .unwrap();
    assert!(file.contains("- ball:me\n"), "{file}");
    let screen = h.press(KeyCode::Char('b'));
    assert!(screen.contains("ball → agent"), "{screen}");
    assert_eq!(
        h.app
            .selected_task()
            .unwrap()
            .labels
            .iter()
            .filter(|l| l.starts_with("ball:"))
            .count(),
        1
    );
    let screen = h.press(KeyCode::Char('b'));
    assert!(screen.contains("ball dropped"), "{screen}");
    assert!(!h
        .app
        .selected_task()
        .unwrap()
        .labels
        .iter()
        .any(|l| l.starts_with("ball:")));
}

#[test]
fn ball_filters_sorts_and_the_starter_view_is_my_inbox() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('b'));
    let mine = h.app.selected_task().unwrap().title.clone();
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('5'));
    assert!(screen.contains("v5 · ball:me · 1/3"), "{screen}");
    assert!(screen.contains(&mine), "{screen}");
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('s'));
    h.type_text("b");
    h.type_text("d");
    assert_eq!(visible_titles(&h)[0], mine, "descending puts me first");
    assert!(h.render().contains("↓ball"));
}

#[test]
fn a_dispatching_task_reads_as_agent_without_a_ball_label() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();
    switchbard_core::set_backlog_label(&h.root, &id, "dispatching", true).unwrap();
    h.app.tick();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("ball:agent");
    assert!(screen.contains("1/3") && screen.contains(&id), "{screen}");
}
