use super::*;
use crate::backlog::*;
use crate::storage::{with_test_database, MigrationPlan, SourceDocument};
use std::fs;

const RAW: &str = "---\nid: TASK-7\ntitle: Original\nstatus: To Do\nlabels: []\ncustom:\n  nested: [one, {two: null}]\n# retain me\n---\n\n## Acceptance Criteria\n<!-- AC:BEGIN -->\n- [ ] #1 Existing\n<!-- AC:END -->\n\n## Unrecognized\nUntouched café.\n";

fn migrate(root: &Path, db: &Path, sources: &[&str]) {
    let mut store = Store::open(db).unwrap();
    let repo = store.bind_repository(root).unwrap();
    let sources = sources
        .iter()
        .map(|locator| SourceDocument {
            kind: "task".into(),
            locator: (*locator).into(),
            path: root.join(locator),
        })
        .collect();
    let plan = MigrationPlan::capture(repo, vec!["task".into()], sources).unwrap();
    store.apply_migration(&plan).unwrap();
}

#[test]
fn task_cutover_preserves_custom_content_and_projection_and_multi_field_atomicity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    let locator = "backlog/tasks/task-7 - Original.md";
    fs::write(root.join(locator), RAW).unwrap();
    let db = temp.path().join("state.sqlite3");
    with_test_database(&db, || {
        let before = load_backlog_repo(&root).unwrap();
        migrate(&root, &db, &[locator]);
        let after = load_backlog_repo(&root).unwrap();
        assert_eq!(
            before.tasks,
            after
                .tasks
                .iter()
                .cloned()
                .map(|mut task| {
                    task.storage_identity = None;
                    task
                })
                .collect::<Vec<_>>()
        );
        let store = Store::open(&db).unwrap();
        let repo = store.repository(&root).unwrap().unwrap();
        let original = store.read(&repo, "task", locator).unwrap().unwrap();
        assert_eq!(
            after.tasks[0].storage_identity.as_ref().unwrap().record_id,
            original.id
        );
        let invalid = BacklogTaskPatch {
            title: Some("Should roll back".into()),
            priority: Some("invalid\nline".into()),
            ..Default::default()
        };
        assert!(edit_backlog_task(&root, "7", &invalid).is_err());
        assert_eq!(
            store.read(&repo, "task", locator).unwrap().unwrap(),
            original
        );
        let patch = BacklogTaskPatch {
            title: Some("Changed".into()),
            status: Some("In Progress".into()),
            ..Default::default()
        };
        edit_backlog_task(&root, "7", &patch).unwrap();
        let updated = store.read(&repo, "task", locator).unwrap().unwrap();
        assert_eq!(updated.revision, original.revision + 1);
        assert_eq!(updated.id, original.id);
        let content = String::from_utf8(updated.content.clone()).unwrap();
        assert!(content.contains("custom:\n  nested: [one, {two: null}]\n# retain me\n"));
        assert!(content.ends_with("## Unrecognized\nUntouched café.\n"));
        assert!(content.contains("title: Changed"));
        edit_backlog_task(&root, "7", &patch).unwrap();
        assert_eq!(
            store.read(&repo, "task", locator).unwrap().unwrap(),
            updated
        );
        set_backlog_acceptance_checked(&root, "7", 1, true).unwrap();
        assert!(read(&root.join(locator))
            .unwrap()
            .contains("- [x] #1 Existing"));
        assert_eq!(fs::read_to_string(root.join(locator)).unwrap(), RAW);
    });
}

