//! A first body edit must leave the closing frontmatter fence on its own line.
use std::{
    path::Path,
    process::{Command, Output},
};

fn sb(root: &Path, database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sb"))
        .env("SWITCHBARD_DATABASE", database)
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn first_body_edits_remain_valid_for_storage_migration() {
    for flag in ["--description", "--append-notes", "--ac", "--plan"] {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("repo");
        let database = fixture.path().join("db.sqlite3");
        std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
        success(sb(&root, &database, &["create", "Bodyless task"]));
        success(sb(
            &root,
            &database,
            &["edit", "1", flag, "First body content"],
        ));
        let path = std::fs::read_dir(root.join("backlog/tasks"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let raw = std::fs::read_to_string(&path).unwrap();
        let preview = sb(&root, &database, &["storage", "migrate", "--kind", "task"]);
        assert!(
            preview.status.success(),
            "{flag}: {}\nraw task: {raw}",
            String::from_utf8_lossy(&preview.stderr)
        );
        assert!(
            !raw.contains("---##"),
            "{flag} joined body to frontmatter fence"
        );
        let custom = "\n## Custom untouched\n\n  Keep whitespace.\nUnknown: [one, null]\n";
        std::fs::write(&path, format!("{raw}{custom}")).unwrap();
        success(sb(
            &root,
            &database,
            &["edit", "1", "--description", "Updated description"],
        ));
        assert!(std::fs::read_to_string(&path).unwrap().ends_with(custom));
    }
}
