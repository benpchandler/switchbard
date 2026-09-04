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
    assert_eq!(h.app.status, "grouped by goal · o flattens");
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
