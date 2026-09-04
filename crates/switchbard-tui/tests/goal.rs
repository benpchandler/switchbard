//! The goal column: which weekly goal a task feeds (TASK-142, "organize by
//! goal"). Membership is derived from `goals.yml` scope and attachments, so it
//! filters, groups, and shows like any other column.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

/// Two goals: one scoped to the Chase project, one with an attached task.
/// `Write onboarding guide` (TASK-3) is attached; the Ally task feeds nothing.
fn goal_harness() -> Harness {
    let mut h = Harness::new();
    seed_project(&h.root, "Chase", "In Progress", Some("Lenders"));
    seed_in_project(&h.root, "Chase rate sheet", "Done", "Chase", None);
    seed_in_project(&h.root, "Chase portal login", "To Do", "Chase", None);
    seed_in_project(&h.root, "Ally intake form", "To Do", "Ally", None);
    seed_goal(&h.root, "lenders-live", "journeys", 3, Some("Chase"), &[]);
    seed_goal(&h.root, "docs-shipped", "guides", 1, None, &["TASK-3"]);
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h
}

#[test]
fn group_goal_sections_tasks_by_the_goal_they_feed_with_this_weeks_progress() {
    let mut h = goal_harness();
    h.press(KeyCode::Char(':'));
    h.type_text("group goal");
    let screen = h.press(KeyCode::Enter);
    let rows = screen_rows(&h);
    // The seeded Done task carries no updated date, so core does not count it
    // as done this week; the heading reads core's actual, not a row count.
    assert!(
        rows[0].starts_with("# lenders-live · 0/3 journeys · "),
        "the scoped goal heads the list with core's actual over target: {rows:?}"
    );
    assert_eq!(
        rows[1..3],
        ["Chase portal login", "Chase rate sheet"],
        "{rows:?}"
    );
    assert!(
        rows[3].starts_with("# docs-shipped · 0/1 guides · "),
        "{rows:?}"
    );
    assert_eq!(rows[4], "Write onboarding guide", "{rows:?}");
    assert_eq!(rows[5], "# no goal", "{rows:?}");
    assert!(screen.contains("group:goal"), "{screen}");
    assert_eq!(h.app.status, "organized by goal · o changes it");
}

#[test]
fn goal_filter_keeps_only_tasks_feeding_that_goal() {
    let mut h = goal_harness();
    h.press(KeyCode::Char('/'));
    h.type_text("goal:lenders");
    h.press(KeyCode::Enter);
    assert_eq!(
        visible_titles(&h),
        ["Chase portal login", "Chase rate sheet"]
    );
    h.press(KeyCode::Char('/'));
    for _ in 0.."goal:lenders".len() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("goal:!lenders-live");
    h.press(KeyCode::Enter);
    assert!(
        !visible_titles(&h)
            .iter()
            .any(|title| title.starts_with("Chase")),
        "{:?}",
        visible_titles(&h)
    );
}

#[test]
fn goal_column_shows_the_goal_name_and_is_empty_for_tasks_outside_every_goal() {
    let mut h = goal_harness();
    h.press(KeyCode::Char(':'));
    h.type_text("group off");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('c'));
    let screen = h.render();
    assert!(
        screen.contains("goal"),
        "the column picker lists goal: {screen}"
    );
    h.press(KeyCode::Esc);
    h.app
        .state
        .columns
        .push(switchbard_tui::columns::Column::Goal);
    let screen = h.render();
    let header = header_line(&screen);
    assert!(header.contains("goal"), "{header}");
    let chase_row = screen
        .lines()
        .find(|line| line.contains("Chase portal login"))
        .unwrap();
    assert!(chase_row.contains("lenders-live"), "{chase_row}");
    let ally_row = screen
        .lines()
        .find(|line| line.contains("Ally intake form"))
        .unwrap();
    assert!(
        !ally_row.contains("lenders-live") && !ally_row.contains("docs-shipped"),
        "{ally_row}"
    );
}

// ─── linking the selected task to a goal ───────────────────────────────

#[test]
fn tg_opens_the_goal_panel_and_a_pick_attaches_then_detaches_the_selected_task() {
    let mut h = goal_harness();
    // The default sort puts "Fix login redirect loop" (TASK-1, no goal) first.
    assert_eq!(h.selected_title(), "Fix login redirect loop");
    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('g'));
    assert!(screen.contains("TASK-1 · goals"), "{screen}");
    assert!(
        screen.contains(" lenders-live") && screen.contains(" docs-shipped"),
        "{screen}"
    );

    h.press(KeyCode::Char('1'));
    let screen = h.render();
    assert!(
        screen.contains("✓lenders-live"),
        "picking marks it attached: {screen}"
    );
    // The footer shows the panel's hint while it is open; the status line
    // behind it carries the outcome.
    assert_eq!(h.app.status, "TASK-1 attached to lenders-live");
    assert!(
        std::fs::read_to_string(h.root.join("backlog/goals.yml"))
            .unwrap()
            .contains("'TASK-1'"),
        "the attachment is written through core"
    );

    h.press(KeyCode::Char('1'));
    let screen = h.render();
    assert!(screen.contains(" lenders-live"), "{screen}");
    assert!(
        !screen.contains("✓lenders-live"),
        "a second pick detaches: {screen}"
    );
    assert_eq!(h.app.status, "TASK-1 detached from lenders-live");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char(':'));
    h.type_text("group goal");
    h.press(KeyCode::Enter);
    let rows = screen_rows(&h);
    assert!(rows.contains(&"# no goal".to_string()), "{rows:?}");
}

#[test]
fn goal_panel_marks_tasks_already_in_scope_without_an_attachment() {
    let mut h = goal_harness();
    h.press(KeyCode::Char('/'));
    h.type_text("Chase portal");
    h.press(KeyCode::Enter);
    assert_eq!(h.selected_title(), "Chase portal login");
    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('g'));
    assert!(
        screen.contains("·lenders-live"),
        "scope membership shows as implied, not attached: {screen}"
    );
}

#[test]
fn goal_command_attaches_by_name_and_names_the_goals_on_a_miss() {
    let mut h = goal_harness();
    h.press(KeyCode::Char(':'));
    h.type_text("goal docs-shipped");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("TASK-1 attached to docs-shipped"),
        "{screen}"
    );
    h.press(KeyCode::Char(':'));
    h.type_text("goal nope");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.status, "goal: one of lenders-live, docs-shipped");
}

#[test]
fn help_lists_the_task_chord_key() {
    let mut h = goal_harness();
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("t       task"), "{screen}");
}
