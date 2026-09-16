mod harness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness::*;
use std::time::Duration;
use switchbard_core::bug_run::{AgentOutcome, Run, RunOptions, RunState, Store};

fn repository(h: &Harness) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&h.root)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(output.status.success());
}
fn question(h: &Harness) -> Run {
    repository(h);
    let mut store = Store::open(h.root.join("bug-runs.sqlite3")).unwrap();
    let run = store
        .enqueue(&h.root, "TASK-1", RunOptions::default())
        .unwrap();
    let lease = store.claim(&run.id).unwrap();
    store
        .record_thread(&lease, "a0000000-0000-4000-8000-000000000001")
        .unwrap();
    store
        .complete_agent(
            &lease,
            AgentOutcome::AwaitingAnswer {
                question: "Should the redirect retain the original destination?".into(),
            },
        )
        .unwrap()
}
fn inbox(h: &mut Harness) {
    h.type_text(":inbox");
    h.press(KeyCode::Enter);
    for _ in 0..100 {
        h.app.tick();
        if !h.app.inbox.rows.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("Inbox did not load: {}", h.render());
}

#[test]
fn owner_reply_survives_restart_and_submits_once() {
    let mut h = Harness::new();
    let run = question(&h);
    inbox(&mut h);
    assert!(h.render().contains("Your answer needed"));
    h.press(KeyCode::Enter);
    h.type_text("Yes, keep /orders/東京");
    h.press(KeyCode::Enter);
    h.type_text("Do not change the sign-in page.");
    let draft = h.app.inbox.draft.clone();
    assert!(Store::open(h.root.join("bug-runs.sqlite3"))
        .unwrap()
        .get(&run.id)
        .unwrap()
        .draft
        .contains("東京"));
    h.app = open_app(&h.root, &h.config_path);
    inbox(&mut h);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.inbox.draft, draft);
    h.app
        .handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let saved = Store::open(h.root.join("bug-runs.sqlite3"))
        .unwrap()
        .get(&run.id)
        .unwrap();
    assert_eq!(saved.state, RunState::ResumeQueued);
    assert_eq!(saved.messages.last().unwrap().text, draft);
    h.app
        .handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(
        Store::open(h.root.join("bug-runs.sqlite3"))
            .unwrap()
            .get(&run.id)
            .unwrap()
            .messages,
        saved.messages
    );
}

#[test]
fn another_owner_cannot_overwrite_or_discard_a_conflicting_draft() {
    let mut h = Harness::new();
    let run = question(&h);
    inbox(&mut h);
    h.press(KeyCode::Enter);
    h.type_text("first");
    let mut store = Store::open(h.root.join("bug-runs.sqlite3")).unwrap();
    let latest = store.get(&run.id).unwrap();
    store
        .save_draft(&run.id, latest.revision, "other window")
        .unwrap();
    h.type_text(" window");
    h.press(KeyCode::Esc);
    assert!(h.app.inbox.editing);
    assert_eq!(h.app.inbox.draft, "first window");
    assert_eq!(store.get(&run.id).unwrap().draft, "other window");
    assert!(h.app.status.contains("Draft still here"));
    h.app
        .handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    let merged = store.get(&run.id).unwrap().draft;
    assert!(merged.contains("other window") && merged.contains("first window"));
    h.press(KeyCode::Esc);
    assert!(!h.app.inbox.editing);
}

#[test]
fn review_requires_explicit_publish_and_preserves_feedback() {
    let mut h = Harness::new();
    let run = question(&h);
    let mut store = Store::open(h.root.join("bug-runs.sqlite3")).unwrap();
    let run = store
        .save_draft(&run.id, run.revision, "Keep destination")
        .unwrap();
    let run = store.submit_reply(&run.id, run.revision).unwrap();
    let lease = store.claim(&run.id).unwrap();
    store.record_review_head(&lease, &"a".repeat(40)).unwrap();
    store
        .record_review_target(
            &lease,
            "https://github.com/fixture/repo.git",
            "fixture/repo",
        )
        .unwrap();
    store
        .complete_agent(
            &lease,
            AgentOutcome::AwaitingReview {
                summary: "Redirect now retains destination".into(),
                tests: vec!["Redirect journey passed".into()],
                risks: vec![],
            },
        )
        .unwrap();
    inbox(&mut h);
    assert!(h.render().contains("Your review needed"));
    h.press(KeyCode::Enter);
    h.type_text("Please also handle deep links");
    h.press(KeyCode::Esc);
    h.type_text(":publish");
    h.press(KeyCode::Enter);
    assert_eq!(store.get(&run.id).unwrap().state, RunState::AwaitingReview);
    assert!(h.app.status.contains("saved review reply"));
}

