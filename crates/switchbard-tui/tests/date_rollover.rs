//! Calendar invalidation uses real task files, keyboard filters and the normal tick.
mod harness;
use crossterm::event::KeyCode;
use harness::*;

#[test]
fn utc_day_change_rebuilds_a_cached_date_filter_without_disk_edits() {
    let mut h = Harness::new();
    h.type_text("/filed:today");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.visible.len(), 3);
    // Model the previous day's empty projection: these newly filed tasks were future.
    h.app.calendar_day -= 1;
    h.app.visible.clear();
    h.app.rows.clear();
    h.app.tick();
    let screen = h.render();
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("3/3"), "{screen}");
    assert_eq!(h.app.state.filter, "filed:today");
}

#[test]
fn utc_day_change_rebuilds_tasks_while_pull_requests_is_active() {
    let mut h = Harness::new();
    h.type_text("/filed:today");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Tab);
    h.app.calendar_day -= 1;
    h.app.visible.clear();
    h.app.rows.clear();
    h.app.tick();
    assert_eq!(
        h.app.visible.len(),
        3,
        "inactive Tasks projection is refreshed"
    );
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("filed:today"), "{screen}");
}
