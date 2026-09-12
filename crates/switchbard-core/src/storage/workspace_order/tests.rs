use super::*;
use crate::storage::{with_test_database, MigrationPlan, SourceDocument};

#[test]
fn workspace_ordering_stays_local_and_follows_stable_targets_after_rehome_and_rebind() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("alpha");
    std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    let locator = "backlog/tasks/task-1 - First.md";
    std::fs::write(
        root.join(locator),
        "---\nid: TASK-1\ntitle: First\nstatus: To Do\n---\n",
    )
    .unwrap();
    let source = root.join("ordering.yml");
    let raw = "# retain comment\nranked: [alpha:TASK-1, unavailable:TASK-9]\ncustom: {nested: [null, keep]}\n";
    std::fs::write(&source, raw).unwrap();
    let db = temp.path().join("state.sqlite3");
    with_test_database(&db, || {
        let mut store = Store::open(&db).unwrap();
        let repo = store.bind_repository(&root).unwrap();
        let plan = MigrationPlan::capture(
            repo.clone(),
            vec!["task".into()],
            vec![SourceDocument {
                kind: "task".into(),
                locator: locator.into(),
                path: root.join(locator),
            }],
        )
        .unwrap();
        store.apply_migration(&plan).unwrap();
        let before_exchange = store.export_snapshot(&repo).unwrap();
        let ordering = store
            .migrate_workspace_ordering(&source, &[("alpha".into(), root.clone())])
            .unwrap();
        assert_eq!(ordering.content, raw.as_bytes());
        assert_eq!(ordering.entries.len(), 2);
        assert_eq!(
            ordering.entries[0].target.as_ref().unwrap().repository_id,
            repo
        );
        assert_eq!(ordering.entries[1].locator, "unavailable:TASK-9");
        assert!(ordering.entries[1].target.is_none());
        assert_eq!(store.export_snapshot(&repo).unwrap(), before_exchange);
        crate::rehome_task_file(&root.join(locator), "TASK", "2", None).unwrap();
        let moved_root = temp.path().join("renamed");
        std::fs::rename(&root, &moved_root).unwrap();
        store.rebind_repository(&repo, &moved_root).unwrap();
        let sequence = store.change_sequence().unwrap();
        let edited = format!("{raw}# central-only edit\n");
        store
            .replace_workspace_ordering(
                edited.as_bytes(),
                &[("renamed".into(), moved_root.clone())],
                sequence,
            )
            .unwrap();
        assert_eq!(
            store.workspace_ordering().unwrap().unwrap().entries,
            ordering.entries
        );
        let stable = store.workspace_ordering().unwrap().unwrap();
        assert!(store
            .replace_workspace_ordering(raw.as_bytes(), &[], sequence)
            .is_err());
        assert_eq!(store.workspace_ordering().unwrap().unwrap(), stable);
        let hub = crate::find_hub_repo([moved_root.as_path()])
            .unwrap()
            .unwrap();
        let (overlay, warning) = crate::load_ordering_overlay(&hub).unwrap();
        assert!(warning.is_none());
        let loaded = crate::load_backlog_repo(&moved_root).unwrap();
        let ranked =
            crate::triage_entry_from_task(moved_root.clone(), "renamed", &loaded.tasks[0], &loaded);
        let mut other = ranked.clone();
        other.storage_identity = None;
        other.task_id = "TASK-0".into();
        other.priority = crate::TriagePriority::High;
        let sorted = crate::triage_rank(&[other, ranked], &overlay);
        assert_eq!(sorted[0].task_id, "TASK-2");
        assert_eq!(
            std::fs::read_to_string(moved_root.join("ordering.yml")).unwrap(),
            raw
        );
        assert_eq!(
            store.workspace_ordering().unwrap().unwrap().entries,
            ordering.entries
        );
    });
}

#[test]
fn workspace_ordering_refuses_malformed_or_replaced_sources_without_effects() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("ordering.yml");
    let db = temp.path().join("state.sqlite3");
    let mut store = Store::open(&db).unwrap();
    std::fs::write(&source, "ranked: broken").unwrap();
    let before = store.change_sequence().unwrap();
    assert!(store.migrate_workspace_ordering(&source, &[]).is_err());
    assert_eq!(store.change_sequence().unwrap(), before);
    assert!(store.workspace_ordering().unwrap().is_none());
    std::fs::write(&source, "ranked: [unknown:TASK-1]\n").unwrap();
    let digest = workspace_ordering_source_digest(&source).unwrap();
    std::fs::write(&source, "ranked: [other:TASK-1]\n").unwrap();
    assert!(store
        .migrate_workspace_ordering_checked(&source, &[], &digest)
        .is_err());
    assert!(store.workspace_ordering().unwrap().is_none());
    std::fs::write(&source, "ranked: [unknown:TASK-1]\n").unwrap();
    let original = store.migrate_workspace_ordering(&source, &[]).unwrap();
    std::fs::write(&source, "ranked: [unknown:TASK-2]\n").unwrap();
    assert!(store.migrate_workspace_ordering(&source, &[]).is_err());
    assert_eq!(store.workspace_ordering().unwrap().unwrap(), original);
    with_test_database(&db, || {
        store
            .connection
            .execute("UPDATE workspace_ordering SET entries_json='broken'", [])
            .unwrap();
        assert!(crate::find_hub_repo([root.as_path()]).is_err());
        assert!(crate::load_ordering_overlay(&root).is_err());
    });
}
