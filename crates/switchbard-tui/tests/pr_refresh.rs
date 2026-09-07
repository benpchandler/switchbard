//! Real refresh workers and terminal rendering, with explicit scheduler time.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant};

fn complete_at(h: &mut Harness, now: Instant) {
    let deadline = Instant::now() + Duration::from_secs(100);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        h.app.pull_requests.tick(&h.root, false, 60, now);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!h.app.pull_requests.loading(), "refresh must finish");
}

fn screens(h: &mut Harness, expected: &str, state: &str) {
    let mut evidence = String::new();
    for (width, height) in [(40, 8), (60, 16), (100, 24), (180, 40)] {
        h.terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        assert!(
            screen
                .lines()
                .nth(2 + usize::from(!h.app.pull_requests.notifications.is_empty()))
                .unwrap()
                .contains(expected),
            "{screen}"
        );
        assert_eq!(
            screen.matches("refreshing").count(),
            usize::from(expected == "refreshing"),
            "{screen}"
        );
        evidence.push_str(&format!("{state} {width}x{height}\n{screen}\n"));
    }
    std::fs::write(
        std::env::temp_dir().join(format!("sbt-pr-countdown-{state}.txt")),
        evidence,
    )
    .unwrap();
}

#[test]
fn countdown_failure_retry_and_manual_refresh_render_in_one_slot() {
    let mut h = Harness::new();
    h.press(KeyCode::Tab);
    screens(&mut h, "refreshing", "loading");
    let completed = Instant::now();
    complete_at(&mut h, completed);
    assert!(h.app.pull_requests.error.is_some());
    screens(&mut h, "60s", "failure-reset");
    assert!(h.render().contains("Unavailable"));

    for (seconds, expected) in [(1, "59s"), (59, "1s")] {
        h.app
            .pull_requests
            .tick(&h.root, false, 60, completed + Duration::from_secs(seconds));
        assert!(
            !h.app.pull_requests.loading(),
            "background refresh waits for its deadline"
        );
        screens(&mut h, expected, &format!("countdown-{seconds}"));
    }
    h.app
        .pull_requests
        .tick(&h.root, true, 60, completed + Duration::from_millis(59_500));
    assert!(!h.app.pull_requests.loading());
    screens(&mut h, "0s", "visible-zero");
    h.app
        .pull_requests
        .tick(&h.root, false, 60, completed + Duration::from_secs(60));
    assert!(
        h.app.pull_requests.loading(),
        "failed refresh retries on the hidden page at deadline"
    );
    screens(&mut h, "refreshing", "automatic-retry");
    assert!(
        h.render().contains("Unavailable"),
        "in-flight retry retains failure"
    );
    let delayed_completion = completed + Duration::from_secs(160);
    complete_at(&mut h, delayed_completion);
    screens(&mut h, "60s", "delayed-completion");
    h.app.pull_requests.tick(
        &h.root,
        true,
        60,
        delayed_completion + Duration::from_secs(59),
    );
    screens(&mut h, "1s", "after-delay");
    assert!(
        !h.app.pull_requests.loading(),
        "cadence starts at completion"
    );

    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Char('r'));
    screens(&mut h, "refreshing", "manual-retry");
    h.press(KeyCode::Tab);
    assert!(h.render().contains("[Tasks]"));
    assert!(!h.render().contains("refreshing"));
    complete_at(&mut h, delayed_completion + Duration::from_secs(60));
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; reads only"]
fn live_success_countdown_refresh_and_cached_failure_share_the_header_slot() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let mut h = Harness::new();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.press(KeyCode::Tab);
    let completed = Instant::now();
    complete_at(&mut h, completed);
    assert!(
        h.app.pull_requests.error.is_none(),
        "{:?}",
        h.app.pull_requests.error
    );
    h.app.pull_requests.refilter();
    screens(&mut h, "60s", "live-success");
    let row_count = h.app.pull_requests.snapshot.as_ref().unwrap().rows.len();
    for (seconds, expected) in [(1, "59s"), (59, "1s")] {
        h.app
            .pull_requests
            .tick(&h.root, false, 60, completed + Duration::from_secs(seconds));
        screens(&mut h, expected, &format!("live-countdown-{seconds}"));
    }
    // A real invalid repository fails without replacing the successful cache.
    h.app.pull_requests.refresh(&h.root);
    screens(&mut h, "refreshing", "live-cached-refresh");
    complete_at(&mut h, completed + Duration::from_secs(120));
    assert!(h.app.pull_requests.error.is_some());
    screens(&mut h, "60s", "live-cached-failure");
    assert!(h.render().contains("STALE"));
    assert_eq!(
        h.app.pull_requests.snapshot.as_ref().unwrap().rows.len(),
        row_count
    );
}
