//! Retained legacy source drift is visible without changing central authority.
use std::{path::Path, process::Command};

fn sb(root: &Path, database: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", database)
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn status_reports_old_client_edits_and_retirement_without_fallback() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("repo");
    let database = fixture.path().join("state.sqlite3");
    std::fs::create_dir_all(root.join("backlog/initiatives")).unwrap();
    std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    let path = root.join("backlog/initiatives/Future.md");
    std::fs::write(
        &path,
        "---\nname: Future\nstatus: Planned\ncustom: [one, null]\n---\nOriginal body\n",
    )
    .unwrap();
    let preview: serde_json::Value = serde_json::from_str(&sb(
        &root,
        &database,
        &["storage", "migrate", "--kind", "initiative"],
    ))
    .unwrap();
    sb(
        &root,
        &database,
        &[
            "storage",
            "migrate",
            "--kind",
            "initiative",
            "--apply",
            "--preview-digest",
            preview["digest"].as_str().unwrap(),
        ],
    );
    sb(
        &root,
        &database,
        &["initiative", "edit", "Future", "--status", "Completed"],
    );
    let canonical_path = path.canonicalize().unwrap();
    let source_state = || {
        let status: serde_json::Value =
            serde_json::from_str(&sb(&root, &database, &["storage", "status"])).unwrap();
        assert_eq!(
            status["retained_sources"][0]["path"],
            canonical_path.to_str().unwrap()
        );
        status["retained_sources"][0]["state"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(source_state(), "matched");
    std::fs::write(
        &path,
        "---\nname: Old client data\nstatus: Planned\n---\nExternal change\n",
    )
    .unwrap();
    assert_eq!(source_state(), "changed");
    let central = sb(&root, &database, &["initiative", "list"]);
    assert!(central.contains("Future") && central.contains("Completed"));
    assert!(!central.contains("Old client data"));
    std::fs::remove_file(&path).unwrap();
    assert_eq!(source_state(), "missing");
    assert_eq!(sb(&root, &database, &["initiative", "list"]), central);
    assert!(!path.exists());
    std::fs::write(&database, b"corrupt established database").unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", &database)
        .arg("--repo")
        .arg(&root)
        .args(["storage", "status"])
        .output()
        .unwrap();
    assert!(
        !failed.status.success(),
        "corrupt storage cannot become legacy status"
    );
}