#[test]
fn bug_files_before_dispatch_and_idea_stays_capture_only() {
    let mut h = Harness::new();
    repository(&h);
    h.type_text(":bug Login redirects to the wrong destination");
    h.press(KeyCode::Enter);
    assert!(h.app.status.contains("filed"));
    let store = Store::open(h.root.join("bug-runs.sqlite3")).unwrap();
    let runs = store.list(&h.root).unwrap();
    assert_eq!(runs.len(), 1);
    let backlog = switchbard_core::load_backlog_repo(&h.root).unwrap();
    let task = backlog
        .tasks
        .iter()
        .find(|t| t.id == runs[0].task_id)
        .unwrap();
    assert!(
        switchbard_core::read_backlog_task_snapshot(&h.root, &task.id)
            .unwrap()
            .content
            .contains("## Screen")
    );
    assert!(!task.labels.iter().any(|l| l == "dispatch"));
    h.type_text(":idea Add a help view");
    h.press(KeyCode::Enter);
    assert_eq!(store.list(&h.root).unwrap().len(), 1);
}

#[test]
fn question_and_draft_render_at_narrow_and_wide_sizes() {
    let mut h = Harness::new();
    let run = question(&h);
    inbox(&mut h);
    for (width, height) in [(40, 8), (80, 24), (120, 40)] {
        h.terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        assert!(screen.contains("Inbox"), "{screen}");
        assert!(screen.contains(&run.task_id), "{screen}");
        h.press(KeyCode::Enter);
        h.type_text("Keep destination");
        assert!(h.render().contains("Keep destination"), "{}", h.render());
        h.press(KeyCode::Esc);
    }
}

#[test]
fn publishing_needs_visible_confirmation_and_remappable_inbox_key() {
    let mut h = Harness::new();
    let run = question(&h);
    let mut store = Store::open(h.root.join("bug-runs.sqlite3")).unwrap();
    let run = store.save_draft(&run.id, run.revision, "Fix it").unwrap();
    let run = store.submit_reply(&run.id, run.revision).unwrap();
    let lease = store.claim(&run.id).unwrap();
    store.record_review_head(&lease, &"b".repeat(40)).unwrap();
    store
        .record_review_target(
            &lease,
            "https://github.com/fixture/repo.git",
            "fixture/repo",
        )
        .unwrap();
    store
        .complete_agent(
            &lease,
            AgentOutcome::AwaitingReview {
                summary: "Fixed destination".into(),
                tests: vec!["Journey passed".into()],
                risks: vec![],
            },
        )
        .unwrap();
    std::fs::write(
        &h.config_path,
        "return { inbox_keys = { p = 'none', x = 'publish' } }",
    )
    .unwrap();
    inbox(&mut h);
    h.press(KeyCode::Char('p'));
    assert!(h.app.inbox.publish_confirmation.is_none());
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
    h.press(KeyCode::Char('x'));
    assert!(h.app.inbox.publish_confirmation.is_some());
    h.press(KeyCode::Enter);
    assert_eq!(store.get(&run.id).unwrap().state, RunState::AwaitingReview);
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    let screen = h.render();
    assert!(screen.contains("Reviewed commit"), "{screen}");
    assert!(screen.contains("GitHub: fixture/repo"), "{screen}");
    assert!(screen.contains(&"b".repeat(40)), "{screen}");
    assert!(h.app.inbox.confirmation_visible);
    h.press(KeyCode::Enter);
    assert_eq!(store.get(&run.id).unwrap().state, RunState::PublishQueued);
}
