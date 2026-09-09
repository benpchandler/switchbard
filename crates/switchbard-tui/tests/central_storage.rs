//! Real central-store mutations observed by the TUI, in an isolated process.
mod harness;
use harness::Harness;
use switchbard_core::storage::{MigrationPlan, SourceDocument, Store};

#[test]
fn central_refresh_preserves_selection_after_reparent() {
    if std::env::var_os("SWITCHBARD_TUI_CENTRAL_CHILD").is_none() {
        let dir = tempfile::tempdir().expect("isolated state");
        let status = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "central_refresh_preserves_selection_after_reparent",
                "--nocapture",
            ])
            .env("SWITCHBARD_TUI_CENTRAL_CHILD", "1")
            .env("SWITCHBARD_DATABASE", dir.path().join("state.sqlite3"))
            .status()
            .expect("run isolated central test");
        assert!(status.success());
        return;
    }
    let mut harness = Harness::new();
    let old_id = harness.app.selected_task().expect("selection").id.clone();
    let mut store = Store::open_default().expect("open database");
    let repo_id = store.bind_repository(&harness.root).expect("bind");
    let mut sources: Vec<_> = std::fs::read_dir(harness.root.join("backlog/tasks"))
        .expect("tasks")
        .map(|entry| entry.expect("entry").path())
        .map(|path| SourceDocument {
            kind: "task".into(),
            locator: path
                .strip_prefix(&harness.root)
                .expect("relative")
                .to_str()
                .expect("UTF8")
                .into(),
            path,
        })
        .collect();
    sources.push(SourceDocument {
        kind: "config".into(),
        locator: "backlog/config.yml".into(),
        path: harness.root.join("backlog/config.yml"),
    });
    let plan = MigrationPlan::capture(
        repo_id,
        vec![
            "task".into(),
            "goals".into(),
            "ranking".into(),
            "config".into(),
        ],
        sources,
    )
    .expect("capture");
    store.apply_migration(&plan).expect("cutover");
    drop(store);
    harness.app.tick();
    let selected = harness
        .app
        .selected_task()
        .expect("central selection")
        .clone();
    assert_eq!(selected.id, old_id);
    let identity = selected.storage_identity.expect("central identity");
    let repo = switchbard_core::load_backlog_repo(&harness.root).expect("reload");
    let parent = repo
        .tasks
        .iter()
        .find(|task| task.id != old_id)
        .expect("parent")
        .id
        .clone();
    let new_id = switchbard_core::move_backlog_task(&harness.root, &old_id, Some(&parent))
        .expect("reparent")
        .expect("changed id");
    let started = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(1050));
    harness.app.tick();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    let selected = harness.app.selected_task().expect("retained selection");
    assert_eq!(selected.id, new_id);
    assert_eq!(
        selected
            .storage_identity
            .as_ref()
            .expect("identity")
            .record_id,
        identity.record_id
    );
    switchbard_core::edit_backlog_task(
        &harness.root,
        &new_id,
        &switchbard_core::BacklogTaskPatch {
            title: Some("Changed centrally".into()),
            ..Default::default()
        },
    )
    .expect("external edit");
    std::thread::sleep(std::time::Duration::from_millis(1050));
    harness.app.tick();
    assert_eq!(
        harness.app.selected_task().expect("selected").title,
        "Changed centrally"
    );
    assert!(harness.render().contains("Changed centrally"));
    // An unavailable/corrupt central store must retain the last good snapshot,
    // then recover without waiting for a further change sequence increment.
    let database = std::path::PathBuf::from(std::env::var_os("SWITCHBARD_DATABASE").unwrap());
    let backup = std::fs::read(&database).expect("fixture backup");
    std::fs::write(&database, b"invalid sqlite fixture").expect("inject unavailable store");
    std::thread::sleep(std::time::Duration::from_millis(1050));
    harness.app.tick();
    assert_eq!(
        harness
            .app
            .selected_task()
            .expect("retained stale task")
            .title,
        "Changed centrally"
    );
    std::fs::write(&database, backup).expect("restore fixture");
    std::thread::sleep(std::time::Duration::from_millis(1050));
    harness.app.tick();
    assert_eq!(
        harness.app.selected_task().expect("reconnected task").id,
        new_id
    );
}
