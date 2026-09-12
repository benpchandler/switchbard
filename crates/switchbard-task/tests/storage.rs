//! Actual CLI journeys for the first strangler cutover; each process has an isolated database.
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn sb(root: &Path, database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", database)
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .expect("run sb")
}

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF8 output")
}

#[test]
fn initiative_cutover_preserves_reads_custom_content_and_legacy_files() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().join("repo");
    let database = fixture.path().join("store.sqlite3");
    fs::create_dir_all(root.join("backlog/initiatives")).expect("defs");
    fs::create_dir_all(root.join("backlog/tasks")).expect("tasks");
    let path = root.join("backlog/initiatives/Future.md");
    let source = "---\nname: Future\nstatus: Planned\ncustom: {nested: [one, null], empty: ''}\n---\n\nA flexible document.\n\n## Custom\nKeep exactly.\n";
    fs::write(&path, source).expect("source");
    let before = successful(sb(&root, &database, &["initiative", "list"]));
    let preview = successful(sb(
        &root,
        &database,
        &["storage", "migrate", "--kind", "initiative"],
    ));
    assert!(preview.contains("preview"));
    assert!(!database.exists(), "preview must not create central state");
    migrate(&root, &database, "initiative");
    assert_eq!(
        before,
        successful(sb(&root, &database, &["initiative", "list"]))
    );
    successful(sb(
        &root,
        &database,
        &["initiative", "edit", "Future", "--status", "Completed"],
    ));
    assert_eq!(fs::read_to_string(&path).expect("original"), source);
    let store = switchbard_core::storage::Store::open(&database).expect("store");
    let repo = store
        .repository(&root)
        .expect("binding")
        .expect("registered");
    let docs = store.list(&repo, "initiative").expect("docs");
    assert_eq!(docs.len(), 1);
    let updated = String::from_utf8(docs[0].content.clone()).expect("text");
    assert_eq!(
        updated,
        source.replace("status: Planned", "status: Completed")
    );
    assert!(store.authority(&repo, "initiative").expect("authority"));
    assert!(!store.authority(&repo, "project").expect("legacy kind"));
    drop(store);
    fs::remove_dir_all(root.join("backlog"))
        .expect("simulate retired legacy files in fixture only");
    let after = successful(sb(&root, &database, &["initiative", "list"]));
    assert!(after.contains("Future") && after.contains("Completed"));
    successful(sb(&root, &database, &["initiative", "create", "Second"]));
    assert!(
        !root.join("backlog").exists(),
        "central writes cannot recreate legacy tree"
    );
}

#[test]
fn corrupt_database_cannot_silently_write_legacy_definition() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().join("repo");
    let database = fixture.path().join("store.sqlite3");
    fs::create_dir_all(root.join("backlog/tasks")).expect("tasks");
    migrate(&root, &database, "initiative");
    fs::write(&database, b"not a sqlite database").expect("inject corruption");
    let result = sb(
        &root,
        &database,
        &["initiative", "create", "Unsafe fallback"],
    );
    assert!(!result.status.success());
    assert!(!root.join("backlog/initiatives").exists());
}

fn migrate(root: &Path, database: &Path, kind: &str) {
    let preview = successful(sb(root, database, &["storage", "migrate", "--kind", kind]));
    let value: serde_json::Value = serde_json::from_str(&preview).expect("preview JSON");
    let digest = value["digest"].as_str().expect("preview digest");
    successful(sb(
        root,
        database,
        &[
            "storage",
            "migrate",
            "--kind",
            kind,
            "--apply",
            "--preview-digest",
            digest,
        ],
    ));
}

#[test]
fn stale_migration_preview_rejects_without_database_or_source_effects() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().join("repo");
    let database = fixture.path().join("store.sqlite3");
    fs::create_dir_all(root.join("backlog/initiatives")).expect("directory");
    let path = root.join("backlog/initiatives/One.md");
    fs::write(&path, "---\nname: One\n---\noriginal\n").expect("original");
    let preview = successful(sb(
        &root,
        &database,
        &["storage", "migrate", "--kind", "initiative"],
    ));
    let preview: serde_json::Value = serde_json::from_str(&preview).expect("preview");
    fs::write(&path, "---\nname: One\n---\nchanged after review\n").expect("edit");
    let result = sb(
        &root,
        &database,
        &[
            "storage",
            "migrate",
            "--kind",
            "initiative",
            "--apply",
            "--preview-digest",
            preview["digest"].as_str().expect("digest"),
        ],
    );
    assert!(!result.status.success());
    assert!(!database.exists());
    assert!(fs::read_to_string(path)
        .expect("source")
        .contains("changed after review"));
}

