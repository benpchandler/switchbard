//! Actual keyboard and rendered list journeys through automatic history.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use switchbard_tui::page::Page;
use switchbard_tui::picker::PickerPurpose;
use switchbard_tui::views::ViewState;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

fn capture(h: &mut Harness, filter: &str, when: u64) {
    let state = ViewState {
        filter: filter.into(),
        ..ViewState::default()
    };
    let registry = h.app.registry().clone();
    h.app
        .history
        .capture(Page::Tasks, &state, &registry, when)
        .expect("history capture");
}

#[test]
fn timer_captures_without_a_save_and_history_restores_then_promotes() {
    let mut h = Harness::new();
    h.type_text("/login");
    h.press(KeyCode::Enter);
    h.app
        .checkpoint_if_due(Instant::now() + Duration::from_secs(31));
    assert!(h.root.join("views-repo.history.json").exists());
    assert!(!h.root.join("views-repo.lua").exists());
    h.type_text("v2");
    h.type_text("vh");
    let screen = h.render();
    assert!(screen.contains("view history"), "{screen}");
    assert!(screen.contains("login"), "{screen}");
    assert!(screen.contains("just now"), "{screen}");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.filter, "login");
    assert!(h.app.status.contains("history restored"));
    h.type_text("vs2");
    assert_eq!(h.app.views.get(1).expect("saved slot").filter, "login");
    let next = open_app(&h.root, &h.config_path);
    assert_eq!(next.views.get(1).expect("durable slot").filter, "login");
}

#[test]
fn menu_identity_survives_new_capture_and_search_does_not_restore_early() {
    let mut h = Harness::new();
    capture(&mut h, "login", now() - 7200);
    capture(&mut h, "theme", now() - 60);
    h.type_text("vh");
    h.type_text("login");
    assert_eq!(
        h.app.picker.as_ref().expect("picker stays open").purpose,
        PickerPurpose::History
    );
    capture(&mut h, "guide", now());
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.state.filter, "login",
        "payload is stable, not an index into moving history"
    );
    h.type_text("vh");
    h.type_text("missing view");
    let before = h.app.state.clone();
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state, before);
    assert!(h.app.status.contains("nothing matches"));
}

#[test]
fn history_is_page_scoped_and_palette_tokens_resolve_after_restoring() {
    let mut h = Harness::new();
    h.type_text("p2a");
    h.press(KeyCode::Esc);
    let painted = h.app.state.paint.clone();
    assert!(!painted.is_empty());
    assert!(painted[0].to_text(h.app.registry()).contains(":p1"));
    h.app.checkpoint_session().expect("checkpoint");
    h.type_text(":palette bloomberg");
    h.press(KeyCode::Enter);
    h.type_text("v2vh");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.paint, painted);
    let token = switchbard_tui::paint::resolve_color("p1", &h.app.config.palette);
    assert_eq!(h.app.state.paint[0].swatch(&h.app.config.palette), token);
    h.next_list_page();
    h.type_text("vh");
    assert!(!h
        .app
        .picker
        .as_ref()
        .expect("PR history")
        .options
        .iter()
        .any(|o| o.label.contains("painted by status")));
}

#[test]
fn empty_one_many_and_long_unicode_history_render_at_terminal_sizes() {
    let mut h = Harness::new();
    h.type_text("vh");
    assert!(h.app.status.contains("No view history yet"));
    h.press(KeyCode::Esc);
    capture(&mut h, "日本語 café مرحبا", now() - 172800);
    h.type_text("vh");
    assert!(h.render().contains("2d ago"));
    h.press(KeyCode::Esc);
    for index in 0..100 {
        capture(
            &mut h,
            &format!("{index} {}", "長い文字列abc".repeat(16)),
            now(),
        );
    }
    h.type_text("vh");
    for (width, height) in [(40, 8), (80, 24), (120, 40), (180, 50), (20, 4), (0, 0)] {
        h.terminal.backend_mut().resize(width, height);
        h.terminal
            .resize(ratatui::layout::Rect::new(0, 0, width, height))
            .expect("resize");
        let screen = h.render();
        assert_eq!(h.terminal.backend().buffer().area.width, width);
        if width >= 80 {
            assert!(screen.contains("view history"), "{screen}");
        }
    }
}

#[test]
fn timer_captures_committed_paint_while_picker_remains_open() {
    let mut h = Harness::new();
    h.type_text("p2a");
    assert_eq!(h.app.mode, switchbard_tui::app::Mode::PickValue);
    let expected = h.app.state.paint.clone();
    h.app
        .checkpoint_if_due(Instant::now() + Duration::from_secs(31));
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    assert!(!expected.is_empty());
    assert_eq!(next.state.paint, expected);
    assert_eq!(next.history.entries(Page::Tasks, now()).len(), 1);
}

#[test]
fn multiline_and_control_text_roundtrips_through_shared_lua_record() {
    let mut h = Harness::new();
    let filter = "日本語\nsecond\r\t\0\u{1b}9";
    capture(&mut h, filter, now());
    h.app.state.filter = filter.into();
    h.app.checkpoint_session().expect("shared multiline codec");
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    assert_eq!(next.state.filter, filter);
    assert_eq!(
        next.history.entries(Page::Tasks, now())[0]
            .view(next.registry())
            .expect("history")
            .filter,
        filter
    );
}
