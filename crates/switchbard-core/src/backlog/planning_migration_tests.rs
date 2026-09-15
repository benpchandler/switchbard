use super::*;
use crate::storage::{with_test_database, MigrationPlan, SourceDocument};

const CONFIG_TEXT: &str = "# config comment\nproject_name: fixture\nstatuses: [\"To Do\", \"Icebox\", \"Done\", \"In Progress\", \"Mystery\"] # keep inline\ncustom: preserved\ndefault_status: To Do # keep default note\n";

fn fixture(root: &Path, db: &Path) {
    std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    std::fs::create_dir_all(root.join("backlog/completed")).unwrap();
    std::fs::create_dir_all(root.join("backlog/archive/tasks")).unwrap();
    let records = [
        ("tasks/task-1.md", "TASK-1", "To Do", ""),
        ("tasks/task-2.md", "TASK-2", "Icebox", ""),
        (
            "tasks/task-3.md",
            "TASK-3",
            "Mystery",
            "planning: Considering\n",
        ),
        ("completed/task-4.md", "TASK-4", "Done", ""),
        ("archive/tasks/task-5.md", "TASK-5", "Icebox", ""),
    ];
    let mut sources = Vec::new();
    for (relative, id, status, planning) in records {
        let locator = format!("backlog/{relative}");
        std::fs::write(root.join(&locator), format!("---\nid: {id}\ntitle: Title {id}\nstatus: {status}\n{planning}labels: [keep]\nupdated_date: 2020-01-01\n---\n\n## Acceptance Criteria\n<!-- AC:BEGIN -->\n- [ ] #1 Original\n<!-- AC:END -->\n")).unwrap();
        sources.push(SourceDocument {
            kind: "task".into(),
            path: root.join(&locator),
            locator,
        });
    }
    for (kind, locator, text) in [
        ("config", CONFIG, CONFIG_TEXT),
        (
            "ranking",
            RANKING,
            "# ranking\nexpedite: [TASK-1]\nroot_tasks: [TASK-2, TASK-1]\n",
        ),
    ] {
        std::fs::write(root.join(locator), text).unwrap();
        sources.push(SourceDocument {
            kind: kind.into(),
            locator: locator.into(),
            path: root.join(locator),
        });
    }
    let mut store = Store::open(db).unwrap();
    let repo = store.bind_repository(root).unwrap();
    let plan =
        MigrationPlan::capture(repo, KINDS.iter().map(|k| (*k).into()).collect(), sources).unwrap();
    store.apply_migration(&plan).unwrap();
}

#[test]
fn mixed_migration_preserves_history_criteria_and_explicit_planning_and_replays() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        fixture(&root, &db);
        let preview = prepare_planning_migration(&root).unwrap();
        assert_eq!(preview.unknown_statuses, vec!["Mystery"]);
        assert_eq!(preview.planned_order, vec!["TASK-1"]);
        let restored: PlanningMigrationPreview =
            serde_json::from_str(&serde_json::to_string(&preview).unwrap()).unwrap();
        assert_eq!(preview, restored);
        let receipt =
            apply_planning_migration(&root, &preview, &dir.path().join("backups")).unwrap();
        assert!(receipt.backup_path.as_ref().unwrap().exists());
        assert!(receipt.receipt_path.as_ref().unwrap().exists());
        for before in &receipt.before {
            let after = receipt.after.iter().find(|d| d.id == before.id).unwrap();
            assert_eq!(before.locator, after.locator);
            if before.kind == "task" {
                assert!(std::str::from_utf8(&after.content)
                    .unwrap()
                    .contains("updated_date: 2020-01-01"));
                assert_eq!(
                    std::str::from_utf8(&before.content)
                        .unwrap()
                        .split_once("---\n\n")
                        .unwrap()
                        .1,
                    std::str::from_utf8(&after.content)
                        .unwrap()
                        .split_once("---\n\n")
                        .unwrap()
                        .1
                );
            }
        }
        let text = |id: &str| {
            String::from_utf8(
                receipt
                    .after
                    .iter()
                    .find(|d| d.locator == id)
                    .unwrap()
                    .content
                    .clone(),
            )
            .unwrap()
        };
        assert!(text("backlog/tasks/task-1.md").contains("status: Not started"));
        assert!(text("backlog/tasks/task-2.md").contains("planning: Considering"));
        assert!(text("backlog/tasks/task-3.md").contains("status: Mystery\nplanning: Considering"));
        assert!(text("backlog/completed/task-4.md").contains("status: Done"));
        assert!(text("backlog/archive/tasks/task-5.md").contains("status: Icebox"));
        assert!(text(CONFIG).contains(" # keep inline\ncustom: preserved\n"));
        assert!(!text(CONFIG).contains("Icebox"));
        assert!(text(CONFIG).contains("default_status: Not started # keep default note"));
        assert!(text(RANKING).contains("expedite: [TASK-1]\nroot_tasks: [TASK-2, TASK-1]"));
        assert!(
            apply_planning_migration(&root, &preview, &dir.path().join("backups"))
                .unwrap()
                .already_applied
        );
        assert!(prepare_planning_migration(&root).unwrap().already_migrated);
        assert_eq!(
            std::fs::read_to_string(root.join(CONFIG)).unwrap(),
            CONFIG_TEXT
        );
    });
}

