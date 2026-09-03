//! `p`: the paint hierarchy.

mod harness;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness::*;
use ratatui::style::Color;

#[test]
fn p_lists_columns_first_then_row_filtered_column_and_hidden_fields() {
    let mut h = Harness::new();
    let selected = h.app.selected_task().unwrap().id.clone();
    let screen = h.press(KeyCode::Char('p'));
    for entry in ["1 ✓id", "2 ✓status", "3 ✓priority", "4 ✓title"] {
        assert!(
            screen.contains(entry),
            "mirrors the header: {entry} in {screen}"
        );
    }
    assert!(screen.contains(&format!("r  row {selected}")), "{screen}");
    assert!(screen.contains("c  column (whole)"), "{screen}");
    assert!(
        screen.contains("5  labels · hidden"),
        "hidden categorical field by name: {screen}"
    );
    let title = screen.lines().nth(2).unwrap_or_default().to_string();
    assert!(
        title.contains("┌ paint ─") && !title.contains("r row"),
        "{title}"
    );
    let footer = screen.lines().last().unwrap_or_default();
    assert!(footer.contains("number or letter picks · esc"), "{footer}");
}

#[test]
fn p11_paints_rows_by_status_and_h21_layers_priority_on_its_own_cells() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    h.press(KeyCode::Char('p'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("┌ by status ─"), "{screen}");
    h.press(KeyCode::Char('1'));
    let screen = h.press(KeyCode::Esc);
    assert!(
        screen.contains("paint:1 ·"),
        "one rule holds every status color: {screen}"
    );
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(Color::Rgb(0xff, 0xcc, 0x00)),
        "To Do rows"
    );
    assert_eq!(
        cell_fg(&h, "Fix login"),
        Some(Color::Rgb(0xf0, 0x88, 0x3e)),
        "In Progress rows"
    );
    assert_eq!(
        cell_fg(&h, "low"),
        Some(Color::Rgb(0xff, 0xcc, 0x00)),
        "base paints whole rows"
    );

    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('1'));
    let screen = h.press(KeyCode::Char('h'));
    assert!(
        screen.contains("┌ paint ─"),
        "h goes back to the target list: {screen}"
    );
    let screen = h.press(KeyCode::Esc);
    assert!(screen.contains("paint:2 ·"), "{screen}");
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(Color::Rgb(0xff, 0xcc, 0x00)),
        "rows still by status"
    );
    assert_ne!(
        cell_fg(&h, "low"),
        Some(Color::Rgb(0xff, 0xcc, 0x00)),
        "priority cell has its own color"
    );
    assert_eq!(
        cell_fg(&h, "high"),
        Some(Color::Rgb(0xff, 0xcc, 0x00)),
        "auto's first color for high"
    );
    assert_eq!(
        h.app
            .state
            .paint
            .iter()
            .map(|r| r.to_text())
            .collect::<Vec<_>>(),
        [
            "by:status=todo:#ffcc00,inprogress:#f0883e",
            "by:priority=high:#ffcc00,low:#f0883e,medium:#2ea043"
        ]
    );
}

#[test]
fn reordering_rules_flips_which_paint_is_the_base() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('h'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('p'));
    let screen = h.type_text("o");
    assert!(
        screen.contains("┌ paint rules · top is the base"),
        "{screen}"
    );
    assert!(screen.contains("1 ✓by status (2 values)"), "{screen}");
    assert!(screen.contains("2  by priority (3 values)"), "{screen}");
    h.press(KeyCode::Char('j'));
    h.app
        .handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT));
    let screen = h.render();
    assert!(screen.contains("1 ✓by priority (3 values)"), "{screen}");
    h.press(KeyCode::Esc);
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(Color::Rgb(0xf0, 0x88, 0x3e)),
        "rows now by priority (low)"
    );
    assert_eq!(
        cell_fg(&h, "To Do"),
        Some(Color::Rgb(0xff, 0xcc, 0x00)),
        "status cell keeps its own color"
    );
    h.press(KeyCode::Char('p'));
    h.type_text("o");
    let screen = h.press(KeyCode::Delete);
    assert!(
        screen.contains("1 ✓by status (2 values)"),
        "deleting the base promotes the next: {screen}"
    );
    h.press(KeyCode::Esc);
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(Color::Rgb(0xff, 0xcc, 0x00))
    );
}

