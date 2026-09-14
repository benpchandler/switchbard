//! Recognition must work before restoring, including long paint-only changes.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};
use switchbard_tui::{columns::Column, paint::PaintRule};

#[test]
fn paint_only_changes_have_distinct_visible_miniatures() {
    let mut h = Harness::new();
    for color in ["p1", "#AbCdEf"] {
        h.app.state.paint = vec![PaintRule::ByColumn {
            column: Column::Status,
            colors: vec![("todo".into(), color.into())],
        }];
        h.app.checkpoint_session().expect("capture");
    }
    let before = h.app.state.clone();
    h.type_text("vh");
    let colors = h
        .terminal
        .backend()
        .buffer()
        .content
        .chunks(100)
        .filter_map(|row| {
            let text = row.iter().map(|cell| cell.symbol()).collect::<String>();
            let byte = text.find("Add dark theme")?;
            let column = text[..byte].chars().count();
            Some(row[column].fg)
        })
        .collect::<Vec<_>>();
    assert!(colors.contains(&ratatui::style::Color::Rgb(0xab, 0xcd, 0xef)));
    assert!(colors.contains(&ratatui::style::Color::Rgb(0xf4, 0x9f, 0x31)));
    h.press(KeyCode::Down);
    assert_eq!(h.app.state, before);
}

#[test]
fn narrow_preview_pages_between_cards_without_restoring() {
    let mut h = Harness::new();
    for query in ["login", "theme", "guide"] {
        h.type_text(&format!("/{query}"));
        h.press(KeyCode::Enter);
        h.app.checkpoint_session().expect("capture separate view");
        h.press(KeyCode::Esc);
    }
    h.terminal = Terminal::new(TestBackend::new(40, 8)).expect("narrow terminal");
    let before = h.app.resume_state();
    let first = h.type_text("vh");
    assert!(first.contains("just now"), "age stays visible: {first}");
    h.press(KeyCode::PageDown);
    assert_eq!(h.app.picker.as_ref().expect("picker").selected, 2);
    assert!(h.render().contains("Current data"));
    assert_ne!(h.render(), first);
    assert_eq!(h.app.resume_state(), before);
    h.press(KeyCode::PageUp);
    assert_eq!(h.app.picker.as_ref().expect("picker").selected, 0);
    assert_eq!(h.render(), first);
}

#[test]
fn history_handles_zero_short_current_and_wide_containers_and_no_matches() {
    let mut h = Harness::new();
    h.app.checkpoint_session().expect("capture");
    h.type_text("vh");
    for (width, height) in [(0, 0), (1, 1), (20, 5), (40, 8), (100, 20), (180, 50)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        h.render();
    }
    assert!(h
        .type_text("no-such-arrangement")
        .contains("No matching history"));
}

#[test]
fn every_card_keeps_relative_time_visible_beside_long_titles() {
    let mut h = Harness::new();
    for query in ["長い検索".repeat(40), "theme".into()] {
        h.type_text(&format!("/{query}"));
        h.press(KeyCode::Enter);
        h.app.checkpoint_session().expect("capture");
        h.press(KeyCode::Esc);
    }
    h.type_text("vh");
    let screen = h.render();
    assert_eq!(
        screen.matches("just now").count(),
        2,
        "each card owns visible age: {screen}"
    );
    assert!(
        screen.contains('…'),
        "long title clips deliberately: {screen}"
    );
    h.terminal = Terminal::new(TestBackend::new(40, 8)).expect("short terminal");
    h.press(KeyCode::Down);
    assert!(
        h.render().contains("just now"),
        "short card retains age: {}",
        h.render()
    );
}
