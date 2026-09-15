//! Real GitHub observations and real keyboard journeys. Never changes a remote PR.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant};

fn settle(h: &mut Harness) {
    h.app.tick();
    let deadline = Instant::now() + Duration::from_secs(110);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
    }
    assert!(!h.app.pull_requests.loading(), "bounded read completed");
}

fn filter(h: &mut Harness, query: &str) {
    h.press(KeyCode::Esc);
    h.type_text(&format!("/{query}"));
    h.press(KeyCode::Enter);
}

fn live() -> (Harness, u64) {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let number: u64 = std::env::var("SBT_QUEUED_PR")
        .expect("SBT_QUEUED_PR")
        .parse()
        .unwrap();
    let source = switchbard_core::fetch_pull_requests(std::path::Path::new(&root)).unwrap();
    let mut h = Harness::new();
    for args in [
        vec!["init", "-q"],
        vec!["remote", "add", "origin", &source.repository_url],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&h.root)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    h.next_list_page();
    settle(&mut h);
    assert!(h.app.pull_requests.error.is_none());
    assert!(h
        .app
        .pull_requests
        .snapshot
        .as_ref()
        .unwrap()
        .queue_warning
        .is_none());
    (h, number)
}

#[test]
#[ignore = "requires authenticated GitHub, SBT_PR_REPO and currently queued SBT_QUEUED_PR; reads only"]
fn live_queue_status_filter_detail_resize_restart_and_stale_refresh() {
    let (mut h, number) = live();
    filter(&mut h, &format!("id:{number} status:queued"));
    assert_eq!(
        h.app.pull_requests.row().expect("queued row").number,
        number
    );
    assert_eq!(
        h.app.pull_requests.row().unwrap().lifecycle,
        switchbard_core::PrLifecycle::Open
    );
    render_sizes(&mut h, number);
    filter(&mut h, &format!("id:{number} status:open"));
    assert_eq!(
        h.app
            .pull_requests
            .row()
            .expect("open includes queued")
            .number,
        number
    );
    h.type_text("f2");
    assert!(h.render().contains("Queued"), "{}", h.render());
    h.press(KeyCode::Char('4'));
    assert!(h.app.pull_requests.filter.contains("status:queued"));
    assert_eq!(h.app.pull_requests.row().unwrap().number, number);
    h.type_text("s21");
    h.type_text("p2");
    pick(&mut h, "Queued");
    pick(&mut h, "red");
    h.press(KeyCode::Esc);
    assert_eq!(
        cell_fg(&h, &format!("#{number}")),
        Some(ratatui::style::Color::Red)
    );
    let resume = h.app.resume_state();
    h.app = open_app(&h.root, &h.config_path);
    h.app.resume_from(Some(&resume));
    settle(&mut h);
    assert!(h.render().contains("Queued"), "{}", h.render());
    stale_refresh(&mut h, number);
}

fn render_sizes(h: &mut Harness, number: u64) {
    for (width, height) in [(80, 24), (120, 40), (180, 50), (40, 12)] {
        h.terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        let row = screen
            .lines()
            .find(|line| line.contains(&format!("#{number}")))
            .expect("selected row rendered");
        assert!(row.contains("Queued"), "{screen}");
        let detail = h.press(KeyCode::Enter);
        assert!(detail.contains("Queued"), "{detail}");
        if width >= 120 {
            assert!(detail.contains("Merge queue: Queued"), "{detail}");
        }
        println!("{width}x{height}\n{detail}");
        h.press(KeyCode::Enter);
    }
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
}

fn stale_refresh(h: &mut Harness, number: u64) {
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&h.root)
        .args(["remote", "remove", "origin"])
        .status()
        .unwrap()
        .success());
    h.press(KeyCode::Char('r'));
    settle(h);
    let screen = h.render();
    assert!(screen.contains("STALE"), "{screen}");
    assert!(screen.contains("Queued"), "{screen}");
    assert_eq!(h.app.pull_requests.row().unwrap().number, number);
    println!("{screen}");
}

fn pick(h: &mut Harness, label: &str) {
    let picker = h.app.picker.as_ref().expect("picker open");
    let index = picker
        .matching()
        .iter()
        .position(|option| option.label == label)
        .expect("option exists");
    let selected = picker.selected;
    for _ in 0..selected {
        h.press(KeyCode::Up);
    }
    for _ in 0..index {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
}

#[test]
fn independent_delivery_and_queue_failures_are_both_visible() {
    let mut h = Harness::new();
    h.next_list_page();
    settle(&mut h);
    h.app.pull_requests.error = None;
    let mut observed = snapshot(switchbard_core::PrMergeQueue::Unknown);
    observed.enrichment_warning = Some("Delivery details unavailable: timeout".into());
    observed.queue_warning = Some("Merge queue unavailable: permission denied".into());
    h.app.pull_requests.snapshot = Some(observed);
    h.app.pull_requests.refilter();
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    let screen = h.render();
    assert!(
        screen.contains("Delivery + merge queue details incomplete"),
        "{screen}"
    );
    let detail = h.press(KeyCode::Enter);
    assert!(detail.contains("Merge queue: Unknown"), "{detail}");
    assert!(
        detail.contains("Delivery details unavailable: timeout"),
        "{detail}"
    );
    assert!(
        detail.contains("Merge queue unavailable: permission denied"),
        "{detail}"
    );
}

// Cached state fixtures exercise real keys/rendering, without replacing the GitHub client.
fn snapshot(queue: switchbard_core::PrMergeQueue) -> switchbard_core::PrSnapshot {
    switchbard_core::PrSnapshot {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        observed_at: std::time::SystemTime::now(),
        truncated: false,
        limit: 100,
        open_count: Ok(1),
        enrichment_warning: None,
        queue_warning: None,
        rows: vec![switchbard_core::PrListRow {
            id: "PR_1".into(),
            number: 1,
            title: "Queued build".into(),
            url: "https://github.com/owner/repo/pull/1".into(),
            head_oid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            draft: false,
            lifecycle: switchbard_core::PrLifecycle::Open,
            merged_at: None,
            merge_queue: queue,
            checks: switchbard_core::PrChecks::Passing,
            review: switchbard_core::PrReview::Approved,
            merge: switchbard_core::PrMerge::Mergeable,
        }],
    }
}

#[test]
fn known_queue_changes_notify_across_pages_unknown_does_not_claim_exit() {
    use switchbard_core::PrMergeQueue::{NotQueued, Queued, Unknown};
    let mut h = Harness::new();
    h.next_list_page();
    settle(&mut h);
    h.app.pull_requests.error = None;
    h.press(KeyCode::Char('n'));
    for (before, after, notice) in [
        (NotQueued, Queued, Some("merge queue: Queued")),
        (Queued, NotQueued, Some("merge queue: Not queued")),
        (Queued, Unknown, None),
        (Unknown, NotQueued, None),
        (Unknown, Queued, None),
        (Queued, Queued, None),
    ] {
        let previous = snapshot(before);
        let next = snapshot(after);
        h.app
            .pull_requests
            .notifications
            .observe(Some(&previous), &next);
        h.app.pull_requests.snapshot = Some(next);
        h.app.pull_requests.refilter();
        let screen = h.render();
        if let Some(notice) = notice {
            assert!(screen.contains(notice), "{screen}");
            h.next_list_page();
            assert!(h.render().contains(notice));
            h.press(KeyCode::Char('n'));
            h.next_list_page();
        } else {
            assert!(!screen.contains("merge queue:"), "{screen}");
        }
        assert!(h.app.pull_requests.notifications.is_empty());
    }
}
