//! TASK-235: the title line holds only the repo chip, the shown count and the
//! view name; the active filter gets its own wrapping line inside the list
//! frame; every other view setting moves to the footer hint bar. TASK-234:
//! header cells split the column number (`keys` surface) from its name
//! (`header` surface).
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::style::{Modifier, Style};

const SIZES: [(u16, u16); 3] = [(40, 8), (100, 24), (160, 40)];

fn command(h: &mut Harness, text: &str) {
    h.type_text(&format!(":{text}"));
    h.press(KeyCode::Enter);
}

fn resize(h: &mut Harness, width: u16, height: u16) -> String {
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    h.render()
}

/// Style of the character at `text[offset]` on the first row that contains
/// `text` verbatim (fg/bg/modifiers only — position, not a live buffer cell).
fn cell_style(h: &Harness, text: &str, offset: usize) -> Style {
    let buffer = h.terminal.backend().buffer();
    let width = usize::from(buffer.area.width).max(1);
    for row in buffer.content.chunks(width) {
        let line: String = row.iter().map(|cell| cell.symbol()).collect();
        if let Some(start) = line.find(text) {
            let column = line[..start].chars().count() + offset;
            let cell = &row[column];
            return Style::default()
                .fg(cell.fg)
                .bg(cell.bg)
                .add_modifier(cell.modifier);
        }
    }
    panic!(
        "missing {text:?} in:\n{}",
        switchbard_tui::view::buffer_text(buffer)
    );
}

// --- TASK-235: title line ---------------------------------------------------

#[test]
fn title_line_holds_only_repo_count_and_view_name_with_settings_in_the_footer() {
    for (width, height) in SIZES {
        let mut h = Harness::new();
        resize(&mut h, width, height);
        // Populate view settings the title line used to carry: outline and,
        // unambiguously (column 3 has no "3X" sibling among the 15 sortable
        // columns, so a single digit resolves at once, matching sort.rs),
        // a sort order.
        command(&mut h, "outline status");
        h.press(KeyCode::Char('s'));
        h.press(KeyCode::Char('3'));
        h.press(KeyCode::Char('1'));
        let screen = h.render();
        let title = title_border(&screen);
        assert!(title.contains("shown"), "{width}x{height}: {screen}");
        for marker in ["≈pri", "outline:status", "cols:"] {
            assert!(
                !title.contains(marker),
                "{marker} leaked onto the title line at {width}x{height}: {screen}"
            );
        }
        if width >= 100 {
            // Narrower than this, the settings tail may legitimately be
            // ellipsis-truncated (see the dedicated truncation test below).
            assert!(
                screen.contains("outline:status"),
                "settings summary missing from the footer at {width}x{height}: {screen}"
            );
        }
    }
}

#[test]
fn title_line_shows_the_saved_slot_name_and_relabels_to_custom_once_edited() {
    let mut h = Harness::new();
    let title = title_border(&h.render()).to_string();
    assert!(title.contains(" v1 "), "{title}");
    h.press(KeyCode::Char('/'));
    h.type_text("label:ui");
    let screen = h.press(KeyCode::Enter);
    let title = title_border(&screen);
    assert!(title.contains(" custom "), "{screen}");
}

// --- TASK-235: the filter line ----------------------------------------------

#[test]
fn filter_line_is_absent_with_no_filter_and_present_once_one_is_set() {
    for (width, height) in SIZES {
        let mut h = Harness::new();
        resize(&mut h, width, height);
        let screen = h.render();
        // No filter: the header follows the title border directly, no blank row.
        assert!(
            under_title_border(&screen).is_some_and(|line| line.contains("1 id")),
            "{width}x{height}: {screen}"
        );
        h.press(KeyCode::Char('/'));
        h.type_text("status:todo");
        let screen = h.press(KeyCode::Enter);
        assert!(
            under_title_border(&screen).is_some_and(|line| line.contains("/ status:todo")),
            "{width}x{height}: {screen}"
        );
    }
}

#[test]
fn a_filter_longer_than_the_width_wraps_instead_of_being_cut() {
    let mut h = Harness::new();
    resize(&mut h, 40, 8);
    h.press(KeyCode::Char('/'));
    // Comfortably wider than the 38-column inner frame at 40 columns.
    let long_filter = "status:todo status:inprogress status:done extra padding words";
    let screen = h.type_text(long_filter);
    // However it is bucketed, at least two physical rows carry filter words
    // (not the whole query truncated onto one row), and nothing panicked.
    let filter_line_count = screen
        .lines()
        .filter(|line| {
            long_filter
                .split_whitespace()
                .any(|word| line.contains(word))
        })
        .count();
    assert!(
        filter_line_count >= 2,
        "expected the filter to wrap across multiple rows: {screen}"
    );
}

