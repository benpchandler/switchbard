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
    for fill in ["none", "band", "h1", "h2", "h3"] {
        assert!(screen.contains(fill), "step 1 offers {fill}: {screen}");
    }
    let screen = h.press(KeyCode::Char('N'));
    for role in ["keep default ink", "quiet", "strong", "alert", "struck"] {
        assert!(screen.contains(role), "step 2 offers {role}: {screen}");
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
    h.type_text("prBK");
    assert_eq!(h.app.state.paint.len(), 1);
    h.type_text("phBK");
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
    h.press(KeyCode::Char('N'));
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
    h.press(KeyCode::Char('N'));
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
    h.type_text("hNS");
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

/// The owner's ask (TASK-245): a highlight and a text style, brought together
/// without typing either of them.
#[test]
fn a_highlight_and_a_text_style_are_chosen_in_two_steps() {
    let mut h = Harness::new();
    let selected = h.app.selected_task().unwrap().id.clone();
    h.type_text("pr");
    let screen = h.render();
    assert!(screen.contains("highlight · step 1 of 2"), "{screen}");
    let screen = h.type_text("h3");
    assert!(
        screen.contains("highlight · h3 ← this is how it looks"),
        "step 1 names itself and previews the fill it would apply: {screen}"
    );
    h.press(KeyCode::Enter);
    let screen = h.render();
    assert!(
        screen.contains("text · h3 ← this is how it looks"),
        "step 2 opens naming itself and previewing the fill already chosen: {screen}"
    );
    let screen = h.type_text("alert");
    assert!(
        screen.contains("h3+alert ← this is how it looks"),
        "the composition is previewed before Enter: {screen}"
    );
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted h3+alert"), "{screen}");
    assert!(
        h.app.state.paint.iter().any(|rule| matches!(
            rule,
            PaintRule::Rows { filter, color } if filter == &format!("id:{selected}") && color == "h3+alert"
        )),
        "the saved rule is the same grammar as before: {:?}",
        h.app.state.paint
    );
    h.press(KeyCode::Char('j'));
    h.render();
    let composed = h
        .app
        .config
        .theme
        .emphasis_style("h3+alert", &h.app.config.palette)
        .expect("the composition resolves");
    assert_eq!(cell_bg(&h, "Fix login"), composed.bg, "fill from step 1");
    assert_eq!(cell_fg(&h, "Fix login"), composed.fg, "ink from step 2");
}

/// The text step is additive: Space gathers tokens and the title shows the
/// composition growing before Enter applies it (TASK-245).
#[test]
fn the_text_step_gathers_several_tokens_before_applying() {
    let mut h = Harness::new();
    h.type_text("prN");
    h.type_text("strong");
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        screen.contains("strong ← this is how it looks"),
        "the first token is held, not applied: {screen}"
    );
    assert!(h.app.state.paint.is_empty(), "nothing is applied yet");
    for _ in 0.."strong".len() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("p2");
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        screen.contains("strong+p2 ← this is how it looks"),
        "both tokens compose in the title: {screen}"
    );
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted strong+p2"), "{screen}");
    assert!(
        h.app
            .state
            .paint
            .iter()
            .any(|rule| rule.role_lists().contains(&"strong+p2")),
        "{:?}",
        h.app.state.paint
    );
}

/// Esc cancels at either step, Left returns to the highlight with the fill
/// still marked, and a rule typed in full still lands at either step (TASK-245).
#[test]
fn esc_cancels_and_left_steps_back_through_the_style_picker() {
    let mut h = Harness::new();
    h.type_text("pr");
    h.press(KeyCode::Esc);
    assert!(
        h.app.state.paint.is_empty(),
        "esc at step 1 applies nothing"
    );
    h.type_text("prh2");
    h.press(KeyCode::Enter);
    assert!(h.render().contains("text · h2 ←"), "{}", h.render());
    h.press(KeyCode::Left);
    let screen = h.render();
    assert!(screen.contains("highlight ·"), "back at step 1: {screen}");
    assert!(
        screen.contains("✓h2"),
        "the fill chosen is still marked: {screen}"
    );
    h.press(KeyCode::Esc);
    assert!(
        h.app.state.paint.is_empty(),
        "esc at step 2 applies nothing"
    );
    h.type_text("prband+red");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("painted band+red"),
        "typed at step 1: {screen}"
    );
    h.type_text("prN");
    h.type_text("quiet+struck");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("painted quiet+struck"),
        "typed at step 2: {screen}"
    );
}

/// Both steps stay usable where the list is narrowest (TASK-245).
#[test]
fn the_two_steps_work_on_a_narrow_terminal() {
    let mut h = Harness::new();
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 12)).unwrap();
    h.render();
    h.type_text("pr");
    let screen = h.render();
    assert!(screen.contains("highlight"), "{screen}");
    h.type_text("h1");
    h.press(KeyCode::Enter);
    let screen = h.render();
    assert!(screen.contains("text"), "{screen}");
    h.type_text("quiet");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted h1+quiet"), "{screen}");
}