#[test]
fn task_cutover_lifecycle_moves_keep_identity_and_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    let locator = "backlog/tasks/task-7 - Original.md";
    fs::write(root.join(locator), RAW).unwrap();
    let db = temp.path().join("state.sqlite3");
    with_test_database(&db, || {
        migrate(&root, &db, &[locator]);
        let store = Store::open(&db).unwrap();
        let repo = store.repository(&root).unwrap().unwrap();
        let original = store.read(&repo, "task", locator).unwrap().unwrap();
        archive_backlog_task(&root, "7").unwrap();
        let after = load_backlog_repo(&root).unwrap();
        assert_eq!(after.tasks[0].source, BacklogTaskSource::Archived);
        let archived = store
            .read(&repo, "task", "backlog/archive/tasks/task-7 - Original.md")
            .unwrap()
            .unwrap();
        assert_eq!(original.id, archived.id);
        assert_eq!(original.content, archived.content);
        assert_eq!(fs::read_to_string(root.join(locator)).unwrap(), RAW);
        assert!(!root.join("backlog/archive").exists());
        edit_backlog_task(
            &root,
            "7",
            &BacklogTaskPatch {
                title: Some("Archived edit".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            load_backlog_repo(&root).unwrap().tasks[0].title,
            "Archived edit"
        );
    });
}

fn new_task(title: &str) -> NewBacklogTask {
    NewBacklogTask {
        title: title.into(),
        description: String::new(),
        status: String::new(),
        priority: String::new(),
        acceptance_criteria: vec![],
        parent: None,
        labels: vec![],
        assignees: vec![],
        project: None,
        dependencies: vec![],
        due_date: None,
    }
}

#[test]
fn task_cutover_concurrent_creates_are_distinct_without_markdown_writes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    let db = temp.path().join("state.sqlite3");
    migrate(&root, &db, &[]);
    let threads: Vec<_> = (0..4)
        .map(|index| {
            let root = root.clone();
            let db = db.clone();
            std::thread::spawn(move || {
                with_test_database(&db, || {
                    create_task_allocating_id(&root, &new_task(&format!("New {index}")))
                        .unwrap()
                        .0
                })
            })
        })
        .collect();
    let mut ids: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 4);
    assert!(!root.join("backlog/tasks").exists());
    with_test_database(&db, || {
        assert_eq!(load_backlog_repo(&root).unwrap().tasks.len(), 4)
    });
}

#[test]
fn task_cutover_reparent_commits_dependencies_and_aggregates_together() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    let originals = [
        ("task", "backlog/tasks/task-7 - Original.md", RAW.to_owned()),
        ("task", "backlog/tasks/task-8 - Parent.md", RAW.replace("TASK-7", "TASK-8")),
        ("task", "backlog/tasks/task-9 - Dependent.md", RAW.replace("TASK-7", "TASK-9").replace("labels: []", "labels: []\ndependencies: [TASK-7]")),
        ("goals", "backlog/goals.yml", "goals:\n  - name: Finish\n    measure: tasks\n    inputs:\n      tasks: [TASK-7]\n    weeks: {}\n".to_owned()),
        ("ranking", "backlog/ranking.yml", "root_tasks: [TASK-7]\ncustom: {retain: yes}\n".to_owned()),
    ];
    for (_, locator, text) in &originals {
        fs::write(root.join(locator), text).unwrap();
    }
    let db = temp.path().join("state.sqlite3");
    with_test_database(&db, || {
        let mut store = Store::open(&db).unwrap();
        let repo = store.bind_repository(&root).unwrap();
        let sources = originals
            .iter()
            .map(|(kind, locator, _)| SourceDocument {
                kind: (*kind).into(),
                locator: (*locator).into(),
                path: root.join(locator),
            })
            .collect();
        let plan = MigrationPlan::capture(
            repo.clone(),
            vec!["task".into(), "goals".into(), "ranking".into()],
            sources,
        )
        .unwrap();
        store.apply_migration(&plan).unwrap();
        let old_id = store
            .read(&repo, "task", originals[0].1)
            .unwrap()
            .unwrap()
            .id;
        assert_eq!(
            move_backlog_task(&root, "7", Some("8")).unwrap(),
            Some("TASK-8.1".into())
        );
        let moved = store
            .read(&repo, "task", "backlog/tasks/task-8.1 - Original.md")
            .unwrap()
            .unwrap();
        assert_eq!(moved.id, old_id);
        assert!(String::from_utf8(moved.content)
            .unwrap()
            .contains("parent_task_id: TASK-8"));
        assert!(read(&root.join(originals[2].1))
            .unwrap()
            .contains("- TASK-8.1"));
        for (kind, locator, _) in &originals[3..] {
            let raw = String::from_utf8(store.read(&repo, kind, locator).unwrap().unwrap().content)
                .unwrap();
            if *kind == "goals" {
                assert!(raw.contains("TASK-8.1"), "{raw}");
            } else {
                assert!(
                    !raw.contains("TASK-7"),
                    "old sibling rank must be removed: {raw}"
                );
                assert!(raw.contains("custom: {retain: yes}"));
            }
        }
        for (_, locator, text) in &originals {
            assert_eq!(fs::read_to_string(root.join(locator)).unwrap(), *text);
        }
        // Fail after task drafts have changed: malformed aggregate must roll back all kinds.
        store
            .mutate(&repo, "goals", "backlog/goals.yml", None, |_| {
                Ok(Some(b"goals: [broken".to_vec()))
            })
            .unwrap();
        let before = store.list(&repo, "task").unwrap();
        let rank_before = store.list(&repo, "ranking").unwrap();
        assert!(move_backlog_task(&root, "8.1", None).is_err());
        assert_eq!(store.list(&repo, "task").unwrap(), before);
        assert_eq!(store.list(&repo, "ranking").unwrap(), rank_before);
    });
}

