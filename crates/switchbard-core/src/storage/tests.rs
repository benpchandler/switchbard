use super::*;
use tempfile::tempdir;

fn empty_authority(store: &mut Store, root: &Path, kind: &str) -> RepositoryId {
    let repo = store.bind_repository(root).unwrap();
    let plan = MigrationPlan::capture(repo.clone(), vec![kind.into()], vec![]).unwrap();
    store.apply_migration(&plan).unwrap();
    repo
}

#[test]
fn per_kind_authority_and_flexible_bytes_survive_reopen() {
    let root = tempdir().unwrap();
    let path = root.path().join("store.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let repo = empty_authority(&mut store, root.path(), "initiative");
    assert!(!store.authority(&repo, "task").unwrap());
    let bytes = b"---\ncustom: {nested: [null, '', {}, []]}\n---\n## Custom\n\xff";
    let first = store
        .mutate(&repo, "initiative", "one", None, |_| {
            Ok(Some(bytes.to_vec()))
        })
        .unwrap()
        .unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.read(&repo, "initiative", "one").unwrap(), Some(first));
    with_test_database(&path, || assert_eq!(default_database_path().unwrap(), path));
}

#[test]
fn migration_rechecks_sources_and_preserves_original_bytes() {
    let root = tempdir().unwrap();
    let source = root.path().join("old.md");
    std::fs::write(&source, "original").unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = store.bind_repository(root.path()).unwrap();
    let plan = MigrationPlan::capture(
        repo.clone(),
        vec!["task".into()],
        vec![SourceDocument {
            kind: "task".into(),
            locator: "TASK-1".into(),
            path: source.clone(),
        }],
    )
    .unwrap();
    std::fs::write(&source, "changed").unwrap();
    assert!(store.apply_migration(&plan).is_err());
    assert!(!store.authority(&repo, "task").unwrap());
    assert!(store.list(&repo, "task").unwrap().is_empty());
    std::fs::write(&source, "original").unwrap();
    store.apply_migration(&plan).unwrap();
    store
        .mutate(&repo, "task", "TASK-1", None, |_| Ok(Some(b"new".to_vec())))
        .unwrap();
    let original: Vec<u8> = store
        .connection
        .query_row("SELECT content FROM migration_sources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(original, b"original");
    assert_eq!(std::fs::read(source).unwrap(), b"original");
}

#[test]
fn stale_revision_and_closure_failure_have_no_effect() {
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = empty_authority(&mut store, root.path(), "task");
    let first = store
        .mutate(&repo, "task", "1", None, |_| Ok(Some(b"one".to_vec())))
        .unwrap()
        .unwrap();
    let second = store
        .mutate(&repo, "task", "1", Some(first.revision), |_| {
            Ok(Some(b"two".to_vec()))
        })
        .unwrap()
        .unwrap();
    let sequence = store.change_sequence().unwrap();
    assert!(store
        .mutate(&repo, "task", "1", Some(first.revision), |_| Ok(Some(
            b"lost".to_vec()
        )))
        .is_err());
    assert!(store
        .mutate(&repo, "task", "1", None, |_| anyhow::bail!(
            "transform failed"
        ))
        .is_err());
    assert_eq!(store.read(&repo, "task", "1").unwrap(), Some(second));
    assert_eq!(store.change_sequence().unwrap(), sequence);
}

#[test]
fn deletion_retains_identity_and_history() {
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = empty_authority(&mut store, root.path(), "task");
    let first = store
        .mutate(&repo, "task", "1", None, |_| Ok(Some(b"one".to_vec())))
        .unwrap()
        .unwrap();
    let deleted = store
        .mutate(&repo, "task", "1", None, |_| Ok(None))
        .unwrap()
        .unwrap();
    assert!(deleted.deleted);
    assert_eq!(deleted.id, first.id);
    assert_eq!(deleted.content, first.content);
    assert_eq!(deleted.revision, 2);
}

#[test]
fn malformed_database_never_falls_back_to_legacy() {
    let root = tempdir().unwrap();
    let path = root.path().join("db");
    assert!(Store::open_existing(&path).unwrap().is_none());
    permissions::prepare(&path).unwrap();
    std::fs::write(&path, b"not sqlite").unwrap();
    assert!(Store::open_existing(&path).is_err());
}

#[test]
fn linked_worktrees_share_identity() {
    let root = tempdir().unwrap();
    let primary = root.path().join("primary");
    std::fs::create_dir(&primary).unwrap();
    let run = |args: &[&str]| {
        let output = crate::git_cmd()
            .arg("-C")
            .arg(&primary)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "-q"]);
    run(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "--allow-empty",
        "-qm",
        "initial",
    ]);
    let sibling = root.path().join("sibling");
    run(&[
        "worktree",
        "add",
        "-qb",
        "sibling",
        sibling.to_str().unwrap(),
    ]);
    let mut store = Store::open(root.path().join("db")).unwrap();
    assert_eq!(
        store.bind_repository(&primary).unwrap(),
        store.bind_repository(&sibling).unwrap()
    );
}

