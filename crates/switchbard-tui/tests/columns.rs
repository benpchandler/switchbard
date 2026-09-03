//! `c`: which columns show, their order, glyph mode.

mod harness;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness::*;

#[test]
fn c_toggles_columns_by_position_and_numbers_follow_what_is_shown() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    let screen = h.render();
    assert!(screen.contains("1 ✓id"), "{screen}");
    assert!(
        screen.contains("5  labels · hidden"),
        "hidden columns listed after shown: {screen}"
    );
    let screen = h.press(KeyCode::Char('5'));
    assert!(
        screen.contains("┌ columns ─"),
        "adding keeps the picker open so the column can be placed: {screen}"
    );
    assert_eq!(
        h.app.status,
        "labels added as column 5 · m then numbers to reorder · esc"
    );
    h.press(KeyCode::Char('m'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Char('5'));
    assert!(screen.contains("┌ move columns: 125"), "{screen}");
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Esc);
    let header = header_line(&screen);
    assert!(
        header.contains("1 id")
            && header.contains("2 status")
            && header.contains("3 labels")
            && header.contains("4 pri")
            && header.contains("5 title"),
        "cm125 put labels third and the rest kept their order: {header}"
    );
    assert!(screen.contains("auth,bug"), "{screen}");
    h.press(KeyCode::Char('c'));
    let screen = h.press(KeyCode::Char('4'));
    let header = header_line(&screen);
    assert!(!header.contains("pri"), "{header}");
    assert!(
        header.contains("3 labels") && header.contains("4 title"),
        "renumbered: {header}"
    );
    assert!(screen.contains("cols:id,status,labels,title"), "{screen}");
}

#[test]
fn hidden_columns_are_listed_after_shown_ones_and_stay_filterable_and_sortable() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Char('3'));
    assert!(!header_line(&h.render()).contains("pri"));
    let screen = h.press(KeyCode::Char('f'));
    assert!(
        screen.contains("3 ✓title") && screen.contains("4  priority · hidden"),
        "hidden priority listed after the shown columns: {screen}"
    );
    let screen = h.press(KeyCode::Char('4'));
    assert!(screen.contains("┌ pri ─"), "{screen}");
    let screen = h.type_text("h");
    assert!(
        screen.contains("pri:high · cols:id,status,title · 1/3"),
        "{screen}"
    );
    h.press(KeyCode::Char('s'));
    let screen = h.type_text("p");
    assert!(
        screen.contains("1  priority · hidden") && screen.contains("2  project · hidden"),
        "an ambiguous name narrows to both: {screen}"
    );
    h.type_text("ri");
    let screen = h.type_text("d");
    assert!(screen.contains("↓pri"), "{screen}");
}

#[test]
fn shift_k_moves_a_column_up_and_the_order_saves_with_the_view() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Char('j'));
    h.app
        .handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT));
    let screen = h.press(KeyCode::Esc);
    let header = header_line(&screen);
    assert!(
        header.starts_with("│1 status") || header.contains("1 status    2 id"),
        "{header}"
    );
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(
        file.contains("columns = \"status,id,priority,title\""),
        "{file}"
    );
    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.state.columns[0].name(), "status");
    assert_eq!(fresh.view_label(), "v1");
    assert_eq!(
        fresh.views.get(0).unwrap().name(),
        "cols:status,id,priority,title"
    );
}

#[test]
fn the_last_column_cannot_be_hidden() {
    let mut h = Harness::new();
    for _ in 0..3 {
        h.press(KeyCode::Char('c'));
        h.press(KeyCode::Char('1'));
    }
    h.press(KeyCode::Char('c'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("at least one column must stay"), "{screen}");
    assert_eq!(h.app.state.columns.len(), 1);
}

#[test]
fn g_in_the_columns_picker_shows_priority_as_glyphs_and_saves_with_the_view() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('g'));
    assert_eq!(h.app.status, "priority shows glyphs");
    let screen = h.press(KeyCode::Esc);
    let header = header_line(&screen);
    assert!(
        header.contains("2 status") && header.contains("3 ↑·↓") && !header.contains("3 pri"),
        "glyph header carries the legend in vocabulary order: {header}"
    );
    assert!(screen.contains("↑"), "high: {screen}");
    assert!(screen.contains("↓"), "low: {screen}");
    let row = screen
        .lines()
        .find(|l| l.contains("Fix login"))
        .unwrap_or_default();
    assert!(row.contains(" · "), "medium: {row}");
    assert!(screen.contains("glyphs:priority ·"), "{screen}");
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(file.contains("glyphs = \"priority\""), "{file}");
    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(
        fresh.state.glyph_columns,
        vec![switchbard_tui::columns::Column::Priority]
    );
    assert_eq!(fresh.view_label(), "v1");
}

#[test]
fn glyphs_come_from_lua_and_fall_back_to_the_first_letter() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        "return { glyphs = { priority = { high = \"H\" }, status = { todo = \"T\" } } }",
    )
    .unwrap();
    h.app.tick();
    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('g'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('g'));
    let screen = h.press(KeyCode::Esc);
    let row = |needle: &str| {
        screen
            .lines()
            .find(|l| l.contains(needle))
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        row("Write onboarding").contains(" T ") && row("Write onboarding").contains(" H "),
        "user glyphs for To Do + high: {screen}"
    );
    assert!(
        row("Add dark theme").contains(" ↓ "),
        "default low glyph survives the merge: {screen}"
    );
    assert!(
        row("Fix login").contains(" ◐ ") && row("Fix login").contains(" · "),
        "In Progress keeps its default glyph: {screen}"
    );
    std::fs::write(
        &h.config_path,
        "return { glyphs = { status = { inprogress = \"\" } } }",
    )
    .unwrap();
    h.app.tick();
    let screen = h.render();
    let fix = screen
        .lines()
        .find(|l| l.contains("Fix login"))
        .unwrap_or_default();
    assert!(
        fix.contains(" I "),
        "an empty glyph falls back to the first letter: {fix}"
    );
    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('g'));
    assert_eq!(h.app.status, "title has no glyphs: it is free text");
}

#[test]
fn ids_drop_the_repo_prefix_priority_is_a_letter_and_columns_fit_their_content() {
    let mut h = Harness::new();
    let screen = h.render();
    let row = screen
        .lines()
        .find(|line| line.contains("Add dark theme"))
        .unwrap()
        .to_string();
    assert!(row.starts_with("│2 "), "bare id, no TASK- prefix: {row}");
    assert!(row.contains(" L "), "priority as a letter: {row}");
    assert!(!row.contains("TASK-"), "{row}");
    let header = header_line(&screen);
    let status_start = header.find("2 status").unwrap();
    let pri_start = header.find("3 pri").unwrap();
    assert_eq!(
        pri_start - status_start,
        "In Progress".len() + 1,
        "status column is as wide as its widest value, plus one space: {header}"
    );
    h.press(KeyCode::Enter);
    let detail = h.render();
    assert!(
        detail.contains("TASK-1 ·"),
        "detail keeps the full id: {detail}"
    );
}
