//! Two real CLI peers exercise reviewable conflict choices without touching user state.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use switchbard_core::storage::{
    ExchangeContent, ExchangeSnapshot, MigrationPlan, SourceDocument, Store,
};

struct Peers {
    _dir: tempfile::TempDir,
    a: PathBuf,
    b: PathBuf,
    a_db: PathBuf,
    b_db: PathBuf,
    source: PathBuf,
    original: Vec<u8>,
}
fn sb(root: &Path, db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", db)
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn snapshot(root: &Path, db: &Path) -> ExchangeSnapshot {
    let store = Store::open_existing(db).unwrap().unwrap();
    store
        .export_snapshot(&store.repository(root).unwrap().unwrap())
        .unwrap()
}
fn fingerprint(db: &Path) -> String {
    Store::open_existing(db)
        .unwrap()
        .unwrap()
        .recovery_fingerprint()
        .unwrap()
}
fn export(root: &Path, db: &Path) -> PathBuf {
    successful(sb(root, db, &["storage", "export"]));
    root.join(".switchbard/tasks.json")
}
fn preview(root: &Path, db: &Path, file: &Path, bind: bool) -> serde_json::Value {
    let mut args = vec!["storage", "import", "--file", file.to_str().unwrap()];
    if bind {
        args.push("--bind");
    }
    serde_json::from_str(&successful(sb(root, db, &args))).unwrap()
}
fn apply(
    root: &Path,
    db: &Path,
    file: &Path,
    preview: &serde_json::Value,
    bind: bool,
    resolution: Option<&Path>,
) -> Output {
    let sequence = preview["expected_sequence"].as_u64().unwrap().to_string();
    let mut args = vec![
        "storage",
        "import",
        "--file",
        file.to_str().unwrap(),
        "--apply",
        "--snapshot-digest",
        preview["incoming_digest"].as_str().unwrap(),
        "--expected-sequence",
        &sequence,
    ];
    if bind {
        args.push("--bind");
    }
    if let Some(path) = resolution {
        args.extend(["--resolution-file", path.to_str().unwrap()]);
    }
    sb(root, db, &args)
}
fn peers() -> Peers {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let a_db = dir.path().join("a.db");
    let b_db = dir.path().join("b.db");
    fs::create_dir_all(a.join("backlog/tasks")).unwrap();
    fs::create_dir(&b).unwrap();
    let source = a.join("backlog/tasks/task-1 - Original.md");
    let original = b"---\nid: TASK-1\ntitle: Original\nstatus: To Do\ncustom: {nested: [null, '', {}, []]}\n---\n\n## Custom\nKeep original exactly.\n".to_vec();
    fs::write(&source, &original).unwrap();
    let mut store = Store::open(&a_db).unwrap();
    let repo = store.bind_repository(&a).unwrap();
    store
        .apply_migration_checked(
            &MigrationPlan::capture(
                repo,
                vec!["task".into()],
                vec![SourceDocument {
                    kind: "task".into(),
                    locator: "backlog/tasks/task-1 - Original.md".into(),
                    path: source.clone(),
                }],
            )
            .unwrap(),
            switchbard_core::backlog::validate_storage_snapshot,
        )
        .unwrap();
    drop(store);
    let file = export(&a, &a_db);
    let plan = preview(&b, &b_db, &file, true);
    successful(apply(&b, &b_db, &file, &plan, true, None));
    Peers {
        _dir: dir,
        a,
        b,
        a_db,
        b_db,
        source,
        original,
    }
}
fn resolution_file(
    peers: &Peers,
    plan: &serde_json::Value,
    entries: Vec<serde_json::Value>,
) -> PathBuf {
    let file = peers._dir.path().join("resolution.json");
    fs::write(&file,serde_json::to_vec_pretty(&serde_json::json!({"version":1,"incoming_digest":plan["incoming_digest"],"expected_sequence":plan["expected_sequence"],"resolutions":entries})).unwrap()).unwrap();
    file
}

#[test]
fn offline_allocated_id_collision_relocates_explicitly_then_peers_converge() {
    let p = peers();
    successful(sb(
        &p.a,
        &p.a_db,
        &["create", "Concurrent", "--description", "Created by A"],
    ));
    successful(sb(
        &p.b,
        &p.b_db,
        &["create", "Concurrent", "--description", "Created by B"],
    ));
    let incoming = snapshot(&p.b, &p.b_db);
    let created = incoming
        .records
        .iter()
        .find(|record| record.locator.contains("task-2 "))
        .unwrap();
    let file = export(&p.b, &p.b_db);
    let plan = preview(&p.a, &p.a_db, &file, false);
    assert_eq!(plan["conflicts"].as_array().unwrap().len(), 1);
    let before = fingerprint(&p.a_db);
    assert!(!apply(&p.a, &p.a_db, &file, &plan, false, None)
        .status
        .success());
    assert_eq!(fingerprint(&p.a_db), before);
    let content = String::from_utf8(created.content.bytes().unwrap())
        .unwrap()
        .replace("TASK-2", "TASK-3");
    let resolution = resolution_file(
        &p,
        &plan,
        vec![
            serde_json::json!({"record_id":created.id,"choice":"relocate-incoming","locator":created.locator.replace("task-2 ","task-3 "),"content":ExchangeContent::from_bytes(content.as_bytes())}),
        ],
    );
    successful(apply(&p.a, &p.a_db, &file, &plan, false, Some(&resolution)));
    let file = export(&p.a, &p.a_db);
    let plan = preview(&p.b, &p.b_db, &file, false);
    successful(apply(&p.b, &p.b_db, &file, &plan, false, None));
    assert_eq!(snapshot(&p.a, &p.a_db), snapshot(&p.b, &p.b_db));
    assert_eq!(snapshot(&p.a, &p.a_db).records.len(), 3);
    assert_eq!(fs::read(&p.source).unwrap(), p.original);
    assert!(!p.b.join("backlog").exists());
}

#[test]
fn custom_resolution_is_checked_and_duplicate_or_stale_choices_have_no_effect() {
    let p = peers();
    successful(sb(
        &p.a,
        &p.a_db,
        &["edit", "1", "--description", "A changed"],
    ));
    successful(sb(
        &p.b,
        &p.b_db,
        &["edit", "1", "--description", "B changed"],
    ));
    let file = export(&p.b, &p.b_db);
    let plan = preview(&p.a, &p.a_db, &file, false);
    let record = snapshot(&p.a, &p.a_db).records[0].clone();
    let content = String::from_utf8(record.content.bytes().unwrap())
        .unwrap()
        .replace("A changed", "A and B resolved");
    let entry = serde_json::json!({"record_id":record.id,"choice":"custom","content":ExchangeContent::from_bytes(content.as_bytes()),"deleted":false});
    let before = fingerprint(&p.a_db);
    let duplicate = resolution_file(&p, &plan, vec![entry.clone(), entry.clone()]);
    assert!(!apply(&p.a, &p.a_db, &file, &plan, false, Some(&duplicate))
        .status
        .success());
    assert_eq!(fingerprint(&p.a_db), before);
    let invalid = resolution_file(
        &p,
        &plan,
        vec![
            serde_json::json!({"record_id":record.id,"choice":"custom","content":ExchangeContent::from_bytes(b"---\nid: [broken\n"),"deleted":false}),
        ],
    );
    assert!(!apply(&p.a, &p.a_db, &file, &plan, false, Some(&invalid))
        .status
        .success());
    assert_eq!(fingerprint(&p.a_db), before);
    let mut stale_plan = plan.clone();
    stale_plan["expected_sequence"] = serde_json::json!(0);
    let stale = resolution_file(&p, &stale_plan, vec![entry.clone()]);
    assert!(!apply(&p.a, &p.a_db, &file, &plan, false, Some(&stale))
        .status
        .success());
    assert_eq!(fingerprint(&p.a_db), before);
    let resolution = resolution_file(&p, &plan, vec![entry]);
    let sequence = plan["expected_sequence"].as_u64().unwrap().to_string();
    let local = format!("{}=local", record.id);
    let duplicate_flag = sb(
        &p.a,
        &p.a_db,
        &[
            "storage",
            "import",
            "--file",
            file.to_str().unwrap(),
            "--apply",
            "--snapshot-digest",
            plan["incoming_digest"].as_str().unwrap(),
            "--expected-sequence",
            &sequence,
            "--resolve",
            &local,
            "--resolution-file",
            resolution.to_str().unwrap(),
        ],
    );
    assert!(!duplicate_flag.status.success());
    assert!(String::from_utf8_lossy(&duplicate_flag.stderr).contains("duplicate resolution"));
    assert_eq!(fingerprint(&p.a_db), before);
    successful(apply(&p.a, &p.a_db, &file, &plan, false, Some(&resolution)));
    let file = export(&p.a, &p.a_db);
    let plan = preview(&p.b, &p.b_db, &file, false);
    successful(apply(&p.b, &p.b_db, &file, &plan, false, None));
    assert_eq!(snapshot(&p.a, &p.a_db), snapshot(&p.b, &p.b_db));
    assert!(
        String::from_utf8(snapshot(&p.b, &p.b_db).records[0].content.bytes().unwrap())
            .unwrap()
            .contains("Keep original exactly.")
    );
    assert_eq!(fs::read(&p.source).unwrap(), p.original);
}

#[test]
fn newly_activated_kind_in_bound_repo_cannot_hide_divergent_legacy_content() {
    let p = peers();
    let relative = "backlog/projects/Shared.md";
    fs::create_dir_all(p.a.join("backlog/projects")).unwrap();
    fs::create_dir_all(p.b.join("backlog/projects")).unwrap();
    fs::write(
        p.a.join(relative),
        "---\nname: Shared\nstatus: Planned\n---\nA content\n",
    )
    .unwrap();
    let original = "---\nname: Shared\nstatus: Planned\n---\nB divergent custom content\n";
    fs::write(p.b.join(relative), original).unwrap();
    let mut store = Store::open_existing(&p.a_db).unwrap().unwrap();
    let repo = store.repository(&p.a).unwrap().unwrap();
    store
        .apply_migration_checked(
            &MigrationPlan::capture(
                repo,
                vec!["project".into()],
                vec![SourceDocument {
                    kind: "project".into(),
                    locator: relative.into(),
                    path: p.a.join(relative),
                }],
            )
            .unwrap(),
            switchbard_core::backlog::validate_storage_snapshot,
        )
        .unwrap();
    let file = export(&p.a, &p.a_db);
    let before = fingerprint(&p.b_db);
    let result = sb(
        &p.b,
        &p.b_db,
        &["storage", "import", "--file", file.to_str().unwrap()],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("differs from exchange"));
    assert_eq!(fingerprint(&p.b_db), before);
    assert_eq!(fs::read_to_string(p.b.join(relative)).unwrap(), original);
    let b = Store::open_existing(&p.b_db).unwrap().unwrap();
    assert!(!b
        .authority(&b.repository(&p.b).unwrap().unwrap(), "project")
        .unwrap());
}

#[test]
fn differently_named_offline_allocations_can_be_explicitly_relocated() {
    let p = peers();
    successful(sb(&p.a, &p.a_db, &["create", "First independent task"]));
    successful(sb(&p.b, &p.b_db, &["create", "Second independent task"]));
    let incoming = snapshot(&p.b, &p.b_db);
    let created = incoming
        .records
        .iter()
        .find(|record| record.locator.contains("task-2 "))
        .unwrap();
    let file = export(&p.b, &p.b_db);
    let plan = preview(&p.a, &p.a_db, &file, false);
    assert!(plan["validation_errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|error| error.as_str().unwrap().contains("duplicate task public ID")));
    assert!(plan["candidate_record_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id.as_str() == Some(created.id.as_str())));
    let before = fingerprint(&p.a_db);
    assert!(!apply(&p.a, &p.a_db, &file, &plan, false, None)
        .status
        .success());
    assert_eq!(fingerprint(&p.a_db), before);
    let content = String::from_utf8(created.content.bytes().unwrap())
        .unwrap()
        .replace("TASK-2", "TASK-3");
    let resolution = resolution_file(
        &p,
        &plan,
        vec![
            serde_json::json!({"record_id":created.id,"choice":"relocate-incoming","locator":created.locator.replace("task-2 ","task-3 "),"content":ExchangeContent::from_bytes(content.as_bytes())}),
        ],
    );
    successful(apply(&p.a, &p.a_db, &file, &plan, false, Some(&resolution)));
    let file = export(&p.a, &p.a_db);
    let plan = preview(&p.b, &p.b_db, &file, false);
    successful(apply(&p.b, &p.b_db, &file, &plan, false, None));
    assert_eq!(snapshot(&p.a, &p.a_db), snapshot(&p.b, &p.b_db));
    assert_eq!(snapshot(&p.a, &p.a_db).records.len(), 3);
    assert_eq!(fs::read(&p.source).unwrap(), p.original);
}