#[test]
fn a_filter_that_matches_but_wraps_past_the_row_budget_still_leaves_a_task_row_and_a_cut_cue() {
    let mut h = Harness::new();
    resize(&mut h, 40, 8);
    h.press(KeyCode::Char('/'));
    // Repeating the same term is still a match (it just ANDs with itself)
    // but is comfortably longer than the frame's 2-row filter budget at this
    // size (inner height 4, minus 1 for the header, minus 1 to guarantee a
    // task row survives).
    let long_filter = "label:ui ".repeat(20);
    let long_filter = long_filter.trim_end();
    h.type_text(long_filter);
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.lines().any(|line| line.contains("1 id")),
        "the header must survive a long filter: {screen}"
    );
    assert!(
        screen.contains("Add dark theme"),
        "the one match must still get a task row, not be starved off screen: {screen}"
    );
    assert!(
        screen.contains('…'),
        "a filter cut short of fitting needs a visible cue that it was cut: {screen}"
    );
}

#[test]
fn filter_editing_still_opens_with_slash_and_the_footer_shows_the_live_draft() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("dark");
    let footer = screen.lines().last().unwrap_or_default();
    assert!(footer.starts_with('/'), "{screen}");
    assert!(footer.contains("dark"), "{screen}");
    // The in-frame filter line mirrors the same live value.
    assert!(
        under_title_border(&screen).is_some_and(|line| line.contains("/ dark")),
        "{screen}"
    );
    // Esc mid-edit keeps the live value (it only leaves editing); committing
    // with Enter and then a second Esc from Browse is what clears a filter.
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Esc);
    assert!(
        !under_title_border(&screen).is_some_and(|line| line.contains("/ dark")),
        "esc clears the filter line too: {screen}"
    );
}

// --- TASK-235: settings in the footer hint bar ------------------------------

#[test]
fn footer_settings_are_muted_after_the_key_hints_and_truncate_before_them() {
    let mut h = Harness::new();
    command(&mut h, "outline status");
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('1'));
    let screen = h.render();
    let footer = screen.lines().last().unwrap_or_default();
    assert!(footer.contains("keys"), "key hints present: {footer}");
    assert!(footer.contains("outline:status"), "{footer}");

    // Narrow the terminal to just wider than the key hints themselves ("t
    // tasks · v views · ? keys" is 26 columns) so there is only a sliver left
    // for the settings tail; the key hints must still be intact, never the
    // part that gets cut.
    resize(&mut h, 34, 20);
    let screen = h.render();
    let footer = screen.lines().last().unwrap_or_default();
    assert!(
        footer.contains("t tasks · v views · ? keys"),
        "key hints must never lose room to settings: {footer}"
    );
    assert!(
        footer.contains("keys"),
        "key hints must never lose room to settings: {footer}"
    );
}

// --- TASK-234: header cells split the key from the name ---------------------

#[test]
fn header_number_and_name_render_in_different_inks() {
    let h = Harness::new();
    let number_style = cell_style(&h, "1 id", 0);
    let name_style = cell_style(&h, "1 id", 2);
    assert_ne!(
        number_style.fg, name_style.fg,
        "the column number and its name must read as different ink"
    );
    let theme = &h.app.config.theme;
    assert_eq!(
        number_style.fg,
        theme.style(switchbard_tui::config::Surface::Keys).fg
    );
    assert_eq!(
        name_style.fg,
        theme.style(switchbard_tui::config::Surface::Header).fg
    );
}

#[test]
fn sort_underline_and_header_paint_still_cover_the_whole_split_cell() {
    let mut h = Harness::new();
    // Column 3 (priority) has no "3X" sibling among the 15 sortable columns,
    // so a single digit resolves at once (matching sort.rs's own pattern).
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('1'));
    h.render();
    assert!(
        cell_style(&h, "3 pri", 0)
            .add_modifier
            .contains(Modifier::UNDERLINED),
        "the sort underline must reach the number span too"
    );
    assert!(cell_style(&h, "3 pri", 2)
        .add_modifier
        .contains(Modifier::UNDERLINED));

    command(&mut h, "paint header=strong");
    h.render();
    assert!(
        cell_style(&h, "3 pri", 0)
            .add_modifier
            .contains(Modifier::BOLD),
        "header= paint must patch the number span too"
    );
    assert!(cell_style(&h, "3 pri", 2)
        .add_modifier
        .contains(Modifier::BOLD));
}
