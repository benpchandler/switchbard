//! TASK-250: mark PRs with `space`, merge them all with one `m` (offline).
//! Preparation runs against a repo with no GitHub remote, so the queue is
//! proved up to the point a real merge would need the network and no further.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant, SystemTime};
use switchbard_core::{PrChecks, PrLifecycle, PrListRow, PrMerge, PrReview, PrSnapshot};

fn settle(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        let root = h.root.clone();
        h.app.pull_requests.tick(&root, 60, Instant::now());
    }
    assert!(!h.app.pull_requests.loading());
}

fn settle_merge(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while h.app.pr_merge.is_pending() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
        h.render();
    }
    assert!(!h.app.pr_merge.is_pending(), "merge preparation timed out");
}

fn row(id: &str, number: u64, lifecycle: PrLifecycle) -> PrListRow {
    PrListRow {
        id: id.into(),
        number,
        title: format!("Change {number}"),
        url: format!("https://github.com/owner/repo/pull/{number}"),
        head_oid: format!("head-{number}"),
        draft: false,
        lifecycle,
        merged_at: None,
        checks: PrChecks::Passing,
        review: PrReview::Approved,
        merge: PrMerge::Mergeable,
    }
}

fn snapshot(rows: Vec<PrListRow>) -> PrSnapshot {
    PrSnapshot {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        observed_at: SystemTime::now(),
        rows,
        truncated: false,
        limit: 100,
        open_count: Ok(3),
        enrichment_warning: None,
    }
}

fn three_prs() -> Harness {
    let mut h = Harness::new();
    h.next_list_page();
    settle(&mut h);
    h.app.pull_requests.snapshot = Some(snapshot(vec![
        row("pr-1", 41, PrLifecycle::Open),
        row("pr-2", 42, PrLifecycle::Open),
        row("pr-3", 43, PrLifecycle::Merged),
    ]));
    h.app.pull_requests.refilter();
    h.app.pull_requests.step(isize::MIN);
    h
}

#[test]
fn space_marks_open_prs_and_shows_a_checkbox() {
    let mut h = three_prs();
    let plain = h.render();
    assert!(!plain.contains("[x]") && !plain.contains("[ ]"), "{plain}");

    h.press(KeyCode::Char(' '));
    assert_eq!(h.app.pull_requests.marked.len(), 1);
    assert!(
        h.app.status.contains("Marked for merge (1)"),
        "{}",
        h.app.status
    );
    let screen = h.render();
    assert!(screen.contains("[x] #41"), "{screen}");
    assert!(screen.contains("[ ] #42"), "{screen}");
    // Marking steps the cursor down, so the next space marks the next PR.
    h.press(KeyCode::Char(' '));
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2"]
    );

    // A merged PR cannot be marked.
    h.press(KeyCode::Char(' '));
    assert!(h.app.status.contains("Only open"), "{}", h.app.status);
    assert_eq!(h.app.pull_requests.marked.len(), 2);

    // Toggling again unmarks.
    h.app.pull_requests.step(isize::MIN);
    h.press(KeyCode::Char(' '));
    assert_eq!(h.app.pull_requests.marked_in_view_order(), vec!["pr-2"]);
    assert!(h.render().contains("[ ] #41"));

    // Esc clears marks before it touches anything else.
    h.press(KeyCode::Esc);
    assert!(h.app.pull_requests.marked.is_empty());
    assert!(!h.render().contains("[ ]"));
}

#[test]
fn m_with_marks_queues_them_in_list_order_and_names_the_rest() {
    let mut h = three_prs();
    h.press(KeyCode::Char(' '));
    h.press(KeyCode::Char(' '));
    // Cursor sits on #43; m must start from the first marked PR, not the cursor.
    h.press(KeyCode::Char('m'));
    let queue = h.app.pr_merge.queue.clone().expect("bulk merge queue");
    assert_eq!(queue.total, 2);
    assert_eq!(queue.merged, 0);
    assert_eq!(queue.method, None);
    assert_eq!(queue.remaining, vec![("pr-2".to_string(), 42)]);
    assert_eq!(h.app.pull_requests.row().map(|r| r.number), Some(41));

    // Preparation cannot reach GitHub from this fixture: it fails, and the
    // queue is dropped with it rather than lingering for the next m.
    settle_merge(&mut h);
    assert!(h.app.picker.is_none());
    assert!(h.app.pr_merge.queue.is_none(), "{}", h.app.status);
    assert!(!h.app.pr_merge.is_submitting());
    assert!(h.app.pr_merge.last_result.is_none());
}

#[test]
fn moving_the_cursor_drops_an_unconfirmed_queue_but_keeps_the_marks() {
    let mut h = three_prs();
    h.press(KeyCode::Char(' '));
    h.press(KeyCode::Char(' '));
    h.press(KeyCode::Char('m'));
    assert!(h.app.pr_merge.queue.is_some());
    h.press(KeyCode::Down);
    assert!(h.app.pr_merge.queue.is_none());
    settle_merge(&mut h);
    assert!(h.app.pr_merge.queue.is_none());
    // Marks are not the queue: they stay until merged or cleared.
    assert_eq!(h.app.pull_requests.marked.len(), 2);
}
