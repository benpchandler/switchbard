//! Historical fused-fence repair is staged, reviewable, and preserves originals.
use std::{fs, path::Path, process::Command};
use switchbard_core::storage::{ExchangeSnapshot, Store};

fn sb(root: &Path, db: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", db)
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .expect("sb");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF8")
}

#[test]
fn fused_fence_migration_reports_verified_repair_and_keeps_original_bytes() {
    let temp = tempfile::tempdir().expect("fixture");
    let root = temp.path().join("repo");
    let db = temp.path().join("store.sqlite3");
    let locator = "backlog/tasks/task-1 - Example.md";
    fs::create_dir_all(root.join("backlog/tasks")).expect("tasks");
    let original = "---\nid: TASK-1\ntitle: Example\nstatus: To Do\ncustom: {nested: [null, one]}\n---## Description\n\nKeep this description.\n\n## Acceptance Criteria\n- [ ] #1 Keep this criterion\n";
    fs::write(root.join(locator), original).expect("source");
    let before = sb(&root, &db, &["list"]);
    let preview: serde_json::Value =
        serde_json::from_str(&sb(&root, &db, &["storage", "migrate", "--kind", "task"]))
            .expect("preview");
    assert!(!db.exists());
    let repair = &preview["repairs"][0];
    assert_eq!(repair["locator"], locator);
    assert!(repair["repair"]["reason"]
        .as_str()
        .expect("reason")
        .contains("projection unchanged"));
    assert_ne!(
        repair["repair"]["source_digest"],
        repair["repair"]["target_digest"]
    );
    sb(
        &root,
        &db,
        &[
            "storage",
            "migrate",
            "--kind",
            "task",
            "--apply",
            "--preview-digest",
            preview["digest"].as_str().expect("digest"),
        ],
    );
    assert_eq!(before, sb(&root, &db, &["list"]));
    assert_eq!(
        fs::read(root.join(locator)).expect("original"),
        original.as_bytes()
    );
    let store = Store::open(&db).expect("store");
    let repo = store.repository(&root).expect("lookup").expect("repo");
    let records = store.list(&repo, "task").expect("records");
    assert_eq!(
        records[0].content,
        original.replacen("---##", "---\n##", 1).as_bytes()
    );
    switchbard_core::backlog::validate_storage_snapshot(
        &store.export_snapshot(&repo).expect("snapshot"),
    )
    .expect("valid repaired payload");
    drop(store);
    sb(
        &root,
        &db,
        &["edit", "1", "--description", "Edited after cutover."],
    );
    let file = temp.path().join("exchange.json");
    sb(
        &root,
        &db,
        &["storage", "export", "--file", file.to_str().expect("path")],
    );
    let snapshot = ExchangeSnapshot::parse(&fs::read(file).expect("exchange")).expect("wire");
    switchbard_core::backlog::validate_storage_snapshot(&snapshot).expect("valid edited payload");
    assert_eq!(
        fs::read(root.join(locator)).expect("original"),
        original.as_bytes()
    );
}
