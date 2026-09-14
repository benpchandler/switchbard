//! Recognition must work before restoring, including long paint-only changes.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};
use switchbard_tui::{columns::Column, paint::PaintRule};

#[test]
fn paint_only_changes_have_distinct_recognizable_labels() {
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
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(ratatui::style::Color::Rgb(0xab, 0xcd, 0xef))
    );
    h.press(KeyCode::Down);
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(ratatui::style::Color::Rgb(0xf4, 0x9f, 0x31))
    );
    assert_eq!(h.app.state, before);
}

#[test]
fn narrow_preview_scrolls_current_rows_without_restoring() {
    let mut h = Harness::new();
    for index in 0..25 {
        seed(&h.root, &format!("日本語 preview {index:02}"), "To Do", &[]);
    }
    h.type_text("r");
    h.app.checkpoint_session().expect("capture");
    h.terminal = Terminal::new(TestBackend::new(40, 8)).expect("narrow terminal");
    let before = h.app.resume_state();
    let first = h.type_text("vh");
    assert!(first.contains("just now"), "age stays visible: {first}");
    for _ in 0..10 {
        h.press(KeyCode::PageDown);
    }
    assert!(h.render().contains("Current data"));
    assert_ne!(h.render(), first);
    assert_eq!(h.app.resume_state(), before);
    for _ in 0..10 {
        h.press(KeyCode::PageUp);
    }
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
