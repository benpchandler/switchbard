//! Word-processor-style range select on the PR list: shift-down/shift-up
//! extend the bulk-merge mark from an anchor (offline; no GitHub reads).
mod harness;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness::*;
use std::time::SystemTime;
use switchbard_core::{PrChecks, PrLifecycle, PrListRow, PrMerge, PrReview, PrSnapshot};

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

/// Five open PRs, #41..#45, cursor starting on the first (#41).
fn five_open_prs() -> Harness {
    let mut h = Harness::new();
    h.next_list_page();
    h.app.pull_requests.snapshot = Some(snapshot(vec![
        row("pr-1", 41, PrLifecycle::Open),
        row("pr-2", 42, PrLifecycle::Open),
        row("pr-3", 43, PrLifecycle::Open),
        row("pr-4", 44, PrLifecycle::Open),
        row("pr-5", 45, PrLifecycle::Open),
    ]));
    h.app.pull_requests.refilter();
    h.app.pull_requests.step(isize::MIN);
    h
}

fn shift_down(h: &mut Harness) -> String {
    h.app
        .handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    h.render()
}

fn shift_up(h: &mut Harness) -> String {
    h.app
        .handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT));
    h.render()
}

#[test]
fn shift_down_extends_a_contiguous_range_and_the_screen_shows_it() {
    let mut h = five_open_prs();
    shift_down(&mut h);
    let screen = shift_down(&mut h);

    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3"]
    );
    assert!(
        h.app.status.contains("Marked for merge (3)"),
        "{}",
        h.app.status
    );
    assert!(screen.contains("[x] #41"), "{screen}");
    assert!(screen.contains("[x] #42"), "{screen}");
    assert!(screen.contains("[x] #43"), "{screen}");
    assert!(screen.contains("[ ] #44"), "{screen}");
    assert!(screen.contains("[ ] #45"), "{screen}");
}

#[test]
fn shift_up_contracts_the_range_and_unmarks_what_left_it() {
    let mut h = five_open_prs();
    shift_down(&mut h); // anchor pr-1, cursor pr-2: marks pr-1, pr-2
    shift_down(&mut h); // cursor pr-3: marks pr-1..pr-3
    shift_down(&mut h); // cursor pr-4: marks pr-1..pr-4
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3", "pr-4"]
    );

    shift_up(&mut h); // cursor back to pr-3: pr-4 leaves the range
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3"]
    );
    shift_up(&mut h); // cursor back to pr-2: pr-3 leaves the range
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2"]
    );
}

#[test]
fn a_row_marked_outside_the_sweep_survives_a_contraction_that_passes_over_it() {
    let mut h = five_open_prs();
    // Mark pr-3 by hand, independent of any sweep.
    h.app.pull_requests.step(2);
    h.press(KeyCode::Char(' '));
    assert_eq!(h.app.pull_requests.marked_in_view_order(), vec!["pr-3"]);

    // Start a fresh sweep from pr-1 and extend down across pr-3.
    h.app.pull_requests.step(isize::MIN);
    shift_down(&mut h); // pr-1, pr-2
    shift_down(&mut h); // + pr-3 (already marked outside the sweep)
    shift_down(&mut h); // + pr-4
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3", "pr-4"]
    );

    // Contract past pr-3 entirely: the sweep never owned it, so it must
    // still be marked once the range no longer covers it.
    shift_up(&mut h); // cursor pr-3
    shift_up(&mut h); // cursor pr-2: range is now pr-1..pr-2
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3"],
        "pr-3 was marked by space outside the sweep and must survive"
    );
}

#[test]
fn a_plain_down_between_two_extends_resets_the_anchor() {
    let mut h = five_open_prs();
    shift_down(&mut h); // anchor pr-1, cursor pr-2: marks pr-1, pr-2
    shift_down(&mut h); // cursor pr-3: marks pr-1..pr-3
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3"]
    );

    // A plain Down moves the cursor without marking.
    h.press(KeyCode::Down);
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3"],
        "a plain move must not mark anything"
    );
    assert_eq!(
        h.app.pull_requests.row().map(|r| r.id.as_str()),
        Some("pr-4")
    );

    // Extending upward now must anchor fresh at pr-4 (where the plain move
    // left the cursor), not at the stale pr-1 anchor from before it: a stale
    // anchor would keep the range entirely within pr-1..pr-3 and never touch
    // pr-4 at all, since stepping up from pr-4 lands back inside that range.
    shift_up(&mut h);
    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-3", "pr-4"],
        "a fresh anchor at pr-4 must sweep pr-4 in on the way back to pr-3"
    );
}

#[test]
fn a_closed_pr_inside_the_swept_range_is_not_marked() {
    let mut h = Harness::new();
    h.next_list_page();
    h.app.pull_requests.snapshot = Some(snapshot(vec![
        row("pr-1", 41, PrLifecycle::Open),
        row("pr-2", 42, PrLifecycle::Open),
        row("pr-3", 43, PrLifecycle::Merged),
        row("pr-4", 44, PrLifecycle::Open),
    ]));
    h.app.pull_requests.refilter();
    h.app.pull_requests.step(isize::MIN);

    shift_down(&mut h); // pr-1, pr-2
    shift_down(&mut h); // pr-3 is merged: skipped, exactly like `space`
    let screen = shift_down(&mut h); // pr-4

    assert_eq!(
        h.app.pull_requests.marked_in_view_order(),
        vec!["pr-1", "pr-2", "pr-4"]
    );
    assert!(screen.contains("[x] #41"), "{screen}");
    assert!(screen.contains("[x] #42"), "{screen}");
    assert!(
        screen.contains("[ ] #43"),
        "merged PR shows unchecked, exactly like a `space` refusal: {screen}"
    );
    assert!(screen.contains("[x] #44"), "{screen}");
}

// `mark`/`extend_down`/`extend_up` are shared-list actions: both list pages
// advertise the same chords, since a bulk apply on Tasks uses exactly this
// range-select mechanism. See
// `tests/task_range_select.rs::help_lists_the_selection_keys_on_tasks_and_pull_requests`.
#[test]
fn the_new_chords_appear_on_both_list_pages_help() {
    let mut h = Harness::new();
    h.next_list_page();
    let pr_help = h.press(KeyCode::Char('?'));
    assert!(
        pr_help.contains("shift-down") && pr_help.contains("extend_down"),
        "{pr_help}"
    );
    assert!(
        pr_help.contains("shift-up") && pr_help.contains("extend_up"),
        "{pr_help}"
    );
    h.press(KeyCode::Esc);

    h.next_list_page();
    let tasks_help = h.press(KeyCode::Char('?'));
    assert!(tasks_help.contains("extend_down"), "{tasks_help}");
    assert!(tasks_help.contains("extend_up"), "{tasks_help}");
}
