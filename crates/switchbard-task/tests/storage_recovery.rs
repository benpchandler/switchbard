//! Recovery journeys use fresh databases and never touch the user's central store.
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use switchbard_core::storage::{MigrationPlan, SourceDocument, Store};

fn sb(root: &Path, database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", database)
        .arg("--repo")
        .arg(root)
        .arg("storage")
        .args(args)
        .output()
        .expect("run sb")
}

fn success(output: Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

fn fixture(root: &Path, database: &Path) -> Store {
    fs::create_dir_all(root).unwrap();
    let source = root.join("task.md");
    fs::write(&source, b"original raw bytes with custom: [null, {}]\n").unwrap();
    let mut store = Store::open(database).unwrap();
    let repo = store.bind_repository(root).unwrap();
    let plan = MigrationPlan::capture(
        repo.clone(),
        vec!["task".into()],
        vec![SourceDocument {
            kind: "task".into(),
            locator: "task-1.md".into(),
            path: source,
        }],
    )
    .unwrap();
    store.apply_migration(&plan).unwrap();
    store
        .mutate(&repo, "task", "task-1.md", None, |_| {
            Ok(Some(b"changed live bytes".to_vec()))
        })
        .unwrap();
    let ordering = root.join("ordering.yml");
    fs::write(&ordering, "ranked: [unknown/task-1]\ncustom: keep\n").unwrap();
    store
        .migrate_workspace_ordering_checked(
            &ordering,
            &[],
            &switchbard_core::storage::workspace_ordering_source_digest(&ordering).unwrap(),
        )
        .unwrap();
    store
}

#[test]
fn backup_restore_preserves_all_table_fingerprint_and_original_backup_bytes() {
    let fixture_root = tempfile::tempdir().unwrap();
    let root = fixture_root.path().join("repo");
    let database = fixture_root.path().join("source.db");
    let backup = fixture_root.path().join("backup.db");
    let restored = fixture_root.path().join("restored.db");
    let store = fixture(&root, &database);
    let fingerprint = store.recovery_fingerprint().unwrap();
    let snapshot = store
        .export_snapshot(&store.repository(&root).unwrap().unwrap())
        .unwrap();
    let result = success(sb(
        &root,
        &database,
        &["backup", "--file", backup.to_str().unwrap()],
    ));
    assert_eq!(result["fingerprint"].as_str(), Some(fingerprint.as_str()));
    let source_bytes = fs::read(&backup).unwrap();
    let result = success(sb(
        &root,
        &restored,
        &["restore", "--from", backup.to_str().unwrap()],
    ));
    assert_eq!(result["fingerprint"].as_str(), Some(fingerprint.as_str()));
    assert_eq!(fs::read(&backup).unwrap(), source_bytes);
    let restored_store = Store::open(&restored).unwrap();
    assert_eq!(restored_store.recovery_fingerprint().unwrap(), fingerprint);
    let restored_snapshot = restored_store
        .export_snapshot(&restored_store.repository(&root).unwrap().unwrap())
        .unwrap();
    assert_eq!(restored_snapshot, snapshot);
    assert_eq!(
        restored_store.workspace_ordering().unwrap(),
        store.workspace_ordering().unwrap()
    );
    // Replica rotation does not change existing clocks, but a new edit gets a new component.
    drop(restored_store);
    let mut restored_store = Store::open(&restored).unwrap();
    let repo = restored_store.repository(&root).unwrap().unwrap();
    restored_store
        .mutate(&repo, "task", "task-1.md", None, |_| {
            Ok(Some(b"restored new edit".to_vec()))
        })
        .unwrap();
    assert_eq!(
        restored_store.export_snapshot(&repo).unwrap().records[0]
            .clock
            .len(),
        2
    );
    assert_eq!(store.recovery_fingerprint().unwrap(), fingerprint);
}

#[test]
fn recovery_refuses_existing_files_and_missing_database_with_marker() {
    let fixture_root = tempfile::tempdir().unwrap();
    let root = fixture_root.path().join("repo");
    let database = fixture_root.path().join("source.db");
    let backup = fixture_root.path().join("backup.db");
    let store = fixture(&root, &database);
    success(sb(
        &root,
        &database,
        &["backup", "--file", backup.to_str().unwrap()],
    ));
    let before = fs::read(&backup).unwrap();
    assert!(!sb(
        &root,
        &database,
        &["backup", "--file", backup.to_str().unwrap()]
    )
    .status
    .success());
    assert_eq!(fs::read(&backup).unwrap(), before);
    assert!(!sb(
        &root,
        &database,
        &["restore", "--from", backup.to_str().unwrap()]
    )
    .status
    .success());
    let missing = fixture_root.path().join("missing.db");
    drop(Store::open(&missing).unwrap());
    fs::remove_file(&missing).unwrap();
    assert!(!sb(
        &root,
        &missing,
        &["restore", "--from", backup.to_str().unwrap()]
    )
    .status
    .success());
    assert!(!missing.exists());
    assert_eq!(fs::read(&backup).unwrap(), before);
    assert!(store.recovery_fingerprint().is_ok());
}

#[test]
fn rebind_cli_preserves_repository_content_at_new_checkout() {
    let fixture_root = tempfile::tempdir().unwrap();
    let root = fixture_root.path().join("repo");
    let moved = fixture_root.path().join("moved");
    let database = fixture_root.path().join("source.db");
    let store = fixture(&root, &database);
    let repo = store.repository(&root).unwrap().unwrap();
    let snapshot = store.export_snapshot(&repo).unwrap();
    fs::rename(&root, &moved).unwrap();
    success(sb(&moved, &database, &["rebind", "--repo-id", &repo.0]));
    assert_eq!(store.repository(&moved).unwrap(), Some(repo.clone()));
    assert_eq!(store.repository(&root).unwrap(), Some(repo.clone()));
    assert_eq!(store.export_snapshot(&repo).unwrap(), snapshot);
}

#[test]
fn empty_or_non_sqlite_restore_source_is_not_treated_as_a_fresh_database() {
    let fixture_root = tempfile::tempdir().unwrap();
    let root = fixture_root.path().join("repo");
    fs::create_dir(&root).unwrap();
    let source = fixture_root.path().join("empty");
    let destination = fixture_root.path().join("new.db");
    fs::write(&source, []).unwrap();
    assert!(!sb(
        &root,
        &destination,
        &["restore", "--from", source.to_str().unwrap()]
    )
    .status
    .success());
    assert!(!destination.exists());
    fs::write(&source, b"not a sqlite database at all").unwrap();
    assert!(!sb(
        &root,
        &destination,
        &["restore", "--from", source.to_str().unwrap()]
    )
    .status
    .success());
    assert!(!destination.exists());
}
