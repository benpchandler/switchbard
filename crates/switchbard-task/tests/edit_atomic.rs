//! Exercise the real CLI boundary: multiple flags must not partially commit.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use switchbard_core::storage::{MigrationPlan, SourceDocument, Store};

const RAW: &str = "---\nid: TASK-1\ntitle: Original\nstatus: To Do\nlabels: []\ncustom: {untouched: [one, null]}\n---\n\n## Acceptance Criteria\n<!-- AC:BEGIN -->\n- [ ] #1 Existing\n<!-- AC:END -->\n";

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    let mut sources = Vec::new();
    for id in [1, 8] {
        let locator = format!("backlog/tasks/task-{id} - Original.md");
        let path = root.join(&locator);
        std::fs::write(&path, RAW.replace("TASK-1", &format!("TASK-{id}"))).unwrap();
        sources.push(SourceDocument {
            kind: "task".into(),
            locator,
            path,
        });
    }
    let db = temp.path().join("state.sqlite3");
    let mut store = Store::open(&db).unwrap();
    let repo = store.bind_repository(&root).unwrap();
    let plan = MigrationPlan::capture(
        repo,
        vec!["task".into(), "goals".into(), "ranking".into()],
        sources,
    )
    .unwrap();
    store.apply_migration(&plan).unwrap();
    (temp, root, db)
}

fn run(root: &Path, db: &Path, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", db)
        .arg("--repo")
        .arg(root)
        .args(["edit", "1"])
        .args(flags)
        .output()
        .unwrap()
}

#[test]
fn cli_edit_flags_commit_one_revision_and_fail_without_partial_changes() {
    let (_temp, root, db) = fixture();
    let store = Store::open(&db).unwrap();
    let repo = store.repository(&root).unwrap().unwrap();
    let locator = "backlog/tasks/task-1 - Original.md";
    let before = store.read(&repo, "task", locator).unwrap().unwrap();
    let failed = run(
        &root,
        &db,
        &[
            "--status",
            "Done",
            "--check-ac",
            "999",
            "--append-notes",
            "Never committed",
        ],
    );
    assert!(!failed.status.success());
    assert_eq!(store.read(&repo, "task", locator).unwrap().unwrap(), before);
    let success = run(
        &root,
        &db,
        &[
            "--status",
            "Done",
            "--check-ac",
            "1",
            "--append-notes",
            "Verified",
        ],
    );
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    let after = store.read(&repo, "task", locator).unwrap().unwrap();
    assert_eq!(after.revision, before.revision + 1);
    let text = String::from_utf8(after.content).unwrap();
    assert!(
        text.contains("status: Done")
            && text.contains("- [x] #1 Existing")
            && text.contains("Verified")
    );
    assert!(text.contains("custom: {untouched: [one, null]}"));
    assert_eq!(std::fs::read_to_string(root.join(locator)).unwrap(), RAW);
}

#[test]
fn cli_parent_with_edits_is_atomic_and_keeps_record_identity() {
    let (_temp, root, db) = fixture();
    let store = Store::open(&db).unwrap();
    let repo = store.repository(&root).unwrap().unwrap();
    let before = store.list(&repo, "task").unwrap();
    let failed = run(&root, &db, &["--title", "Do not retain", "--parent", "1"]);
    assert!(!failed.status.success());
    assert_eq!(store.list(&repo, "task").unwrap(), before);
    let result = run(
        &root,
        &db,
        &["--title", "Moved edit", "--check-ac", "1", "--parent", "8"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let moved = store
        .read(&repo, "task", "backlog/tasks/task-8.1 - Original.md")
        .unwrap()
        .unwrap();
    assert_eq!(moved.id, before[0].id);
    assert_eq!(moved.revision, before[0].revision + 1);
    let text = String::from_utf8(moved.content).unwrap();
    assert!(text.contains("title: Moved edit") && text.contains("- [x] #1 Existing"));
}

#[test]
fn independent_cli_processes_preserve_every_append_and_failed_edit_has_no_effect() {
    use std::process::Stdio;
    let (_temp, root, db) = fixture();
    let store = Store::open(&db).unwrap();
    let repo = store.repository(&root).unwrap().unwrap();
    let locator = "backlog/tasks/task-1 - Original.md";
    let before = store.read(&repo, "task", locator).unwrap().unwrap();
    let original = std::fs::read(root.join(locator)).unwrap();
    let children: Vec<_> = (0..8)
        .map(|index| {
            let marker = format!("process-note-{index:02}");
            let child = Command::new(env!("CARGO_BIN_EXE_sb"))
                .env("SWITCHBARD_DATABASE", &db)
                .arg("--repo")
                .arg(&root)
                .args(["edit", "1", "--append-notes", &marker])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            (marker, child)
        })
        .collect();
    let failed = run(
        &root,
        &db,
        &["--append-notes", "must-not-appear", "--check-ac", "999"],
    );
    assert!(!failed.status.success());
    for (marker, child) in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{marker}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let after = store.read(&repo, "task", locator).unwrap().unwrap();
    assert_eq!(after.revision, before.revision + 8);
    let text = String::from_utf8(after.content).unwrap();
    for index in 0..8 {
        assert_eq!(text.matches(&format!("process-note-{index:02}")).count(), 1);
    }
    assert!(!text.contains("must-not-appear"));
    assert!(text.contains("custom: {untouched: [one, null]}"));
    assert_eq!(std::fs::read(root.join(locator)).unwrap(), original);
}