#[test]
fn stale_config_refuses_every_task_write() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        fixture(&root, &db);
        let preview = prepare_planning_migration(&root).unwrap();
        let (mut store, repo) = central(&root).unwrap();
        store
            .mutate(&repo, "config", CONFIG, None, |doc| {
                let mut text = doc.unwrap().content.clone();
                text.extend_from_slice(b"# concurrent edit\n");
                Ok(Some(text))
            })
            .unwrap();
        assert!(
            apply_planning_migration(&root, &preview, &dir.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("stale")
        );
        let tasks = store.list(&repo, "task").unwrap();
        assert_eq!(
            tasks,
            preview
                .expected
                .into_iter()
                .filter(|d| d.kind == "task")
                .collect::<Vec<_>>()
        );
        assert!(!dir.path().join("backups").exists());
    });
}

#[test]
fn legacy_and_unknown_planning_refuse_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    let db = dir.path().join("state.sqlite3");
    std::fs::create_dir_all(&root).unwrap();
    with_test_database(&db, || {
        assert!(prepare_planning_migration(&root).is_err());
        fixture(&root, &db);
        let (mut store, repo) = central(&root).unwrap();
        store
            .mutate(&repo, "task", "backlog/tasks/task-1.md", None, |doc| {
                let text = String::from_utf8(doc.unwrap().content.clone())
                    .unwrap()
                    .replace("status: To Do", "status: To Do\nplanning: Surprise");
                Ok(Some(text.into_bytes()))
            })
            .unwrap();
        assert!(prepare_planning_migration(&root)
            .unwrap_err()
            .to_string()
            .contains("unknown planning"));
    });
}

#[test]
fn block_status_config_preserves_surrounding_comments() {
    let input = "# top\nstatuses:\n  - To Do\n  # historical note\n  - Done\n\nother: preserved\n";
    let out = replace_statuses(input, &["Not started".into(), "Done".into()]).unwrap();
    assert_eq!(
        out,
        "# top\nstatuses: [\"Not started\",\"Done\"]\n  # historical note\n\nother: preserved\n"
    );
}

#[test]
fn altered_preview_and_task_revision_are_rejected_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        fixture(&root, &db);
        let preview = prepare_planning_migration(&root).unwrap();
        let mut altered = preview.clone();
        altered
            .proposed
            .iter_mut()
            .find(|d| d.kind == "task")
            .unwrap()
            .content
            .extend_from_slice(b"unauthorized extra prose\n");
        assert!(
            apply_planning_migration(&root, &altered, &dir.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("differs")
        );
        let (mut store, repo) = central(&root).unwrap();
        assert_eq!(documents(&store, &repo).unwrap(), preview.expected);
        store
            .mutate(&repo, "task", "backlog/tasks/task-1.md", None, |doc| {
                let mut content = doc.unwrap().content.clone();
                content.extend_from_slice(b"concurrent note\n");
                Ok(Some(content))
            })
            .unwrap();
        assert!(
            apply_planning_migration(&root, &preview, &dir.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("stale")
        );
        assert!(!dir.path().join("backups").exists());
    });
}

#[test]
fn inline_status_comments_and_quoted_brackets_survive() {
    for input in [
        "statuses: [\"To Do\"] # migration note ]\nother: retained\n",
        "statuses: [\"To Do\", \"wait ]\", 'it''s ] pending'] # keep ]\nother: retained\n",
    ] {
        let output = replace_statuses(input, &["Not started".into()]).unwrap();
        assert_eq!(
            output.split_once(" #").unwrap().1,
            input.split_once(" #").unwrap().1
        );
        let value: serde_yaml::Value = serde_yaml::from_str(&output).unwrap();
        assert_eq!(value["statuses"][0].as_str(), Some("Not started"));
    }
}

#[test]
fn changed_epoch_refuses_before_apply_or_already_applied_shortcut() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        fixture(&root, &db);
        let preview = prepare_planning_migration(&root).unwrap();
        let mut wrong_epoch = preview.clone();
        wrong_epoch.epoch_id = uuid::Uuid::new_v4().to_string();
        assert!(
            apply_planning_migration(&root, &wrong_epoch, &dir.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("epoch changed")
        );
        assert!(!dir.path().join("backups").exists());
        apply_planning_migration(&root, &preview, &dir.path().join("backups")).unwrap();
        assert!(
            apply_planning_migration(&root, &wrong_epoch, &dir.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("epoch changed")
        );
    });
}
