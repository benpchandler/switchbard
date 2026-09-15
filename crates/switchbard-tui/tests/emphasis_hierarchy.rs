//! Scanning hierarchy, real key events and rendered cells across terminal sizes.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, buffer::Cell, style::Modifier, Terminal};

fn cell(h: &Harness, needle: &str) -> Cell {
    let buffer = h.terminal.backend().buffer();
    for row in buffer.content.chunks(usize::from(buffer.area.width).max(1)) {
        let text: String = row.iter().map(|cell| cell.symbol()).collect();
        if let Some(offset) = text.find(needle) {
            return row[text[..offset].chars().count()].clone();
        }
    }
    panic!("missing {needle}");
}

fn command(h: &mut Harness, text: &str) {
    h.type_text(&format!(":{text}"));
    h.press(KeyCode::Enter);
}

#[test]
fn priority_completion_navigation_and_sort_are_distinct_without_color_alone() {
    let mut h = Harness::new();
    seed(&h.root, "Completed handoff", "Done", &[]);
    h.app.tick();
    h.render();
    assert!(cell(&h, "Write onboarding")
        .modifier
        .contains(Modifier::BOLD));
    assert!(!cell(&h, "Add dark theme").modifier.contains(Modifier::BOLD));
    assert!(cell(&h, "Completed handoff")
        .modifier
        .contains(Modifier::CROSSED_OUT));
    assert!(cell(&h, "[Tasks]")
        .modifier
        .contains(Modifier::UNDERLINED | Modifier::BOLD));
    assert_ne!(
        Some(cell(&h, "[Tasks]").bg),
        h.app
            .config
            .theme
            .style(switchbard_tui::config::Surface::AttentionBadge)
            .bg
    );
    h.type_text("s3");
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Esc);
    assert!(cell(&h, "3 pri").modifier.contains(Modifier::UNDERLINED));
    assert!(!cell(&h, "2 status").modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn grouping_paints_raw_value_without_matching_its_rollup_and_keeps_selection() {
    let mut h = Harness::new();
    seed_project(&h.root, "Delivery", "In Progress", None);
    seed_in_project(
        &h.root,
        "Ship readable hierarchy",
        "To Do",
        "Delivery",
        None,
    );
    h.app.tick();
    command(&mut h, "outline project");
    command(
        &mut h,
        "paint heading:Delivery=alert+struck;header=strong;title=quiet",
    );
    assert!(cell(&h, "Delivery")
        .modifier
        .contains(Modifier::CROSSED_OUT));
    assert!(!cell(&h, "Ship readable")
        .modifier
        .contains(Modifier::CROSSED_OUT));
    let selected = h.selected_title();
    h.type_text("ph");
    h.press(KeyCode::Esc);
    assert_eq!(selected, h.selected_title());
}

#[test]
fn hierarchy_survives_small_empty_wrapped_and_wide_boards() {
    let mut h = Harness::new();
    seed(
        &h.root,
        &format!("日本語 café {}", "unbroken".repeat(45)),
        "To Do",
        &[],
    );
    h.app.tick();
    command(&mut h, "outline status");
    command(
        &mut h,
        "paint heading:todo=strong;header=strong;title=quiet",
    );
    for (width, height) in [(0, 0), (1, 1), (40, 8), (100, 24), (160, 40)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        h.render();
        h.press(KeyCode::Char('j'));
        h.type_text("ph");
        h.press(KeyCode::Esc);
        let screen = h.render();
        if width >= 40 && height >= 8 {
            assert!(screen.contains("Tasks"), "{screen}");
            assert!(screen.contains("shown"), "counts stay findable: {screen}");
        }
    }
    h.type_text("/no-task-matches-this");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("0/4 shown"), "{screen}");
    assert!(screen.contains("no-task-matches-this"), "{screen}");
}

#[test]
fn title_scope_reaches_both_list_titles_and_preserves_filter_context() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    h.type_text("/status:todo");
    h.press(KeyCode::Enter);
    let filter_ink = cell(&h, "/ status:todo").fg;
    command(&mut h, "paint title=#123456");
    assert_eq!(cell(&h, "custom").fg, Color::Rgb(0x12, 0x34, 0x56));
    assert_eq!(cell(&h, "/ status:todo").fg, filter_ink);
    h.press(KeyCode::Tab);
    command(&mut h, "paint title=#654321");
    assert_eq!(
        h.terminal.backend().buffer()[(2, 1)].fg,
        Color::Rgb(0x65, 0x43, 0x21)
    );
    h.next_list_page();
    assert_eq!(cell(&h, "custom").fg, Color::Rgb(0x12, 0x34, 0x56));
}

#[test]
fn light_popups_keep_the_declared_canvas_after_clearing_the_list() {
    let mut h = Harness::new();
    command(&mut h, "theme light");
    h.press(KeyCode::Char('p'));
    assert_eq!(
        Some(cell(&h, "title band").bg),
        h.app.config.theme.background()
    );
    h.press(KeyCode::Char('h'));
    assert_eq!(
        Some(cell(&h, "quiet").bg),
        h.app
            .config
            .theme
            .style(switchbard_tui::config::Surface::Selected)
            .bg
    );
    assert_eq!(Some(cell(&h, "strong").bg), h.app.config.theme.background());
    h.press(KeyCode::Esc);
    h.type_text("vh");
    assert_eq!(
        Some(cell(&h, "view history").bg),
        h.app.config.theme.background()
    );
}
