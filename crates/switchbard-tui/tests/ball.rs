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
    assert!(
        screen.contains("1/3") && screen.contains("Fix login redirect loop"),
        "{screen}"
    );
    assert_eq!(h.app.selected_task().unwrap().id, id);
}

#[test]
fn named_ball_holder_renders_filters_and_b_drops_it() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();
    switchbard_core::set_backlog_label(&h.root, &id, "ball:nick", true).unwrap();
    h.app.tick();
    h.press(KeyCode::Char('c'));
    h.type_text("b");
    h.press(KeyCode::Esc);
    assert!(h.render().contains("nick"));
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("ball:nick");
    assert!(
        screen.contains("1/3") && screen.contains("Fix login redirect loop"),
        "{screen}"
    );
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('h'));
    let screen = h.press(KeyCode::Char('b'));
    assert!(screen.contains("ball dropped"), "{screen}");
    let file = std::fs::read_dir(h.root.join("backlog/tasks"))
        .unwrap()
        .flatten()
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .find(|text| text.contains(&format!("id: {id}")))
        .unwrap();
    assert!(!file.contains("ball:"), "{file}");
}

#[test]
fn task_chord_ball_picker_selects_a_person_or_enters_a_new_one() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('b'));
    assert!(screen.contains("new person"), "{screen}");
    h.press(KeyCode::Esc);

    let other_id = h.app.tasks()[1].id.clone();
    switchbard_core::set_backlog_label(&h.root, &other_id, "ball:nick", true).unwrap();
    h.app.tick();

    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('b'));
    assert!(
        screen.contains("ball") && screen.contains("nick") && screen.contains("new person"),
        "{screen}"
    );
    let screen = h.type_text("ni");
    assert!(screen.contains("ball → nick"), "{screen}");
    assert!(h
        .app
        .selected_task()
        .unwrap()
        .labels
        .contains(&"ball:nick".to_string()));

    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('b'));
    let screen = h.press(KeyCode::Char('5'));
    assert!(screen.contains("ball person:"), "{screen}");
    h.type_text("Dana Smith");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("ball → dana-smith"), "{screen}");
    assert!(h
        .app
        .selected_task()
        .unwrap()
        .labels
        .contains(&"ball:dana-smith".to_string()));
}