#[test]
fn two_cli_peers_bootstrap_edit_and_skip_exports_without_replaying_history() {
    let fixture = tempfile::tempdir().expect("fixture");
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let ad = fixture.path().join("a.sqlite3");
    let bd = fixture.path().join("b.sqlite3");
    let exchange = fixture.path().join("tasks.json");
    fs::create_dir_all(a.join("backlog/initiatives")).expect("a");
    fs::create_dir_all(&b).expect("b");
    fs::write(
        a.join("backlog/initiatives/One.md"),
        "---\nname: One\nstatus: Planned\ncustom: {nested: [1, null]}\n---\nKeep me.\n",
    )
    .expect("source");
    migrate(&a, &ad, "initiative");
    export(&a, &ad, &exchange);
    let first = fs::read(&exchange).expect("first snapshot");
    import(&b, &bd, &exchange, true);
    successful(sb(
        &b,
        &bd,
        &["initiative", "edit", "One", "--status", "In Progress"],
    ));
    export(&b, &bd, &exchange);
    import(&a, &ad, &exchange, false);
    assert!(successful(sb(&a, &ad, &["initiative", "list"])).contains("In Progress"));
    for status in ["Completed", "Canceled"] {
        successful(sb(
            &a,
            &ad,
            &["initiative", "edit", "One", "--status", status],
        ));
        export(&a, &ad, &exchange);
    }
    import(&b, &bd, &exchange, false);
    assert!(successful(sb(&b, &bd, &["initiative", "list"])).contains("Canceled"));
    fs::write(&exchange, first).expect("stale shared file");
    import(&b, &bd, &exchange, false);
    assert!(successful(sb(&b, &bd, &["initiative", "list"])).contains("Canceled"));
    assert!(!b.join("backlog").exists());
}

fn export(root: &Path, database: &Path, path: &Path) {
    successful(sb(
        root,
        database,
        &["storage", "export", "--file", path.to_str().expect("path")],
    ));
}

fn import(root: &Path, database: &Path, path: &Path, bind: bool) {
    let mut args = vec!["storage", "import", "--file", path.to_str().expect("path")];
    if bind {
        args.push("--bind");
    }
    let preview = successful(sb(root, database, &args));
    let preview: serde_json::Value = serde_json::from_str(&preview).expect("preview");
    let sequence = preview["expected_sequence"]
        .as_u64()
        .expect("sequence")
        .to_string();
    args.extend([
        "--apply",
        "--snapshot-digest",
        preview["incoming_digest"].as_str().expect("digest"),
        "--expected-sequence",
        &sequence,
    ]);
    successful(sb(root, database, &args));
}

#[test]
fn custom_document_edit_is_lossless_revision_checked_and_requires_no_schema_change() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().join("repo");
    let database = fixture.path().join("store.sqlite3");
    fs::create_dir_all(root.join("backlog/initiatives")).expect("directory");
    fs::write(
        root.join("backlog/initiatives/One.md"),
        "---\nname: One\nstatus: Planned\n---\nBody\n",
    )
    .expect("source");
    migrate(&root, &database, "initiative");
    let store = switchbard_core::storage::Store::open(&database).expect("store");
    let repo = store.repository(&root).expect("binding").expect("repo");
    let record = store
        .list(&repo, "initiative")
        .expect("documents")
        .remove(0);
    drop(store);
    let source = fixture.path().join("document.md");
    let custom = "---\nname: One\nstatus: Planned\ncustom: {nested: [1, null], empty: '', list: []}\n---\nBody\n\n## My section\n你好\n";
    fs::write(&source, custom).expect("custom content");
    let revision = record.revision.to_string();
    let args = [
        "storage",
        "document",
        "--id",
        &record.id,
        "--write-from",
        source.to_str().expect("path"),
        "--expected-revision",
        &revision,
    ];
    successful(sb(&root, &database, &args));
    assert!(
        !sb(&root, &database, &args).status.success(),
        "stale replacement refused"
    );
    let result = sb(
        &root,
        &database,
        &["storage", "document", "--id", &record.id],
    );
    assert_eq!(successful(result), custom);
    successful(sb(
        &root,
        &database,
        &["initiative", "edit", "One", "--status", "Completed"],
    ));
    assert_eq!(
        successful(sb(
            &root,
            &database,
            &["storage", "document", "--id", &record.id]
        )),
        custom.replace("status: Planned", "status: Completed")
    );
}
