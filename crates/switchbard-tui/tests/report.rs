//! Real-key report journeys against real repository storage contention.
mod harness;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use harness::Harness;
use std::time::{Duration, Instant};

#[test]
fn idea_submission_stays_responsive_while_storage_is_busy() {
    let mut h = Harness::new();
    let selected = h.selected_title();
    h.type_text(":idea keep the terminal responsive");
    let root = h.root.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let lock_thread = std::thread::spawn(move || {
        let _lock = switchbard_core::storage::RepositoryLock::acquire(&root).unwrap();
        ready_tx.send(()).unwrap();
        std::thread::sleep(Duration::from_secs(2));
    });
    ready_rx.recv().unwrap();
    let start = Instant::now();
    let screen = h.press(KeyCode::Enter);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "Enter blocked for {elapsed:?}: {screen}"
    );
    assert!(screen.contains("Saving"), "{screen}");
    let started = Instant::now();
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Enter);
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(h.app.mode, switchbard_tui::app::Mode::Browse);
    assert!(screen.contains("retry editing"), "{screen}");
    h.press(KeyCode::Tab);
    h.press(KeyCode::Tab);
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("[Inbox]"), "{screen}");
    assert!(screen.contains("Saving"), "{screen}");
    h.type_text(":bug second draft");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.input, "bug second draft");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('q'));
    assert!(!h.app.should_quit);
    let start = Instant::now();
    h.app.tick();
    assert!(start.elapsed() < Duration::from_millis(500));
    lock_thread.join().unwrap();
    let screen = h.wait_report();
    assert!(screen.contains("filed TASK-4"), "{screen}");
    assert!(screen.contains("[Inbox]"), "{screen}");
    assert_eq!(h.app.total_tasks(), 4);
    assert_eq!(h.app.status, "filed TASK-4");
    h.press(KeyCode::Tab);
    assert_eq!(h.selected_title(), selected);
    assert!(h.render().contains("filed TASK-4"));
    let task = h
        .app
        .tasks()
        .iter()
        .find(|task| task.title.contains("keep the terminal"))
        .unwrap();
    assert!(task.description.contains("page=Tasks"));
    h.type_text(":idea held enter must not submit");
    h.app.handle_key(KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    ));
    assert!(!h.app.report.is_pending());
    assert_eq!(h.app.total_tasks(), 4);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('n'));
    h.press(KeyCode::Tab);
    assert!(!h.render().contains("filed TASK-4"));
    h.press(KeyCode::Char('q'));
    assert!(h.app.should_quit);
}

#[test]
fn failed_report_retains_a_retryable_unicode_draft() {
    let mut h = Harness::new();
    let lock = h.root.join(".switchbard-storage.lock");
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    h.type_text(":bug café 界 does not respond");
    h.press(KeyCode::Enter);
    for _ in 0..600 {
        h.app.tick();
        if !h.app.report.is_pending() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
    let screen = h.render();
    assert!(
        screen.contains("Report failed; : restores draft"),
        "{screen}"
    );
    assert!(!screen.contains("filed TASK"));
    assert_eq!(h.app.total_tasks(), 3);
    h.press(KeyCode::Tab);
    assert!(h.render().contains("Report failed"));
    std::fs::remove_dir(&lock).unwrap();
    h.press(KeyCode::Char(':'));
    assert_eq!(h.app.input, "bug café 界 does not respond");
    h.press(KeyCode::Enter);
    let screen = h.wait_report();
    assert!(screen.contains("filed TASK-4"), "{screen}");
    assert_eq!(h.app.total_tasks(), 4);
}

#[test]
fn release_events_keep_auto_selection_and_filtered_success_has_full_id() {
    let mut h = Harness::new();
    h.type_text(":idea selected normally");
    h.press(KeyCode::Enter);
    h.app.handle_key(KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    h.wait_report();
    assert_eq!(h.selected_title(), "sbt idea: selected normally");
    h.type_text("/login");
    h.press(KeyCode::Enter);
    h.type_text(":idea hidden by current filter");
    h.press(KeyCode::Enter);
    for _ in 0..600 {
        h.app.tick();
        if h.app.total_tasks() == 5 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let screen = h.render();
    assert!(screen.contains("filed TASK-5"), "{screen}");
    assert_eq!(h.selected_title(), "Fix login redirect loop");
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8)).unwrap();
    assert!(h.render().contains("filed TASK-5"));
}

#[test]
fn focused_detail_adopts_background_refresh_without_storage_io_on_tick() {
    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, switchbard_tui::app::Mode::DetailFocus);
    let selected = h.selected_title();
    harness::seed(&h.root, "External refresh while focused", "To Do", &[]);
    h.app.tick();
    // Let the real background read finish while the event loop is idle, then
    // contend the core lock before the next frame collects its result.
    std::thread::sleep(Duration::from_secs(2));
    let root = h.root.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _lock = switchbard_core::storage::RepositoryLock::acquire(&root).unwrap();
        ready_tx.send(()).unwrap();
        std::thread::sleep(Duration::from_secs(2));
    });
    ready_rx.recv().unwrap();
    let started = Instant::now();
    h.app.tick();
    let elapsed = started.elapsed();
    holder.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(500),
        "focused refresh blocked input for {elapsed:?}"
    );
    h.tick_until_tasks_settle();
    assert_eq!(h.selected_title(), selected);
    assert_eq!(h.app.total_tasks(), 4);
    let narrow = h.render();
    println!("Focused refresh at 100 columns:\n{narrow}");
    assert!(h
        .app
        .tasks()
        .iter()
        .any(|task| task.title == "External refresh while focused"));
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 20)).unwrap();
    let wide = h.render();
    println!("Focused refresh at 160 columns:\n{wide}");
    assert!(wide.contains("External refresh while focused"), "{wide}");
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Enter);
    h.type_text("Done");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.selected_task().unwrap().status, "Done");
}
