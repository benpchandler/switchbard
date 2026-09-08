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
    h.press(KeyCode::Tab);
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("filed:today"), "{screen}");
}

#[test]
fn utc_day_change_rebuilds_tasks_while_inbox_is_active() {
    let mut h = Harness::new();
    h.type_text("/filed:today");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Tab);
    h.press(KeyCode::Tab);
    h.app.calendar_day -= 1;
    h.app.visible.clear();
    h.app.rows.clear();
    h.app.tick();
    assert_eq!(h.app.visible.len(), 3);
    assert!(h.render().contains("Your review requests and follow-ups"));
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("filed:today"), "{screen}");
}

fn recent_merge() -> switchbard_core::PrSnapshot {
    use switchbard_core::{PrChecks, PrLifecycle, PrListRow, PrMerge, PrReview, PrSnapshot};
    PrSnapshot {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        observed_at: std::time::SystemTime::now(),
        rows: vec![PrListRow {
            id: "owner/repo#1".into(),
            number: 1,
            title: "Merged today".into(),
            url: "https://github.com/owner/repo/pull/1".into(),
            head_oid: "abc".into(),
            draft: false,
            lifecycle: PrLifecycle::Merged,
            merged_at: Some(chrono::Utc::now()),
            checks: PrChecks::Unknown,
            review: PrReview::Unknown,
            merge: PrMerge::Unknown,
        }],
        truncated: false,
        limit: 100,
        open_count: Ok(0),
        enrichment_warning: None,
    }
}

#[test]
fn utc_day_change_rebuilds_cached_pr_dates_while_inbox_is_active() {
    let mut h = Harness::new();
    h.press(KeyCode::Tab);
    h.type_text("/merged:today");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Tab);
    h.app.pull_requests = Default::default();
    h.app.pull_requests.snapshot = Some(recent_merge());
    h.app.pull_requests.filter = "merged:today".into();
    h.app.calendar_day -= 1;
    h.app.tick();
    assert_eq!(
        h.app.pull_requests.visible.len(),
        1,
        "cached PR projection refreshed from Inbox"
    );
    assert_eq!(h.app.page, switchbard_tui::page::Page::Inbox);
}
