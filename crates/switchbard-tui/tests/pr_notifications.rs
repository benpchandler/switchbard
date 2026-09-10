//! Real keys and bounded GitHub CLI processes, including unavailable repository recovery.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant};

fn settle(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(100);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
    }
    assert!(!h.app.pull_requests.loading(), "refresh exceeded its bound");
}

#[test]
fn failed_refresh_alert_survives_navigation_and_deduplicates_retries() {
    let mut h = Harness::new();
    assert!(h.app.pull_requests.notifications.is_empty());
    h.next_list_page();
    h.next_list_page();
    settle(&mut h);
    let screen = h.render();
    assert!(screen.contains("[Tasks]"), "{screen}");
    assert!(screen.contains("PR Refresh failed"), "{screen}");
    assert!(screen.contains("Refresh failed"), "{screen}");
    h.next_list_page();
    h.press(KeyCode::Char('r'));
    settle(&mut h);
    assert_eq!(h.app.pull_requests.notifications.len(), 1);
    for (width, height) in [(40, 8), (80, 24), (120, 40), (180, 50)] {
        h.terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        assert!(screen.contains("PR Refresh failed"), "{screen}");
        assert!(screen.contains("n dismiss"), "{screen}");
        assert!(screen.contains("Pull Requests"), "{screen}");
    }
    h.next_list_page();
    h.press(KeyCode::Char('n'));
    assert!(h.app.pull_requests.notifications.is_empty());
    assert!(!h.render().contains("n dismiss"));
    h.next_list_page();
    h.press(KeyCode::Char('r'));
    settle(&mut h);
    assert!(
        h.app.pull_requests.notifications.is_empty(),
        "unchanged failure must stay dismissed"
    );
    assert!(h.app.pull_requests.error.is_some());
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; read-only 30-second background interval"]
fn live_baseline_has_no_historical_flood_and_refresh_continues_on_tasks() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let mut h = Harness::new();
    std::fs::write(&h.config_path, "return { pr_refresh_seconds = 30 }").unwrap();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.next_list_page();
    settle(&mut h);
    assert!(
        h.app.pull_requests.error.is_none(),
        "{:?}",
        h.app.pull_requests.error
    );
    let snapshot = h
        .app
        .pull_requests
        .snapshot
        .as_ref()
        .expect("live snapshot");
    assert!(
        snapshot.enrichment_warning.is_none(),
        "{:?}",
        snapshot.enrichment_warning
    );
    let observed = snapshot.observed_at;
    assert!(
        h.app.pull_requests.notifications.is_empty(),
        "first observation is a baseline"
    );
    h.next_list_page();
    let deadline = Instant::now() + Duration::from_secs(130);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        h.app.tick();
        if h.app
            .pull_requests
            .snapshot
            .as_ref()
            .is_some_and(|s| s.observed_at > observed)
        {
            break;
        }
    }
    assert!(h
        .app
        .pull_requests
        .snapshot
        .as_ref()
        .is_some_and(|s| s.observed_at > observed));
    assert!(h.render().contains("[Tasks]"));
}

fn git(root: &std::path::Path, args: &[&str]) {
    let output = switchbard_core::git_cmd()
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; local remote removal and recovery, read-only GitHub"]
fn live_refresh_failure_retains_rows_then_recovers_across_pages_without_duplicate_alerts() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let source = switchbard_core::fetch_pull_requests(std::path::Path::new(&root)).unwrap();
    let mut h = Harness::new();
    git(&h.root, &["init", "-q"]);
    git(
        &h.root,
        &["remote", "add", "origin", &source.repository_url],
    );
    h.next_list_page();
    settle(&mut h);
    assert!(h.app.pull_requests.error.is_none());
    assert!(h.app.pull_requests.notifications.is_empty());
    let baseline = h
        .app
        .pull_requests
        .snapshot
        .as_ref()
        .expect("baseline")
        .clone();
    git(&h.root, &["remote", "remove", "origin"]);
    h.press(KeyCode::Char('r'));
    h.next_list_page();
    settle(&mut h);
    assert!(h.app.pull_requests.error.is_some());
    assert_eq!(
        h.app.pull_requests.snapshot.as_ref().unwrap().rows,
        baseline.rows
    );
    assert_eq!(
        h.app.pull_requests.snapshot.as_ref().unwrap().observed_at,
        baseline.observed_at
    );
    assert_eq!(h.app.pull_requests.notifications.len(), 1);
    let screen = h.render();
    assert!(
        screen.contains("[Tasks]") && screen.contains("PR Refresh failed"),
        "{screen}"
    );
    git(
        &h.root,
        &["remote", "add", "origin", &source.repository_url],
    );
    h.next_list_page();
    h.press(KeyCode::Char('r'));
    h.next_list_page();
    settle(&mut h);
    assert!(h.app.pull_requests.error.is_none());
    assert_eq!(
        h.app.pull_requests.notifications.latest(),
        Some("Refresh recovered")
    );
    let recovered_count = h.app.pull_requests.notifications.len();
    let screen = h.render();
    assert!(
        screen.contains("[Tasks]") && screen.contains("PR Refresh recovered"),
        "{screen}"
    );
    h.next_list_page();
    h.press(KeyCode::Char('r'));
    h.next_list_page();
    settle(&mut h);
    assert_eq!(h.app.pull_requests.notifications.len(), recovered_count);
    h.press(KeyCode::Char('n'));
    assert!(h.render().contains("PR Refresh failed"));
    h.press(KeyCode::Char('n'));
    assert!(h.app.pull_requests.notifications.is_empty());
}

#[test]
#[ignore = "requires SBT_PR_REPO and SBT_WATCH_PR with naturally changing live GitHub status; read-only up to 10 minutes"]
fn live_delivery_transition_is_reported_while_tasks_are_visible() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let number = std::env::var("SBT_WATCH_PR").expect("SBT_WATCH_PR");
    let mut h = Harness::new();
    std::fs::write(&h.config_path, "return { pr_refresh_seconds = 30 }").unwrap();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.next_list_page();
    settle(&mut h);
    assert!(h.app.pull_requests.error.is_none());
    assert!(h
        .app
        .pull_requests
        .snapshot
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .any(|r| r.number.to_string() == number));
    h.next_list_page();
    let prefix = format!("#{number}:");
    let deadline = Instant::now() + Duration::from_secs(600);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        h.app.tick();
        if h.app
            .pull_requests
            .notifications
            .latest()
            .is_some_and(|message| message.starts_with(&prefix))
        {
            let screen = h.render();
            assert!(screen.contains("[Tasks]"), "{screen}");
            assert!(screen.contains(&prefix), "{screen}");
            println!("{screen}");
            return;
        }
    }
    panic!("No live transition for PR {number} observed within ten minutes");
}
