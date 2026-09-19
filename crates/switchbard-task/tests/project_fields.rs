//! Custom project fields through the real CLI process boundary.
use std::path::Path;
use std::process::{Command, Output};

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("backlog/tasks")).unwrap();
    std::fs::write(
        dir.path().join("backlog/config.yml"),
        "statuses: [To Do, Done]\n",
    )
    .unwrap();
    dir
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", root.join("store.sqlite3"))
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn declare(root: &Path) {
    ok(
        root,
        &[
            "project",
            "field",
            "add",
            "stage",
            "--kind",
            "enum",
            "--values",
            "Review,Approved",
            "--groupable",
        ],
    );
}
#[test]
fn project_field_lifecycle_and_filter_are_independent_of_task_fields() {
    let dir = fixture();
    let root = dir.path();
    declare(root);
    ok(root, &["field", "add", "stage", "--kind", "text"]);
    assert_eq!(
        ok(root, &["project", "field", "list"]),
        "stage\tenum\ttrue\tReview,Approved\n"
    );
    ok(
        root,
        &["project", "create", "Huntington", "--set", "stage=Review"],
    );
    ok(root, &["project", "create", "Other"]);
    let id = ok(root, &["create", "Task", "--set", "stage=Task-only"]);
    assert!(ok(root, &["project", "view", "Huntington"]).contains("stage: Review"));
    let filtered = ok(root, &["project", "list", "--where", "stage=Review"]);
    assert!(filtered.contains("Huntington"));
    assert!(!filtered.contains("Other"));
    ok(
        root,
        &["project", "edit", "Huntington", "--set", "stage=Approved"],
    );
    assert!(!run(root, &["project", "field", "remove", "stage"])
        .status
        .success());
    assert!(!run(
        root,
        &["project", "field", "edit", "stage", "--values", "Review"]
    )
    .status
    .success());
    ok(root, &["project", "edit", "Huntington", "--unset", "stage"]);
    assert!(!ok(root, &["project", "view", "Huntington"]).contains("stage:"));
    ok(
        root,
        &["project", "field", "edit", "stage", "--values", "Review"],
    );
    ok(root, &["project", "field", "remove", "stage"]);
    assert!(ok(root, &["view", id.trim()]).contains("Task-only"));
    assert!(ok(root, &["field", "list"]).contains("stage\ttext"));
}
#[test]
fn invalid_project_fields_never_partially_change_builtin_or_custom_values() {
    let dir = fixture();
    let root = dir.path();
    declare(root);
    ok(
        root,
        &[
            "project",
            "create",
            "Huntington",
            "--set",
            "stage=Review",
            "--lead",
            "Sara",
        ],
    );
    let before = ok(root, &["project", "view", "Huntington"]);
    for args in [
        vec![
            "project",
            "edit",
            "Huntington",
            "--lead",
            "Changed",
            "--set",
            "stage=Unknown",
        ],
        vec![
            "project",
            "edit",
            "Huntington",
            "--lead",
            "Changed",
            "--unset",
            "missing",
        ],
        vec![
            "project",
            "edit",
            "Huntington",
            "--set",
            "stage=Approved",
            "--unset",
            "stage",
        ],
        vec!["project", "edit", "Huntington", "--set", "malformed"],
    ] {
        assert!(
            !run(root, &args).status.success(),
            "{args:?} unexpectedly succeeded"
        );
        assert_eq!(ok(root, &["project", "view", "Huntington"]), before);
    }
    assert!(!run(
        root,
        &["project", "create", "Invalid", "--set", "stage=Unknown"]
    )
    .status
    .success());
    assert!(!ok(root, &["project", "list"]).contains("Invalid"));
    assert!(!run(root, &["project", "list", "--where", "missing=value"])
        .status
        .success());
}
#[test]
fn project_text_date_and_person_values_survive_rename_and_lifecycle() {
    let dir = fixture();
    let root = dir.path();
    for (name, kind) in [
        ("note", "text"),
        ("reviewer", "person"),
        ("review_on", "date"),
    ] {
        ok(root, &["project", "field", "add", name, "--kind", kind]);
    }
    ok(
        root,
        &[
            "project",
            "create",
            "Bank",
            "--set",
            "note=a=b",
            "--set",
            "reviewer=sara",
            "--set",
            "review_on=2026-09-25",
        ],
    );
    assert!(!run(
        root,
        &["project", "edit", "Bank", "--set", "review_on=not-a-date"]
    )
    .status
    .success());
    ok(root, &["project", "rename", "Bank", "Huntington"]);
    ok(root, &["project", "complete", "Huntington"]);
    let view = ok(root, &["project", "view", "Huntington"]);
    for value in [
        "note: a=b",
        "reviewer: sara",
        "review_on: 2026-09-25",
        "Status: Completed",
    ] {
        assert!(view.contains(value), "{view}");
    }
    assert!(!run(root, &["project", "field", "remove", "reviewer"])
        .status
        .success());
}

#[test]
fn project_fields_use_central_config_and_projects_after_cutover() {
    let dir = fixture();
    let root = dir.path();
    declare(root);
    ok(
        root,
        &["project", "create", "Bank", "--set", "stage=Review"],
    );
    let before = ok(root, &["project", "view", "Bank"]);
    let legacy_config = std::fs::read(root.join("backlog/config.yml")).unwrap();
    for kind in ["config", "project"] {
        let preview = ok(root, &["storage", "migrate", "--kind", kind]);
        let preview: serde_json::Value = serde_json::from_str(&preview).unwrap();
        let digest = preview["digest"].as_str().unwrap();
        ok(
            root,
            &[
                "storage",
                "migrate",
                "--kind",
                kind,
                "--apply",
                "--preview-digest",
                digest,
            ],
        );
    }
    assert_eq!(ok(root, &["project", "view", "Bank"]), before);
    ok(
        root,
        &["project", "edit", "Bank", "--set", "stage=Approved"],
    );
    assert!(ok(root, &["project", "view", "Bank"]).contains("stage: Approved"));
    assert!(!run(
        root,
        &["project", "field", "edit", "stage", "--values", "Review"]
    )
    .status
    .success());
    ok(root, &["project", "edit", "Bank", "--unset", "stage"]);
    ok(root, &["project", "field", "remove", "stage"]);
    assert_eq!(
        std::fs::read(root.join("backlog/config.yml")).unwrap(),
        legacy_config
    );
    assert!(ok(root, &["project", "field", "list"]).is_empty());
}
