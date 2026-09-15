//! Human keyboard journeys through formatting scopes and role composition.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::style::Modifier;
use switchbard_tui::paint::PaintRule;

#[test]
fn paint_exposes_roles_and_structural_scopes_without_losing_row_selection() {
    let mut h = Harness::new();
    let selected = h.app.selected_task().unwrap().id.clone();
    let screen = h.press(KeyCode::Char('p'));
    for label in ["title band", "column headings", "selected row values"] {
        assert!(screen.contains(label), "{label}: {screen}");
    }
    let screen = h.press(KeyCode::Char('h'));
    for role in ["quiet", "strong", "alert", "band", "struck"] {
        assert!(screen.contains(role), "{role}: {screen}");
    }
    h.type_text("strong+p2");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted strong+p2"), "{screen}");
    assert!(h
        .app
        .state
        .paint
        .iter()
        .any(|rule| matches!(rule, PaintRule::Header { color } if color == "strong+p2")));
    assert_eq!(h.app.selected_task().unwrap().id, selected);
    assert!(h
        .terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .any(|cell| cell.symbol() == "i" && cell.modifier.contains(Modifier::BOLD)));
}

#[test]
fn roles_require_enter_for_composition_and_cancel_leaves_rules_unchanged() {
    let mut h = Harness::new();
    h.type_text("prquiet");
    assert!(h.app.state.paint.is_empty());
    h.press(KeyCode::Left);
    assert!(h.render().contains("selected row values"));
    h.press(KeyCode::Char('r'));
    h.type_text("alert+strong");
    h.press(KeyCode::Esc);
    assert!(h.app.state.paint.is_empty());
    h.type_text("prstrong+struck");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted strong+struck"), "{screen}");
}

#[test]
fn a_second_band_is_refused_without_changing_existing_paint() {
    let mut h = Harness::new();
    h.type_text("prB");
    let before = h.app.state.paint.clone();
    assert_eq!(before.len(), 1);
    h.type_text("phB");
    let screen = h.render();
    assert!(screen.contains("band already belongs"), "{screen}");
    assert_eq!(
        h.app.state.paint, before,
        "rejected formatting is transactional"
    );
}

#[test]
fn selected_values_and_actual_group_headings_are_reachable_and_preserve_facts() {
    let mut h = Harness::new();
    let before: Vec<_> = h
        .app
        .tasks()
        .iter()
        .map(|task| (task.id.clone(), task.title.clone(), task.status.clone()))
        .collect();
    h.type_text("pe");
    let screen = h.render();
    assert!(screen.contains("status:"), "{screen}");
    assert!(screen.contains("paints this value wherever"), "{screen}");
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('Q'));
    h.press(KeyCode::Char(':'));
    h.type_text("outline status");
    h.press(KeyCode::Enter);
    h.type_text("pg");
    let screen = h.render();
    assert!(
        screen.contains("To Do") || screen.contains("In Progress"),
        "{screen}"
    );
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('S'));
    assert!(h
        .app
        .state
        .paint
        .iter()
        .any(|rule| matches!(rule, PaintRule::Heading { .. })));
    assert_eq!(
        before,
        h.app
            .tasks()
            .iter()
            .map(|task| (task.id.clone(), task.title.clone(), task.status.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn pr_structural_paint_is_available_without_rows_and_isolated_from_tasks() {
    let mut h = Harness::new();
    let tasks = h.app.state.clone();
    h.next_list_page();
    let screen = h.press(KeyCode::Char('p'));
    assert!(screen.contains("column headings"), "{screen}");
    assert!(
        !screen.contains("selected row values"),
        "no selected PR: {screen}"
    );
    h.type_text("hS");
    assert!(h.render().contains("painted strong"));
    assert!(h
        .app
        .state
        .paint
        .iter()
        .any(|rule| matches!(rule, PaintRule::Header { color } if color == "strong")));
    h.next_list_page();
    assert_eq!(h.app.state, tasks);
}

#[test]
fn invalid_typed_roles_are_reported_without_mutating_rules() {
    let mut h = Harness::new();
    h.type_text("prquiet+unknown");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("nothing matches"), "{screen}");
    assert!(h.app.state.paint.is_empty());
}
