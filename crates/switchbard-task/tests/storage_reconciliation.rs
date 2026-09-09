use std::path::Path;
use std::process::Command;
use switchbard_core::storage::Store;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_reconciles_proven_stale_copy_and_backs_up_all_original_variants() {
    let temp = tempfile::tempdir().unwrap();
    let primary = temp.path().join("z-primary");
    let secondary = temp.path().join("a-secondary");
    std::fs::create_dir_all(primary.join("backlog/initiatives")).unwrap();
    git(&primary, &["init", "-q", "-b", "main"]);
    let locator = "backlog/initiatives/one.md";
    let old = "---\nname: Earlier\ncustom: {retain: old}\n---\nOriginal.\n";
    let selected = "---\nname: Current\ncustom: {retain: new}\n---\nUpdated primary.\n";
    std::fs::write(primary.join(locator), old).unwrap();
    git(&primary, &["add", "."]);
    git(&primary, &["commit", "-qm", "baseline"]);
    git(
        &primary,
        &[
            "worktree",
            "add",
            "-qb",
            "secondary",
            secondary.to_str().unwrap(),
        ],
    );
    std::fs::write(primary.join(locator), selected).unwrap();
    git(&primary, &["add", "."]);
    git(&primary, &["commit", "-qm", "primary update"]);
    let database = temp.path().join("central.sqlite3");
    let command = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sb"))
            .env("SWITCHBARD_DATABASE", &database)
            .arg("--repo")
            .arg(&secondary)
            .args(["storage", "--database"])
            .arg(&database)
            .args(["migrate", "--kind", "initiative"])
            .args(extra)
            .output()
            .unwrap()
    };
    let preview = command(&[]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["records"], 1);
    assert_eq!(preview["sources"].as_array().unwrap().len(), 2);
    assert_eq!(
        preview["sources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|source| !source["stale_proof"].is_null())
            .count(),
        1
    );
    assert!(!database.exists());
    let applied = command(&[
        "--apply",
        "--preview-digest",
        preview["digest"].as_str().unwrap(),
    ]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    let store = Store::open_existing(&database).unwrap().unwrap();
    let repo = store.repository(&primary).unwrap().unwrap();
    assert_eq!(
        store
            .read(&repo, "initiative", locator)
            .unwrap()
            .unwrap()
            .content,
        selected.as_bytes()
    );
    let recovery = Path::new(applied["recovery"].as_str().unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(recovery.join("sources.json")).unwrap()).unwrap();
    let originals = manifest["sources"][0]["original_sources"]
        .as_array()
        .unwrap();
    let contents = originals
        .iter()
        .map(|source| {
            source["bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|byte| byte.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert!(contents.contains(&old.as_bytes().to_vec()));
    assert!(contents.contains(&selected.as_bytes().to_vec()));
    assert_eq!(
        std::fs::read_to_string(primary.join(locator)).unwrap(),
        selected
    );
    assert_eq!(
        std::fs::read_to_string(secondary.join(locator)).unwrap(),
        old
    );
}

#[test]
fn cli_migrates_reviewed_secondary_addition_and_retains_original() {
    let temp = tempfile::tempdir().unwrap();
    let primary = temp.path().join("primary");
    let secondary = temp.path().join("secondary");
    std::fs::create_dir_all(&primary).unwrap();
    git(&primary, &["init", "-q", "-b", "main"]);
    std::fs::write(primary.join("README.md"), "No backlog yet\n").unwrap();
    git(&primary, &["add", "."]);
    git(&primary, &["commit", "-qm", "baseline"]);
    git(
        &primary,
        &[
            "worktree",
            "add",
            "-qb",
            "secondary",
            secondary.to_str().unwrap(),
        ],
    );
    let locator = "backlog/tasks/task-7 - New.md";
    let raw =
        "---\nid: TASK-7\ntitle: New\nstatus: To Do\ncustom: {nested: [null, true]}\n---\nBody\n";
    std::fs::create_dir_all(secondary.join("backlog/tasks")).unwrap();
    std::fs::write(secondary.join(locator), raw).unwrap();
    let database = temp.path().join("central.sqlite3");
    let command = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sb"))
            .env("SWITCHBARD_DATABASE", &database)
            .arg("--repo")
            .arg(&secondary)
            .args(["storage", "--database"])
            .arg(&database)
            .args(["migrate", "--kind", "task"])
            .args(extra)
            .output()
            .unwrap()
    };
    let preview = command(&[]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert!(preview["sources"][0]["addition_proof"].is_object());
    assert!(!database.exists());
    let applied = command(&[
        "--apply",
        "--preview-digest",
        preview["digest"].as_str().unwrap(),
    ]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    let store = Store::open_existing(&database).unwrap().unwrap();
    let repo = store.repository(&primary).unwrap().unwrap();
    assert_eq!(
        store.read(&repo, "task", locator).unwrap().unwrap().content,
        raw.as_bytes()
    );
    let recovery = Path::new(applied["recovery"].as_str().unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(recovery.join("sources.json")).unwrap()).unwrap();
    assert!(manifest["sources"][0]["original_sources"][0]["addition_proof"].is_object());
    assert_eq!(
        std::fs::read_to_string(secondary.join(locator)).unwrap(),
        raw
    );
    assert!(!primary.join("backlog").exists());
}
