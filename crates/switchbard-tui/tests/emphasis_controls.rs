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

/// A fill belongs to the cells a rule claims, not to the view, so a second one
/// is an ordinary rule (TASK-237). The refusal this replaces named which rule
/// owned the single band.
#[test]
fn a_second_fill_is_accepted_and_both_rules_keep_their_own_scope() {
    let mut h = Harness::new();
    h.type_text("prB");
    assert_eq!(h.app.state.paint.len(), 1);
    h.type_text("phB");
    h.press(KeyCode::Char('j'));
    let screen = h.render();
    assert!(!screen.contains("band already belongs"), "{screen}");
    assert_eq!(h.app.state.paint.len(), 2, "both fills are kept: {screen}");
    let fill = h
        .app
        .config
        .theme
        .emphasis_style("band", &h.app.config.palette)
        .and_then(|style| style.bg);
    assert_eq!(
        cell_bg(&h, "Fix login"),
        fill,
        "the row rule keeps its fill"
    );
    assert_eq!(
        cell_bg(&h, "4 title"),
        fill,
        "the header rule keeps its own"
    );
}

/// The owner's ask: pick a highlight, or a highlight and an ink, without
/// leaving the picker (TASK-237).
#[test]
fn the_style_picker_shows_highlight_swatches_and_previews_a_typed_combination() {
    let mut h = Harness::new();
    h.type_text("pr");
    let screen = h.render();
    for slot in ["h1", "h2", "h3"] {
        assert!(screen.contains(slot), "{slot} is offered: {screen}");
    }
    let slot = h
        .app
        .config
        .theme
        .highlight_style(2, &h.app.config.palette)
        .expect("berg declares h2");
    assert_eq!(cell_bg(&h, "h2"), slot.bg, "the row wears its own fill");
    assert_eq!(cell_fg(&h, "h2"), slot.fg, "and its own default ink");
    h.type_text("h2+alert");
    let screen = h.render();
    assert!(
        screen.contains("h2+alert"),
        "the title previews it: {screen}"
    );
    let composed = h
        .app
        .config
        .theme
        .emphasis_style("h2+alert", &h.app.config.palette)
        .expect("the combination resolves");
    assert_eq!(composed.bg, slot.bg, "fill from the slot");
    assert_eq!(
        composed.fg,
        h.app
            .config
            .theme
            .emphasis_style("alert", &h.app.config.palette)
            .and_then(|style| style.fg),
        "ink from the role"
    );
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted h2+alert"), "{screen}");
    h.press(KeyCode::Char('j'));
    h.render();
    assert_eq!(cell_bg(&h, "Fix login"), composed.bg);
    assert_eq!(cell_fg(&h, "Fix login"), composed.fg);
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
