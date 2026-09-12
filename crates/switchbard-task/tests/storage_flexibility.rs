//! A compatible older client preserves opaque payloads while editing supported peers.
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use switchbard_core::storage::{ExchangeContent, MigrationPlan, SourceDocument, Store};

fn sb(root: &Path, db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", db)
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
fn succeeds(result: Output) -> String {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn schema_version(database: &Path) -> [u8; 4] {
    fs::read(database).unwrap()[60..64].try_into().unwrap()
}

#[test]
fn supported_task_and_custom_content_edit_preserve_opaque_kind_and_newer_task() {
    let fixture = tempfile::tempdir().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let c = fixture.path().join("c");
    for root in [&a, &b, &c] {
        fs::create_dir(root).unwrap();
    }
    let a_db = fixture.path().join("a.db");
    let b_db = fixture.path().join("b.db");
    let mut origin = Store::open(&a_db).unwrap();
    let repo = origin.bind_repository(&a).unwrap();
    let mut sources = Vec::new();
    for id in 1..=2 {
        let path = a.join(format!("{id}.md"));
        fs::write(&path,format!("---\nid: TASK-{id}\ntitle: Task {id}\nstatus: To Do\ncustom: {{list: [null, '', [], {{}}]}}\n---\n\n## Custom\nRetain me.\n")).unwrap();
        sources.push(SourceDocument {
            kind: "task".into(),
            locator: format!("backlog/tasks/task-{id} - Task {id}.md"),
            path,
        });
    }
    origin
        .apply_migration_checked(
            &MigrationPlan::capture(repo.clone(), vec!["task".into()], sources).unwrap(),
            switchbard_core::backlog::validate_storage_snapshot,
        )
        .unwrap();
    let mut incoming = origin.export_snapshot(&repo).unwrap();
    let task_two = incoming
        .records
        .iter_mut()
        .find(|record| record.locator.contains("task-2 "))
        .unwrap();
    task_two.content_version = 2;
    task_two.content = ExchangeContent::from_bytes(&[0xff, 0x10, 0x00]);
    let mut unknown = task_two.clone();
    unknown.id = "00000000-0000-4000-8000-000000000017".into();
    unknown.kind = "future-kind".into();
    unknown.locator = "future-record".into();
    unknown.content_version = 1;
    incoming.records.push(unknown);
    incoming.kinds.push("future-kind".into());
    incoming.refresh_digest().unwrap();
    let mut local = Store::open(&b_db).unwrap();
    local
        .bind_exchange_checked(
            &b,
            &incoming,
            switchbard_core::backlog::validate_storage_snapshot,
        )
        .unwrap();
    let opaque_before: Vec<_> = local
        .export_snapshot(&repo)
        .unwrap()
        .records
        .into_iter()
        .filter(|record| {
            !switchbard_core::storage::content_is_understood(&record.kind, record.content_version)
        })
        .collect();
    let list = sb(&b, &b_db, &["list"]);
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stderr).contains("unsupported task content version 2"));
    assert!(String::from_utf8_lossy(&list.stdout).contains("TASK-1"));
    assert!(!String::from_utf8_lossy(&list.stdout).contains("TASK-2"));
    succeeds(sb(
        &b,
        &b_db,
        &["edit", "1", "--description", "Supported edit"],
    ));
    let before_version = schema_version(&b_db);
    let current = local
        .list(&repo, "task")
        .unwrap()
        .into_iter()
        .find(|record| record.content_version == 1)
        .unwrap();
    let text = String::from_utf8(current.content.clone()).unwrap().replace(
        "status: To Do",
        "status: To Do\nnew_custom: {typed: [true, null, 23]}",
    );
    let input = fixture.path().join("custom.md");
    fs::write(&input, &text).unwrap();
    succeeds(sb(
        &b,
        &b_db,
        &[
            "storage",
            "document",
            "--id",
            &current.id,
            "--write-from",
            input.to_str().unwrap(),
            "--expected-revision",
            &current.revision.to_string(),
        ],
    ));
    assert_eq!(schema_version(&b_db), before_version);
    let after = local.export_snapshot(&repo).unwrap();
    assert_eq!(
        after
            .records
            .iter()
            .filter(|record| !switchbard_core::storage::content_is_understood(
                &record.kind,
                record.content_version
            ))
            .cloned()
            .collect::<Vec<_>>(),
        opaque_before
    );
    assert_eq!(
        local
            .read(&repo, "task", &current.locator)
            .unwrap()
            .unwrap()
            .content,
        text.as_bytes()
    );
    let fingerprint = local.recovery_fingerprint().unwrap();
    for record in &opaque_before {
        let document = local
            .read(&repo, &record.kind, &record.locator)
            .unwrap()
            .unwrap();
        let result = sb(
            &b,
            &b_db,
            &[
                "storage",
                "document",
                "--id",
                &record.id,
                "--write-from",
                input.to_str().unwrap(),
                "--expected-revision",
                &document.revision.to_string(),
            ],
        );
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported"));
    }
    assert!(!sb(
        &b,
        &b_db,
        &["edit", "2", "--description", "Unsupported edit"]
    )
    .status
    .success());
    assert!(!sb(&b, &b_db, &["create", "Unsafe allocation"])
        .status
        .success());
    assert_eq!(local.recovery_fingerprint().unwrap(), fingerprint);
    let export = fixture.path().join("exchange.json");
    succeeds(sb(
        &b,
        &b_db,
        &["storage", "export", "--file", export.to_str().unwrap()],
    ));
    let parsed =
        switchbard_core::storage::ExchangeSnapshot::parse(&fs::read(export).unwrap()).unwrap();
    let mut restored = Store::open(fixture.path().join("c.db")).unwrap();
    restored
        .bind_exchange_checked(
            &c,
            &parsed,
            switchbard_core::backlog::validate_storage_snapshot,
        )
        .unwrap();
    assert_eq!(restored.export_snapshot(&repo).unwrap(), after);
    assert!(!b.join("backlog").exists());
}
