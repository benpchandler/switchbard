//! Real TUI keys against authoritative SQLite, isolated from the user's store.
mod harness;
use crossterm::event::KeyCode;
use switchbard_core::storage::{MigrationPlan, SourceDocument, Store};

#[test]
fn central_detail_without_retained_files() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "central_detail_worker", "--nocapture"])
        .env("SWITCHBARD_DATABASE", dir.path().join("state.sqlite3"))
        .env("SWITCHBARD_CENTRAL_DETAIL_TEST", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn central_detail_worker() {
    if std::env::var_os("SWITCHBARD_CENTRAL_DETAIL_TEST").is_none() {
        return;
    }
    let mut h = harness::Harness::new();
    let paths: Vec<_> = h.app.tasks().iter().map(|task| task.path.clone()).collect();
    let mut store = Store::open_default().unwrap();
    let repo = store.bind_repository(&h.root).unwrap();
    let sources = paths
        .iter()
        .map(|path| SourceDocument {
            kind: "task".into(),
            locator: path.strip_prefix(&h.root).unwrap().to_str().unwrap().into(),
            path: path.clone(),
        })
        .collect();
    store
        .apply_migration(&MigrationPlan::capture(repo, vec!["task".into()], sources).unwrap())
        .unwrap();
    for path in paths {
        std::fs::remove_file(path).unwrap();
    }
    h.app = harness::open_app(&h.root, &h.config_path);
    h.render();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Enter);
    assert_eq!(
        h.app.mode,
        switchbard_tui::app::Mode::DetailInput(switchbard_tui::app::DetailInputKind::Title),
        "{screen}"
    );
    for _ in 0..h.app.input.chars().count() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("Central title edited");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("title saved"), "{screen}");
    let id = h.app.selected_task().unwrap().id.clone();
    for _ in 0..7 {
        h.press(KeyCode::Char('j'));
    }
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("acceptance #1 checked"), "{screen}");
    switchbard_core::edit_backlog_task(
        &h.root,
        &id,
        &switchbard_core::BacklogTaskPatch {
            title: Some("Changed elsewhere".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("changed"), "{screen}");
    let task = switchbard_core::load_backlog_repo(&h.root)
        .unwrap()
        .tasks
        .into_iter()
        .find(|task| task.id == id)
        .unwrap();
    assert!(
        task.acceptance_criteria[0].checked,
        "stale toggle must not uncheck the criterion"
    );
    assert_eq!(task.title, "Changed elsewhere");
}
