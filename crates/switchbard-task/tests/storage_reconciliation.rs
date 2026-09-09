use std::path::Path;
use std::process::Command;
use switchbard_core::backlog::migration::content_digest;
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

#[test]
fn cli_applies_digest_bound_source_selection_and_rechecks_resolution_file() {
    let temp = tempfile::tempdir().unwrap();
    let primary = temp.path().join("primary");
    let secondary = temp.path().join("secondary");
    let locator = "backlog/initiatives/one.md";
    std::fs::create_dir_all(primary.join("backlog/initiatives")).unwrap();
    git(&primary, &["init", "-q", "-b", "main"]);
    std::fs::write(primary.join(locator), b"baseline").unwrap();
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
    std::fs::write(primary.join(locator), b"primary choice").unwrap();
    std::fs::write(secondary.join(locator), b"secondary choice").unwrap();
    let resolution = temp.path().join("resolution.json");
    let write_resolution = |note: Option<&str>| {
        let mut value = serde_json::json!({
            "version": 1,
            "kind": "initiative",
            "resolutions": [{
                "source_locators": [locator],
                "target_locator": locator,
                "sources": [
                    {"path": primary.join(locator), "digest": content_digest(b"primary choice"), "decision": "select"},
                    {"path": secondary.join(locator), "digest": content_digest(b"secondary choice"), "decision": "reject"}
                ]
            }]
        });
        if let Some(note) = note {
            value
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), note.into());
        }
        std::fs::write(&resolution, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    };
    write_resolution(None);
    let database = temp.path().join("central.sqlite3");
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sb"))
            .arg("--repo")
            .arg(&primary)
            .args(["storage", "--database"])
            .arg(&database)
            .args(["migrate", "--kind", "initiative", "--resolution-file"])
            .arg(&resolution)
            .args(extra)
            .output()
            .unwrap()
    };
    let preview = run(&[]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert!(preview["resolution_file_digest"].is_string());
    assert_eq!(
        preview["resolutions"][0]["selected_source_digest"],
        content_digest(b"primary choice")
    );
    let digest = preview["digest"].as_str().unwrap().to_owned();
    let original = std::fs::read(&resolution).unwrap();
    std::fs::write(&resolution, [original.as_slice(), b"\n"].concat()).unwrap();
    let changed = run(&["--apply", "--preview-digest", &digest]);
    assert!(!changed.status.success());
    assert!(!database.exists());
    write_resolution(None);
    let applied = run(&["--apply", "--preview-digest", &digest]);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let store = Store::open_existing(&database).unwrap().unwrap();
    let repo = store.repository(&primary).unwrap().unwrap();
    assert_eq!(
        store
            .read(&repo, "initiative", locator)
            .unwrap()
            .unwrap()
            .content,
        b"primary choice"
    );
    assert_eq!(
        std::fs::read(primary.join(locator)).unwrap(),
        b"primary choice"
    );
    assert_eq!(
        std::fs::read(secondary.join(locator)).unwrap(),
        b"secondary choice"
    );
}

#[test]
fn cli_collapses_lifecycle_aliases_and_reissues_unrelated_duplicate_id() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    let completed = "backlog/completed/led-576 - Canonical.md";
    let archived = "backlog/archive/tasks/led-576 - Duplicate.md";
    let retained = "backlog/archive/tasks/led-504 - Plaid.md";
    let reissued_source = "backlog/archive/tasks/led-504 - Aggregator.md";
    let reissued_target = "backlog/archive/tasks/led-675 - Aggregator.md";
    for locator in [completed, archived, retained, reissued_source] {
        std::fs::create_dir_all(root.join(locator).parent().unwrap()).unwrap();
    }
    let canonical = "---\nid: LED-576\ntitle: Canonical\nstatus: Done\n---\nBody\n";
    let alias = "---\nid: LED-576\ntitle: Canonical\nstatus: Done\n---\nOlder body\n";
    let plaid = "---\nid: LED-504\ntitle: Plaid\nstatus: Archived\n---\n";
    let aggregator = "---\nid: LED-504\ntitle: Aggregator\nstatus: Archived\n---\n";
    for (locator, bytes) in [
        (completed, canonical),
        (archived, alias),
        (retained, plaid),
        (reissued_source, aggregator),
    ] {
        std::fs::write(root.join(locator), bytes).unwrap();
    }
    let resolution = temp.path().join("tasks-resolution.json");
    let source = |locator: &str, bytes: &str, decision: &str| {
        serde_json::json!({
            "path": root.join(locator), "digest": content_digest(bytes.as_bytes()), "decision": decision
        })
    };
    std::fs::write(&resolution, serde_json::to_vec_pretty(&serde_json::json!({
        "version": 1,
        "kind": "task",
        "resolutions": [
            {"source_locators": [completed, archived], "target_locator": completed,
             "sources": [source(completed, canonical, "select"), source(archived, alias, "reject")]},
            {"source_locators": [retained], "target_locator": retained,
             "sources": [source(retained, plaid, "select")]},
            {"source_locators": [reissued_source], "target_locator": reissued_target,
             "sources": [source(reissued_source, aggregator, "select")], "repair_task_id": "LED-675"}
        ]
    })).unwrap()).unwrap();
    let database = temp.path().join("central.sqlite3");
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sb"))
            .arg("--repo")
            .arg(&root)
            .args(["storage", "--database"])
            .arg(&database)
            .args(["migrate", "--kind", "task", "--resolution-file"])
            .arg(&resolution)
            .args(extra)
            .output()
            .unwrap()
    };
    let preview = run(&[]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["records"], 3);
    assert_eq!(preview["resolutions"].as_array().unwrap().len(), 3);
    let applied = run(&[
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
    let repo = store.repository(&root).unwrap().unwrap();
    assert_eq!(store.list(&repo, "task").unwrap().len(), 3);
    assert!(store.read(&repo, "task", archived).unwrap().is_none());
    let repaired = store.read(&repo, "task", reissued_target).unwrap().unwrap();
    assert!(String::from_utf8(repaired.content)
        .unwrap()
        .contains("id: LED-675"));
    assert_eq!(
        std::fs::read_to_string(root.join(reissued_source)).unwrap(),
        aggregator
    );
    let recovery = Path::new(applied["recovery"].as_str().unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(recovery.join("sources.json")).unwrap()).unwrap();
    assert_eq!(manifest["resolutions"].as_array().unwrap().len(), 3);
    assert!(manifest["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["selected_content_digest"].is_string()));
}