/// Every fill-and-ink shape reachable by moving the cursor and pressing keys,
/// with no rule typed (TASK-245): no fill or a fill, and no ink, one ink or
/// several. The rule text is the same grammar a view has always saved.
#[test]
fn each_fill_and_ink_shape_is_reachable_without_typing_a_rule() {
    let mut h = Harness::new();
    // Step two lists keep, quiet, strong, alert, struck, then the colors, so
    // two Downs reach strong and a third reaches alert.
    let gather_strong_and_alert = |h: &mut Harness| {
        for _ in 0..2 {
            h.press(KeyCode::Down);
        }
        h.press(KeyCode::Char(' '));
        h.press(KeyCode::Down);
        h.press(KeyCode::Char(' '));
        h.press(KeyCode::Enter);
    };
    // Each case starts from a bare scope, because reopening one deliberately
    // starts from the rule it already wears.
    let clear = |h: &mut Harness| {
        h.type_text(":paint off");
        h.press(KeyCode::Enter);
    };
    for (fill_key, expected) in [('B', "band"), ('N', "none")] {
        clear(&mut h);
        h.type_text("ph");
        h.press(KeyCode::Char(fill_key));
        h.press(KeyCode::Char('K'));
        assert_eq!(header_rule(&h), expected, "{fill_key} with no ink");
    }
    for (fill_key, expected) in [('B', "band+strong+alert"), ('N', "strong+alert")] {
        clear(&mut h);
        h.type_text("ph");
        h.press(KeyCode::Char(fill_key));
        gather_strong_and_alert(&mut h);
        assert_eq!(header_rule(&h), expected, "{fill_key} with two inks");
    }
    // A slot composes the same way, and on its own is a fill wearing the
    // theme's default ink.
    clear(&mut h);
    h.type_text("ph");
    h.type_text("h2");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('K'));
    assert_eq!(header_rule(&h), "h2");
    clear(&mut h);
    h.type_text("ph");
    h.type_text("h2");
    h.press(KeyCode::Enter);
    gather_strong_and_alert(&mut h);
    assert_eq!(header_rule(&h), "h2+strong+alert");
    // Reopening starts from that rule: the fill stays chosen, and dropping the
    // ink leaves the fill behind rather than clearing the rule.
    h.type_text("ph");
    h.type_text("h2");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('K'));
    assert_eq!(header_rule(&h), "h2");
}

/// The modifiers the first cell of `needle` renders with.
fn modifiers(h: &Harness, needle: &str) -> Modifier {
    let buffer = h.terminal.backend().buffer();
    for cells in buffer.content.chunks(usize::from(buffer.area.width).max(1)) {
        let line: String = cells.iter().map(|cell| cell.symbol()).collect();
        if let Some(index) = line.find(needle) {
            return cells[line[..index].chars().count()].modifier;
        }
    }
    panic!("missing rendered text: {needle}")
}

/// The rule on the column headings, or `none` when there is none.
fn header_rule(h: &Harness) -> String {
    h.app
        .state
        .paint
        .iter()
        .find_map(|rule| match rule {
            PaintRule::Header { color } => Some(color.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "none".to_string())
}

/// A rule's stop marker belongs to the rule, not to the roles it wears
/// (TASK-245). Restyling a stopped rule through the picker must not quietly
/// unstop it and change how the rules above it render.
#[test]
fn restyling_a_stopped_rule_keeps_its_stop() {
    let mut h = Harness::new();
    h.type_text(":paint rows:=strong;column:title=green!");
    h.press(KeyCode::Enter);
    assert!(
        !modifiers(&h, "Add dark theme").contains(Modifier::BOLD),
        "the stop holds the base rule off the title to begin with"
    );
    h.type_text("pct");
    h.press(KeyCode::Char('N'));
    // keep, quiet, strong, alert, struck, then the colors: five Downs reach red.
    for _ in 0..5 {
        h.press(KeyCode::Down);
    }
    let screen = h.render();
    assert!(
        screen.contains("red! ← this is how it looks"),
        "the marker is part of what the title previews: {screen}"
    );
    h.press(KeyCode::Enter);
    assert!(
        h.app.state.paint.iter().any(|rule| matches!(
            rule,
            PaintRule::Column { color, .. } if color == "red!"
        )),
        "the stop survives the restyle: {:?}",
        h.app.state.paint
    );
    assert!(
        !modifiers(&h, "Add dark theme").contains(Modifier::BOLD),
        "and still holds the base rule off"
    );
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(ratatui::style::Color::Red),
        "picking an ink replaces the one the rule wore rather than joining it"
    );
}

/// The draft is bounded, and reaching the bound is said out loud rather than
/// swallowing the token (TASK-245).
#[test]
fn the_ink_limit_is_reported_rather_than_dropping_a_token() {
    let mut h = Harness::new();
    h.type_text("prN");
    // Fifteen tokens fit beside the fill; gather them from the rows below the
    // cursor, one Down and one Space each.
    for _ in 0..15 {
        h.press(KeyCode::Down);
        h.press(KeyCode::Char(' '));
    }
    assert!(
        !h.app.status.contains("limit"),
        "fifteen fit: {}",
        h.app.status
    );
    h.press(KeyCode::Down);
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        screen.contains("ink limit reached: 15 text tokens"),
        "the sixteenth is refused out loud: {screen}"
    );
    h.press(KeyCode::Esc);
    assert!(h.app.state.paint.is_empty());
}