#[test]
fn hand_picked_values_row_and_column_and_hex_and_clearing() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    let selected = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('2'));
    let screen = h.type_text("gre");
    assert!(
        screen.contains("┌ by status ─"),
        "back on the value list: {screen}"
    );
    assert_eq!(
        cell_fg(&h, "To Do"),
        Some(Color::Green),
        "the list previews the value's color"
    );
    h.press(KeyCode::Esc);
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));
    assert_eq!(
        cell_fg(&h, "Fix login"),
        Some(Color::Reset),
        "In Progress unpainted"
    );

    h.press(KeyCode::Char('p'));
    h.type_text("r");
    let screen = h.press(KeyCode::Char('1'));
    assert!(
        screen.contains("1▏ color"),
        "first digit waits when 10+ exist: {screen}"
    );
    h.press(KeyCode::Char('2'));
    assert_eq!(
        cell_fg(&h, &selected),
        Some(Color::LightBlue),
        "12 picks lightblue"
    );
    assert!(h
        .app
        .state
        .paint
        .iter()
        .any(|r| r.to_text() == format!("rows:id:{selected}=lightblue")));

    h.press(KeyCode::Char('p'));
    h.type_text("c");
    h.type_text("t");
    h.type_text("#ff8800");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted #ff8800"), "{screen}");
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(Color::Rgb(255, 136, 0)),
        "column rule below the base wins on its column"
    );

    h.press(KeyCode::Char('p'));
    h.type_text("c");
    h.type_text("t");
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("paint cleared"), "{screen}");
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));

    h.press(KeyCode::Char('p'));
    h.type_text("r");
    assert_eq!(
        cell_fg(&h, "green"),
        Some(Color::Green),
        "color list previews colors"
    );
    let screen = h.type_text("#00ff00");
    assert!(
        screen.contains("#00ff00 ← this is how it looks"),
        "{screen}"
    );
    h.press(KeyCode::Esc);

    h.press(KeyCode::Char('p'));
    let screen = h.press(KeyCode::Char('d'));
    assert!(screen.contains("deleted 2 paint rules"), "{screen}");
    assert!(!screen.contains("paint:"), "{screen}");
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('p'));
    let screen = h.press(KeyCode::Delete);
    assert!(screen.contains("deleted 1 paint rules"), "{screen}");
}

#[test]
fn paint_rules_round_trip_through_the_view_file() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('h'));
    h.type_text("r");
    h.type_text("gre");
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(
        file.contains("paint = \"by:status=todo:#ffcc00,inprogress:#f0883e;rows:id:"),
        "{file}"
    );
    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.state.paint, h.app.state.paint);
    assert_eq!(fresh.view_label(), "v1");
}

#[test]
fn a_user_palette_replaces_what_auto_hands_out() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        "return { palette = { \"magenta\", \"#123456\" } }",
    )
    .unwrap();
    h.app.tick();
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Esc);
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Magenta));
    assert_eq!(cell_fg(&h, "Fix login"), Some(Color::Rgb(0x12, 0x34, 0x56)));
}

#[test]
fn palette_presets_swap_live_and_recolor_auto_painted_values() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('5'));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char(':'));
    h.type_text("palette vivid");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.status,
        "palette vivid · keep it: palette = \"vivid\" in tui.lua"
    );
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(Color::Rgb(0xff, 0xcc, 0x00)),
        "preset color swapped for its vivid counterpart"
    );
    assert!(
        h.app
            .state
            .paint
            .iter()
            .any(|rule| rule.to_text().contains("magenta")),
        "hand-picked colors survive: {:?}",
        h.app.state.paint
    );
    h.press(KeyCode::Char(':'));
    h.type_text("palette neon");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.status,
        "palette: one of balanced, bloomberg, muted, vivid"
    );
}
