//! Real command, saved-view and rendered-buffer journeys for semantic emphasis.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::style::{Color, Modifier};

fn paint(h: &mut Harness, rules: &str) -> String {
    h.press(KeyCode::Char(':'));
    h.type_text(&format!("paint {rules}"));
    h.press(KeyCode::Enter)
}

fn modifiers(h: &Harness, needle: &str) -> Modifier {
    let buffer = h.terminal.backend().buffer();
    for cells in buffer.content.chunks(buffer.area.width as usize) {
        let line: String = cells.iter().map(|cell| cell.symbol()).collect();
        if let Some(index) = line.find(needle) {
            return cells[line[..index].chars().count()].modifier;
        }
    }
    panic!("missing rendered text: {needle}")
}

#[test]
fn roles_merge_and_lower_foreground_wins_without_losing_strike() {
    let mut h = Harness::new();
    paint(
        &mut h,
        "rows:status:todo=struck+red;column:title=green+strong",
    );
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));
    assert!(modifiers(&h, "Add dark theme").contains(Modifier::CROSSED_OUT));
    assert!(modifiers(&h, "Add dark theme").contains(Modifier::BOLD));
    assert!(!modifiers(&h, "Fix login").contains(Modifier::CROSSED_OUT));
}

#[test]
fn stop_is_cell_scoped_and_by_column_stop_applies_to_every_mapped_value() {
    let mut h = Harness::new();
    paint(&mut h, "rows:=strong;by:status=todo:red,inprogress:blue!");
    assert!(!modifiers(&h, "To Do").contains(Modifier::BOLD));
    assert_eq!(cell_fg(&h, "To Do"), Some(Color::Red));
    assert!(!modifiers(&h, "In Progress").contains(Modifier::BOLD));
    assert_eq!(cell_fg(&h, "In Progress"), Some(Color::Blue));
    paint(
        &mut h,
        "rows:status:todo=red;column:id=blue!;column:title=green",
    );
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));
}

#[test]
fn malformed_rule_and_second_band_preserve_previous_arrangement() {
    let mut h = Harness::new();
    paint(&mut h, "rows:status:todo=red");
    let before = h.app.state.paint.clone();
    let screen = paint(&mut h, "by:status=todo:green,inprogress:unknown-role");
    assert!(screen.contains("unknown"), "{screen}");
    assert_eq!(h.app.state.paint, before);
    let screen = paint(&mut h, "rows:status:todo=band;column:title=band");
    assert!(screen.contains("band already belongs"), "{screen}");
    assert_eq!(h.app.state.paint, before);
}

#[test]
fn semantic_roles_and_stop_survive_save_and_restart() {
    let mut h = Harness::new();
    paint(&mut h, "column:title=red;rows:status:todo=struck+green!");
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    let before = h.app.state.paint.clone();
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert_eq!(h.app.state.paint, before);
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));
    assert!(modifiers(&h, "Add dark theme").contains(Modifier::CROSSED_OUT));
}

#[test]
fn specific_quiet_stop_blocks_base_bold() {
    let mut h = Harness::new();
    paint(&mut h, "rows:=strong;rows:status:todo=quiet!");
    assert!(!modifiers(&h, "Add dark theme").contains(Modifier::BOLD));
    assert!(modifiers(&h, "Fix login").contains(Modifier::BOLD));
}

#[test]
fn invalid_saved_roles_and_duplicate_band_remain_intact_on_save() {
    for rules in [
        "rows:=strong;column:title=quiet+unknown",
        "rows:=band;header=band",
        "by:status=todo:quiet!,inprogress:strong",
    ] {
        let mut h = Harness::new();
        let source = format!("return {{ [1] = {{ paint='{rules}' }} }}");
        let path = h.root.join("views-repo.lua");
        std::fs::write(&path, &source).unwrap();
        h.app = open_app(&h.root, &h.config_path);
        let screen = h.type_text("vsd");
        assert!(screen.contains("repair file and reopen"), "{screen}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn removed_field_paint_is_pruned_without_losing_valid_semantic_rules() {
    let mut h = Harness::new();
    std::fs::write(
        h.root.join("views-repo.lua"),
        "return { [1] = { paint='column:field:removed=band;rows:status:todo=green+struck' } }",
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert!(
        h.app.status.contains("no longer declares removed"),
        "{}",
        h.app.status
    );
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));
    assert!(modifiers(&h, "Add dark theme").contains(Modifier::CROSSED_OUT));
    let screen = h.type_text("vsd");
    assert!(screen.contains("saved v1"), "{screen}");
}

#[test]
fn editing_a_category_color_keeps_the_rule_stop_at_its_end() {
    let mut h = Harness::new();
    paint(&mut h, "rows:=strong;by:status=todo:red,inprogress:blue!");
    h.press(KeyCode::Char('p'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('2'));
    h.type_text("gre");
    h.press(KeyCode::Esc);
    assert_eq!(cell_fg(&h, "To Do"), Some(Color::Green));
    assert!(!modifiers(&h, "To Do").contains(Modifier::BOLD));
    h.type_text("vsd");
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert_eq!(cell_fg(&h, "To Do"), Some(Color::Green));
    assert!(!modifiers(&h, "To Do").contains(Modifier::BOLD));
}

#[test]
fn heading_field_like_names_survive_while_removed_column_rules_are_pruned() {
    let mut h = Harness::new();
    seed_project(&h.root, "field:obsolete", "Planned", None);
    seed_in_project(
        &h.root,
        "Literal heading task",
        "To Do",
        "field:obsolete",
        None,
    );
    let source = "return { [1] = { group='project', paint='heading:field:obsolete=green+strong;column:field:removed=band;rows:removed:x=alert;rows:project:field:obsolete=struck' } }";
    std::fs::write(h.root.join("views-repo.lua"), source).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert_eq!(cell_fg(&h, "field:obsolete"), Some(Color::Green));
    assert!(modifiers(&h, "Literal heading task").contains(Modifier::CROSSED_OUT));
    assert!(h.app.status.contains("removed"), "{}", h.app.status);
    assert!(!h.app.status.contains("obsolete"), "{}", h.app.status);
    let screen = h.type_text("vsd");
    assert!(screen.contains("saved v1"), "{screen}");
    let saved = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(!saved.contains("rows:removed:"), "{saved}");
    assert!(!saved.contains("column:field:removed"), "{saved}");
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert_eq!(cell_fg(&h, "field:obsolete"), Some(Color::Green));
    assert!(modifiers(&h, "Literal heading task").contains(Modifier::CROSSED_OUT));
}

#[test]
fn legacy_empty_category_rule_remains_writable_and_keeps_later_roles() {
    let mut h = Harness::new();
    std::fs::write(
        h.root.join("views-repo.lua"),
        "return { [1] = { paint=';by:status=;;rows:status:todo=green+struck;' } }",
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));
    assert!(modifiers(&h, "Add dark theme").contains(Modifier::CROSSED_OUT));
    let screen = h.type_text("vsd");
    assert!(screen.contains("saved v1"), "{screen}");
    let saved = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(saved.contains("by:status=;"), "{saved}");
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Green));
    assert!(modifiers(&h, "Add dark theme").contains(Modifier::CROSSED_OUT));
}