#[test]
fn task_cutover_mixed_reparent_refuses_and_old_ids_are_never_reallocated() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    let locator = "backlog/tasks/task-7 - Original.md";
    fs::write(root.join(locator), RAW).unwrap();
    let db = temp.path().join("state.sqlite3");
    with_test_database(&db, || {
        migrate(&root, &db, &[locator]);
        assert!(move_backlog_task(&root, "7", None)
            .unwrap_err()
            .to_string()
            .contains("partially migrated"));
        rehome_task_file(&root.join(locator), "TASK", "1", None).unwrap();
        assert_eq!(next_task_id(&root).unwrap(), 8);
        assert_eq!(
            create_task_allocating_id(&root, &new_task("next"))
                .unwrap()
                .0,
            "8"
        );
        assert_eq!(fs::read_to_string(root.join(locator)).unwrap(), RAW);
    });
}

#[test]
fn task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    let locators = [
        "backlog/tasks/task-1 - Active.md",
        "backlog/completed/task-2 - Completed.md",
        "backlog/drafts/task-3 - Draft.md",
        "backlog/archive/tasks/task-4 - Archive.md",
    ];
    for (index, locator) in locators.iter().enumerate() {
        let path = root.join(locator);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, RAW.replace("TASK-7", &format!("TASK-{}", index + 1))).unwrap();
    }
    let db = temp.path().join("state.sqlite3");
    with_test_database(&db, || {
        let before = load_backlog_repo(&root).unwrap();
        migrate(&root, &db, &locators);
        // Disposable fixtures only: prove the DB has no implicit markdown dependency.
        fs::remove_dir_all(root.join("backlog")).unwrap();
        let after = load_backlog_repo(&root).unwrap();
        assert!(after
            .tasks
            .iter()
            .all(|task| task.storage_identity.is_some()));
        let mut projected = after.tasks.clone();
        for task in &mut projected {
            task.storage_identity = None;
        }
        assert_eq!(before.tasks, projected);
        for id in 1..=4 {
            set_backlog_label(&root, &id.to_string(), "central", true).unwrap();
        }
        assert!(load_backlog_repo(&root)
            .unwrap()
            .tasks
            .iter()
            .all(|task| task.labels.contains(&"central".to_owned())));
        edit_backlog_task(
            &root,
            "1",
            &BacklogTaskPatch {
                status: Some("Done".into()),
                ..Default::default()
            },
        )
        .unwrap();
        complete_backlog_task(&root, "1").unwrap();
        assert_eq!(
            load_backlog_repo(&root)
                .unwrap()
                .tasks
                .iter()
                .find(|task| task.id == "TASK-1")
                .unwrap()
                .source,
            BacklogTaskSource::Completed
        );
        assert!(!root.join("backlog").exists());
    });
}
