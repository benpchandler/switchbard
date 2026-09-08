//! Real keys and real read-only GitHub journeys; never submits a remote merge.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant};

fn settle_prs(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(100);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
    }
    assert!(!h.app.pull_requests.loading());
}

#[test]
fn merge_on_tasks_or_empty_prs_cannot_change_tasks() {
    let mut h = Harness::new();
    let selected = h.selected_title();
    let before = h.app.resume_state();
    h.press(KeyCode::Char('m'));
    assert_eq!(h.app.resume_state(), before);
    h.press(KeyCode::Tab);
    h.press(KeyCode::Char('m'));
    assert!(h.render().contains("No PR selected"));
    h.press(KeyCode::Tab);
    assert_eq!(h.selected_title(), selected);
    settle_prs(&mut h);
}

fn settle_merge(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(100);
    while h.app.pr_merge.is_pending() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
        h.render();
    }
    assert!(!h.app.pr_merge.is_pending(), "merge preparation timed out");
}

fn live_pr() -> Harness {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let number = std::env::var("SBT_PR_MERGE_NUMBER").expect("SBT_PR_MERGE_NUMBER");
    let mut h = Harness::new();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.press(KeyCode::Tab);
    settle_prs(&mut h);
    assert!(h.app.pull_requests.error.is_none());
    h.type_text(&format!("/id:{number}"));
    h.press(KeyCode::Enter);
    assert!(h.app.pull_requests.row().is_some());
    h
}

#[test]
#[ignore = "requires SBT_PR_REPO and historical SBT_PR_MERGE_NUMBER; read-only"]
fn historical_pr_cannot_open_a_merge_confirmation() {
    let mut h = live_pr();
    assert_ne!(
        h.app.pull_requests.row().unwrap().lifecycle,
        switchbard_core::PrLifecycle::Open
    );
    h.press(KeyCode::Char('m'));
    settle_merge(&mut h);
    assert!(h.app.picker.is_none());
    assert!(!h.app.pr_merge.is_submitting());
    assert!(h.app.status.contains("Only open"), "{}", h.app.status);
}

#[test]
#[ignore = "requires SBT_PR_REPO and any SBT_PR_MERGE_NUMBER; read-only"]
fn leaving_during_preparation_never_reopens_confirmation() {
    let mut h = live_pr();
    h.press(KeyCode::Char('m'));
    h.press(KeyCode::Char('m'));
    h.press(KeyCode::Tab);
    settle_merge(&mut h);
    assert!(h.render().contains("[Tasks]"));
    assert!(h.app.picker.is_none());
    assert!(!h.app.pr_merge.is_submitting());
    h.press(KeyCode::Tab);
    assert!(h.app.picker.is_none());
}

#[test]
#[ignore = "requires SBT_PR_REPO and eligible open SBT_PR_MERGE_NUMBER; prepares and cancels only"]
fn live_merge_confirmation_defaults_to_cancel_and_refuses_hidden_confirmation() {
    use switchbard_tui::picker::{Payload, PickerPurpose};
    let mut h = live_pr();
    let head = h.app.pull_requests.row().unwrap().head_oid.clone();
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    h.press(KeyCode::Char('m'));
    settle_merge(&mut h);
    let picker = h.app.picker.as_ref().expect(&h.app.status);
    assert_eq!(picker.purpose, PickerPurpose::Merge);
    let method_label = picker
        .options
        .get(1)
        .expect("at least one merge method")
        .label
        .clone();
    assert_eq!(picker.highlighted().unwrap().payload, Payload::CancelMerge);
    assert!(h.app.pr_merge.confirmation_visible);
    assert!(h.render().contains(&head));
    h.type_text("merge squash");
    assert_eq!(
        h.app
            .picker
            .as_ref()
            .unwrap()
            .highlighted()
            .unwrap()
            .payload,
        Payload::CancelMerge
    );
    assert!(!h.app.pr_merge.is_submitting());
    for (width, height) in [(80, 24), (120, 40), (180, 50)] {
        h.terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        if h.app.pr_merge.confirmation_visible {
            assert!(screen.contains(&head), "{screen}");
            assert!(screen.contains("Cancel"), "{screen}");
            assert!(screen.contains(&method_label), "{screen}");
        } else {
            assert!(screen.contains("Enlarge terminal"), "{screen}");
        }
        println!("confirmation {width}x{height}\n{screen}");
    }
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 12)).unwrap();
    let screen = h.render();
    assert!(screen.contains("Enlarge terminal"), "{screen}");
    assert!(!h.app.pr_merge.confirmation_visible);
    // The false viewport guard is asserted before exercising refusal; no network write can pass it.
    h.press(KeyCode::Char('2'));
    assert!(
        h.app.status.contains("Merge not submitted"),
        "{}",
        h.app.status
    );
    assert!(!h.app.pr_merge.is_pending());
    assert!(h.app.picker.is_none());
    assert!(!h.app.pr_merge.is_submitting());
    assert!(h.app.pr_merge.last_result.is_none());
}

/// TASK-171: a PR GitHub will merge but does not call green (UNSTABLE, checks
/// failing with none required) used to be refused outright. It now prepares,
/// and the confirmation names the reason on screen, so the guard is the human
/// reading that line rather than sbt calling a mergeable PR unmergeable.
#[test]
#[ignore = "requires SBT_PR_REPO and an open, non-CLEAN SBT_PR_MERGE_NUMBER; prepares and cancels only"]
fn live_confirmation_names_a_non_clean_readiness_state() {
    let mut h = live_pr();
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    h.press(KeyCode::Char('m'));
    settle_merge(&mut h);
    assert!(h.app.picker.is_some(), "{}", h.app.status);
    let checks = h
        .app
        .pr_merge
        .confirmation_lines()
        .into_iter()
        .find(|line| line.starts_with("Checks:"))
        .expect("a non-CLEAN PR names its readiness in the confirmation");
    let screen = h.render();
    println!("{screen}");
    assert!(screen.contains(&checks), "{screen}");
    h.press(KeyCode::Esc);
    assert!(h.app.picker.is_none());
    assert!(!h.app.pr_merge.is_submitting());
    assert!(h.app.pr_merge.last_result.is_none());
}
