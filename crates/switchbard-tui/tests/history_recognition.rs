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
    let screen = h.type_text("vh");
    assert!(screen.contains("todo:#AbCdEf"), "{screen}");
    assert!(screen.contains("todo:p1"), "{screen}");
    assert_eq!(h.app.state, before);
}

#[test]
fn narrow_preview_scrolls_to_complete_paint_details_without_restoring() {
    let mut h = Harness::new();
    h.app.state.filter = format!("日本語 {}", "unbroken".repeat(70));
    h.app.state.paint = vec![PaintRule::Column {
        column: Column::Title,
        color: "#123456".into(),
    }];
    h.app.checkpoint_session().expect("capture");
    h.terminal = Terminal::new(TestBackend::new(40, 8)).expect("narrow terminal");
    let before = h.app.state.clone();
    let first = h.type_text("vh");
    assert!(first.contains("just now"), "age stays visible: {first}");
    let mut last = first;
    for _ in 0..30 {
        last = h.press(KeyCode::PageDown);
    }
    assert!(last.contains("#123456"), "tail is readable: {last}");
    assert!(last.contains("just now"), "age remains pinned: {last}");
    assert_eq!(h.app.state, before);
    for _ in 0..30 {
        h.press(KeyCode::PageUp);
    }
    assert!(h.render().contains("日本語"));
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
