//! Consume the independently authored fixed v2 bytes through the actual CLI.
use std::process::{Command, Output};

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn independent_wire_golden_bootstraps_and_exports_identical_bytes() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let database = fixture.path().join("db.sqlite3");
    let input = fixture.path().join("input.json");
    let output = fixture.path().join("output.json");
    let golden =
        include_bytes!("../../../docs/decisions/switchbard-owned-storage/exchange-v2.example.json");
    std::fs::write(&input, golden).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sb"))
            .env("SWITCHBARD_DATABASE", &database)
            .arg("--repo")
            .arg(&root)
            .args(args)
            .output()
            .unwrap()
    };
    let preview = success(run(&[
        "storage",
        "import",
        "--bind",
        "--file",
        input.to_str().unwrap(),
    ]));
    let preview: serde_json::Value = serde_json::from_slice(&preview).unwrap();
    assert_eq!(
        preview["incoming_digest"],
        "580aa9c35261e21ab7a872399e834108aee184b3a0f06f88dba7e29875b46a69"
    );
    success(run(&[
        "storage",
        "import",
        "--bind",
        "--file",
        input.to_str().unwrap(),
        "--apply",
        "--snapshot-digest",
        "580aa9c35261e21ab7a872399e834108aee184b3a0f06f88dba7e29875b46a69",
        "--expected-sequence",
        "0",
    ]));
    let listing = String::from_utf8(success(run(&["initiative", "list"]))).unwrap();
    assert!(listing.contains("Example") && listing.contains("Planned"));
    success(run(&[
        "storage",
        "export",
        "--file",
        output.to_str().unwrap(),
    ]));
    assert_eq!(std::fs::read(output).unwrap(), golden);
    assert_eq!(std::fs::read(input).unwrap(), golden);
    assert!(!root.join("backlog").exists());
}
