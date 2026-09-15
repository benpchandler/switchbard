//! TASK-204: while a confirmed merge hasn't shown up in GitHub's (eventually
//! consistent) list read yet, the affected row and the observation header
//! say so. `PullRequests` is the one writer for that state; these tests
//! assert only that rendering reflects it.
mod harness;

use harness::*;
use std::time::{Duration, Instant, SystemTime};
use switchbard_core::{PrChecks, PrLifecycle, PrListRow, PrMerge, PrReview, PrSnapshot};

/// Landing on the Pull Requests page starts its own real (bounded, local,
/// no-network-in-this-fixture) background refresh; wait it out the same way
/// `tests/pr_refresh.rs` does, so the assertions below never race it.
fn settle(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        let root = h.root.clone();
        h.app.pull_requests.tick(&root, 60, Instant::now());
    }
    assert!(
        !h.app.pull_requests.loading(),
        "initial PR refresh must finish"
    );
}

fn row(id: &str, number: u64, head_oid: &str, lifecycle: PrLifecycle) -> PrListRow {
    PrListRow {
        id: id.into(),
        number,
        title: "Fix login redirect loop".into(),
        url: format!("https://github.com/owner/repo/pull/{number}"),
        head_oid: head_oid.into(),
        draft: false,
        lifecycle,
        merge_queue: switchbard_core::PrMergeQueue::Unknown,
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
        open_count: Ok(1),
        enrichment_warning: None,
        queue_warning: None,
    }
}

#[test]
fn pending_row_and_header_render_while_a_merge_has_not_converged() {
    let mut h = Harness::new();
    h.next_list_page();
    settle(&mut h);
    assert!(h.render().contains("[Pull Requests]"));

    let target = row("pr-1", 42, "headoid", PrLifecycle::Open);
    h.app.pull_requests.snapshot = Some(snapshot(vec![target.clone()]));
    h.app.pull_requests.refilter();
    let screen = h.render();
    assert!(
        !screen.contains("merging") && !screen.contains("syncing"),
        "no expectation yet: {screen}"
    );

    h.app
        .pull_requests
        .expect_merge(target.id.clone(), target.head_oid.clone(), Instant::now());
    let pending = h.render();
    assert!(
        pending.contains("merging"),
        "row must show a pending marker while GitHub still reports Open: {pending}"
    );
    assert!(
        pending.contains("syncing"),
        "header refresh label must say the list is syncing after merge: {pending}"
    );
    // Convergence (row absent or state merged clears the expectation) is
    // covered deterministically, with injected `Instant`s and no thread
    // spawn, in `pull_requests::tests`; this file proves only that
    // rendering reflects whatever `PullRequests` currently reports.
}

#[test]
fn a_second_confirmed_merge_replaces_the_first_expectation() {
    let mut h = Harness::new();
    h.next_list_page();
    settle(&mut h);
    let first = row("pr-1", 42, "headoid-1", PrLifecycle::Open);
    let second = row("pr-2", 43, "headoid-2", PrLifecycle::Open);
    h.app.pull_requests.snapshot = Some(snapshot(vec![first.clone(), second.clone()]));
    h.app.pull_requests.refilter();

    h.app
        .pull_requests
        .expect_merge(first.id.clone(), first.head_oid.clone(), Instant::now());
    assert!(h.app.pull_requests.expecting_merge(&first));

    h.app
        .pull_requests
        .expect_merge(second.id.clone(), second.head_oid.clone(), Instant::now());
    assert!(
        !h.app.pull_requests.expecting_merge(&first),
        "single outstanding expectation: the second merge replaces the first"
    );
    assert!(h.app.pull_requests.expecting_merge(&second));

    let screen = h.render();
    assert!(screen.contains("merging"), "{screen}");
}