#[test]
fn duplicate_sources_require_equal_bytes() {
    let root = tempdir().unwrap();
    let sources: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|name| {
            let path = root.path().join(name);
            std::fs::write(&path, name).unwrap();
            SourceDocument {
                kind: "task".into(),
                locator: "1".into(),
                path,
            }
        })
        .collect();
    assert!(MigrationPlan::capture(
        RepositoryId(Uuid::new_v4().to_string()),
        vec!["task".into()],
        sources
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn private_database_and_symlink_rejection() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = tempdir().unwrap();
    let path = root.path().join("db");
    let store = Store::open(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(store);
    let link = root.path().join("link");
    symlink(&path, &link).unwrap();
    assert!(Store::open(&link).is_err());
}

#[test]
fn independent_connections_do_not_lose_read_modify_writes() {
    let root = tempdir().unwrap();
    let path = root.path().join("db");
    let mut store = Store::open(&path).unwrap();
    let repo = empty_authority(&mut store, root.path(), "task");
    store
        .mutate(&repo, "task", "one", None, |_| Ok(Some(b"0".to_vec())))
        .unwrap();
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            let repo = repo.clone();
            std::thread::spawn(move || {
                let mut store = Store::open(path).unwrap();
                for _ in 0..10 {
                    store
                        .mutate(&repo, "task", "one", None, |document| {
                            let previous: u64 =
                                std::str::from_utf8(&document.unwrap().content)?.parse()?;
                            Ok(Some((previous + 1).to_string().into_bytes()))
                        })
                        .unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        store.read(&repo, "task", "one").unwrap().unwrap().content,
        b"40"
    );
}

#[test]
fn backup_restores_authority_documents_and_provenance() {
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = empty_authority(&mut store, root.path(), "task");
    store
        .mutate(&repo, "task", "1", None, |_| {
            Ok(Some(b"custom: [null, {}]".to_vec()))
        })
        .unwrap();
    let backup = root.path().join("backup/db");
    store.backup_to(&backup).unwrap();
    let restored = Store::open(&backup).unwrap();
    assert!(restored.authority(&repo, "task").unwrap());
    assert_eq!(
        restored.list(&repo, "task").unwrap(),
        store.list(&repo, "task").unwrap()
    );
    assert!(store.backup_to(&backup).is_err());
}

#[test]
fn migration_failure_rolls_back_documents_provenance_and_authority() {
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = store.bind_repository(root.path()).unwrap();
    let path = root.path().join("source");
    std::fs::write(&path, "original").unwrap();
    let plan = MigrationPlan::capture(
        repo.clone(),
        vec!["task".into()],
        vec![SourceDocument {
            kind: "task".into(),
            locator: "1".into(),
            path,
        }],
    )
    .unwrap();
    store.connection.execute_batch("CREATE TRIGGER reject_cutover BEFORE INSERT ON authority BEGIN SELECT RAISE(ABORT,'injected cutover failure'); END;").unwrap();
    assert!(store.apply_migration(&plan).is_err());
    assert!(!store.authority(&repo, "task").unwrap());
    assert!(store.list(&repo, "task").unwrap().is_empty());
    let sources: usize = store
        .connection
        .query_row("SELECT COUNT(*) FROM migration_sources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(sources, 0);
}

#[test]
fn unknown_schema_and_foreign_database_are_rejected() {
    let root = tempdir().unwrap();
    let path = root.path().join("db");
    let store = Store::open(&path).unwrap();
    store
        .connection
        .execute_batch("PRAGMA user_version=999")
        .unwrap();
    drop(store);
    assert!(Store::open(&path).is_err());
    let foreign = root.path().join("foreign");
    permissions::prepare(&foreign).unwrap();
    let connection = Connection::open(&foreign).unwrap();
    connection
        .execute_batch("CREATE TABLE other(data TEXT)")
        .unwrap();
    drop(connection);
    assert!(Store::open(&foreign).is_err());
}

#[test]
fn lost_established_database_never_restores_legacy_authority() {
    let root = tempdir().unwrap();
    let path = root.path().join("db");
    let mut store = Store::open(&path).unwrap();
    empty_authority(&mut store, root.path(), "task");
    drop(store);
    std::fs::remove_file(&path).unwrap();
    assert!(Store::open_existing(&path).is_err());
    assert!(Store::open(&path).is_err());
    assert!(!path.exists());
}

#[test]
fn schema_one_upgrade_preserves_records_and_adds_causal_identity() {
    let root = tempdir().unwrap();
    let path = root.path().join("v1");
    permissions::prepare(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("schema.sql"))
        .unwrap();
    let repo = Uuid::new_v4().to_string();
    let id = Uuid::new_v4().to_string();
    connection
        .execute("INSERT INTO bindings VALUES ('legacy-binding',?1)", [&repo])
        .unwrap();
    connection
        .execute("INSERT INTO authority VALUES (?1,'task')", [&repo])
        .unwrap();
    connection
        .execute(
            "INSERT INTO documents VALUES (?1,?2,'task','1',2,?3,0)",
            params![id, repo, b"custom raw".as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO revisions VALUES (?1,2,?2,0)",
            params![id, b"custom raw".as_slice()],
        )
        .unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    let snapshot = store.export_snapshot(&RepositoryId(repo.clone())).unwrap();
    assert_eq!(snapshot.records[0].id, id);
    assert_eq!(snapshot.records[0].content.bytes().unwrap(), b"custom raw");
    assert_eq!(snapshot.records[0].clock.values().next(), Some(&2));
    assert_eq!(
        store.used_locators(&RepositoryId(repo), "task").unwrap(),
        vec!["1"]
    );
}

#[test]
fn multi_kind_edit_is_atomic_and_rehome_retains_locator_history() {
    let root = tempdir().unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = empty_authority(&mut store, root.path(), "task");
    store
        .apply_migration(
            &MigrationPlan::capture(repo.clone(), vec!["goals".into()], vec![]).unwrap(),
        )
        .unwrap();
    let task = store
        .mutate(&repo, "task", "old", None, |_| Ok(Some(b"task".to_vec())))
        .unwrap()
        .unwrap();
    store
        .mutate(&repo, "goals", "goals", None, |_| {
            Ok(Some(b"goal".to_vec()))
        })
        .unwrap();
    let before = store.export_snapshot(&repo).unwrap();
    assert!(store
        .mutate_kinds(
            &repo,
            &["task", "goals"],
            None,
            |documents, _| -> Result<()> {
                documents[0].content = b"discard".to_vec();
                anyhow::bail!("command failure")
            }
        )
        .is_err());
    assert_eq!(store.export_snapshot(&repo).unwrap(), before);
    store
        .mutate_kinds(&repo, &["task", "goals"], None, |documents, history| {
            assert!(history["task"].contains(&"old".to_string()));
            documents
                .iter_mut()
                .find(|doc| doc.id == task.id)
                .unwrap()
                .locator = "new".into();
            documents
                .iter_mut()
                .find(|doc| doc.kind == "goals")
                .unwrap()
                .content = b"updated goal".to_vec();
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store.read(&repo, "task", "new").unwrap().unwrap().id,
        task.id
    );
    assert_eq!(
        store.used_locators(&repo, "task").unwrap(),
        vec!["new", "old"]
    );
}

#[test]
fn missing_registered_root_keeps_its_repository_and_documents() {
    let root = tempdir().unwrap();
    let checkout = root.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = empty_authority(&mut store, &checkout, "task");
    store
        .mutate(&repo, "task", "1", None, |_| Ok(Some(b"retained".to_vec())))
        .unwrap();
    std::fs::remove_dir(&checkout).unwrap();
    assert_eq!(store.repository(&checkout).unwrap(), Some(repo.clone()));
    assert_eq!(
        store.authority_for_root(&checkout, "task").unwrap(),
        Some(repo.clone())
    );
    assert_eq!(
        store.read(&repo, "task", "1").unwrap().unwrap().content,
        b"retained"
    );
}

#[test]
fn moved_repository_explicit_rebind_retains_old_alias_and_epoch() {
    let root = tempdir().unwrap();
    let original = root.path().join("original");
    let moved = root.path().join("moved");
    std::fs::create_dir(&original).unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = empty_authority(&mut store, &original, "task");
    let snapshot = store.export_snapshot(&repo).unwrap();
    std::fs::rename(&original, &moved).unwrap();
    assert!(store.repository(&moved).unwrap().is_none());
    store.rebind_repository(&repo, &moved).unwrap();
    assert_eq!(store.repository(&original).unwrap(), Some(repo.clone()));
    assert_eq!(store.repository(&moved).unwrap(), Some(repo.clone()));
    assert_eq!(store.export_snapshot(&repo).unwrap(), snapshot);
}

#[test]
fn reused_git_path_never_silently_joins_old_identity() {
    let root = tempdir().unwrap();
    let checkout = root.path().join("checkout");
    let moved = root.path().join("moved");
    std::fs::create_dir(&checkout).unwrap();
    let git_init = |path: &Path| {
        let output = crate::git_cmd()
            .arg("-C")
            .arg(path)
            .args(["init", "-q"])
            .output()
            .unwrap();
        assert!(output.status.success());
    };
    git_init(&checkout);
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo = store.bind_repository(&checkout).unwrap();
    std::fs::rename(&checkout, &moved).unwrap();
    std::fs::create_dir(&checkout).unwrap();
    git_init(&checkout);
    assert!(store.repository(&checkout).is_err());
    assert!(store.bind_repository(&checkout).is_err());
    store.rebind_repository(&repo, &moved).unwrap();
    assert_eq!(store.repository(&moved).unwrap(), Some(repo));
    assert!(store.repository(&checkout).is_err());
}

#[test]
fn rebind_cannot_take_another_repository_binding() {
    let root = tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let mut store = Store::open(root.path().join("db")).unwrap();
    let repo_a = store.bind_repository(&a).unwrap();
    let repo_b = store.bind_repository(&b).unwrap();
    assert!(store.rebind_repository(&repo_a, &b).is_err());
    assert_eq!(store.repository(&b).unwrap(), Some(repo_b));
}
