//! Public command behavior with isolated homes, real Git repositories and controlled tools.
use serde_json::Value;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    repo: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home space");
        let repo = temp.path().join("repo space");
        let bin = temp.path().join("bin space");
        for directory in [&home, &repo, &bin] {
            fs::create_dir(directory).unwrap();
        }
        let status = switchbard_core::git_cmd()
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .unwrap();
        assert!(status.success());
        let git = switchbard_core::git_cmd()
            .args(["--exec-path"])
            .output()
            .unwrap();
        assert!(git.status.success());
        let git_path = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("git"))
            .find(|path| path.is_file())
            .unwrap();
        symlink(git_path, bin.join("git")).unwrap();
        symlink(env!("CARGO_BIN_EXE_sbt"), bin.join("sb")).unwrap();
        Self {
            _temp: temp,
            home,
            repo,
            bin,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sbt"));
        command
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("PATH", &self.bin)
            .env_remove("SWITCHBARD_DATABASE")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_ENTERPRISE_TOKEN")
            .env_remove("GITHUB_ENTERPRISE_TOKEN")
            .env_remove("GH_REPO");
        command
    }

    fn json(&self, github: bool) -> (Output, Value) {
        let mut command = self.command();
        command.args(["doctor", "--json"]);
        if github {
            command.arg("--github");
        }
        let output = command.output().unwrap();
        let report = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{error}: {:?}", output));
        (output, report)
    }

    fn tool(&self, name: &str, body: &str) {
        let path = self.bin.join(name);
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

fn check<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap()
}

#[test]
fn fresh_home_readiness_is_read_only_repeatable_and_optional_tools_do_not_block() {
    let fixture = Fixture::new();
    let (output, report) = fixture.json(false);
    assert!(output.status.success());
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["required_failed"], false);
    assert_eq!(check(&report, "sb")["status"], "ok");
    assert_eq!(check(&report, "workspace")["status"], "warning");
    for id in ["gh", "claude", "codex"] {
        assert_eq!(check(&report, id)["status"], "warning");
    }
    assert_eq!(check(&report, "github")["status"], "skipped");
    assert!(!fixture.home.join(".switchbard").exists());
    assert!(!fixture.repo.join("backlog").exists());
    assert_eq!(fixture.json(false).1, report);
    let text = fixture.command().arg("doctor").output().unwrap();
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("SQLite is embedded"));
    assert!(text.contains("Next:"));
    assert!(!fixture.home.join(".switchbard").exists());
}

#[test]
fn configured_database_is_read_without_rewriting_bytes_or_marker() {
    let fixture = Fixture::new();
    assert!(fixture
        .command()
        .args(["init", "--yes"])
        .status()
        .unwrap()
        .success());
    let database = fixture.home.join(".switchbard/switchbard.sqlite3");
    let marker = fixture
        .home
        .join(".switchbard/switchbard.sqlite3.established");
    let before = fs::read(&database).unwrap();
    let marker_before = fs::read(&marker).unwrap();
    let (output, report) = fixture.json(false);
    assert!(output.status.success());
    assert_eq!(check(&report, "workspace")["status"], "ok");
    assert_eq!(fs::read(database).unwrap(), before);
    assert_eq!(fs::read(marker).unwrap(), marker_before);
}

#[test]
fn corrupt_missing_established_and_unsafe_database_permissions_fail_without_repair() {
    let fixture = Fixture::new();
    assert!(fixture
        .command()
        .args(["init", "--yes"])
        .status()
        .unwrap()
        .success());
    let database = fixture.home.join(".switchbard/switchbard.sqlite3");
    let original = fs::read(&database).unwrap();
    fs::write(&database, b"this is not sqlite").unwrap();
    let (output, report) = fixture.json(false);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(check(&report, "database")["status"], "error");
    assert_eq!(fs::read(&database).unwrap(), b"this is not sqlite");
    fs::write(&database, &original).unwrap();
    fs::set_permissions(&database, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(fixture.json(false).0.status.code(), Some(1));
    assert_eq!(
        fs::metadata(&database).unwrap().permissions().mode() & 0o777,
        0o644
    );
    fs::remove_file(&database).unwrap();
    assert_eq!(fixture.json(false).0.status.code(), Some(1));
    assert!(!database.exists());
}

#[test]
fn invalid_directory_non_git_missing_git_and_read_only_destination_are_blockers() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args([
            "--repo",
            "/nonexistent-switchbard-doctor-fixture",
            "doctor",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(check(&report, "repository_directory")["status"], "error");
    let output = fixture
        .command()
        .arg("--repo")
        .arg(&fixture.home)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    fs::remove_file(fixture.bin.join("git")).unwrap();
    assert_eq!(check(&fixture.json(false).1, "git")["status"], "error");
    let fixture = Fixture::new();
    fs::set_permissions(&fixture.home, fs::Permissions::from_mode(0o500)).unwrap();
    let (output, report) = fixture.json(false);
    fs::set_permissions(&fixture.home, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(check(&report, "database")["status"], "error");
}

#[test]
fn github_is_opt_in_and_auth_failures_are_redacted_and_optional() {
    let fixture = Fixture::new();
    let marker = fixture.home.join("gh-was-run");
    fixture.tool("gh", &format!("printf ran > '{}'\nprintf 'ghp_SECRET_TEST_TOKEN\\n'\nprintf 'ghp_SECRET_TEST_TOKEN\\n' >&2\nexit 1", marker.display()));
    assert!(fixture.json(false).0.status.success());
    assert!(!marker.exists());
    let (output, report) = fixture.json(true);
    assert!(output.status.success());
    assert!(marker.exists());
    assert_eq!(check(&report, "github_auth")["status"], "warning");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SECRET"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SECRET"));
}

#[test]
fn github_repository_read_is_validated_without_showing_subprocess_data() {
    let fixture = Fixture::new();
    fixture.tool("gh", "if [ \"$1\" = auth ]; then exit 0; fi\nif [ \"$1\" = pr ]; then [ \"$3\" = --repo ] && [ \"$4\" = example/repo ] || exit 1; printf '[]'; exit 0; fi\nprintf '%s' '{\"nameWithOwner\":\"example/repo\",\"url\":\"https://github.com/example/repo\",\"token\":\"SECRET\"}'");
    let (output, report) = fixture.json(true);
    assert!(output.status.success());
    assert_eq!(check(&report, "github_repository")["status"], "ok");
    assert_eq!(check(&report, "github_pull_requests")["status"], "ok");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SECRET"));
    fixture.tool("gh", "if [ \"$1\" = auth ]; then exit 0; fi\nif [ \"$1\" = pr ]; then [ \"$3\" = --repo ] && [ \"$4\" = example/repo ] || exit 1; printf '%s' '[{\"number\":\"SECRET\"}]'; exit 0; fi\nprintf '%s' '{\"nameWithOwner\":\"example/repo\",\"url\":\"https://github.com/example/repo\"}'");
    let (output, report) = fixture.json(true);
    assert!(output.status.success());
    assert_eq!(check(&report, "github_pull_requests")["status"], "warning");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SECRET"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SECRET"));
    fixture.tool("gh", "if [ \"$1\" = auth ]; then exit 0; fi\nif [ \"$1\" = pr ]; then exit 1; fi\nprintf '%s' '{\"nameWithOwner\":\"example/repo\",\"url\":\"https://github.com/example/repo\"}'");
    assert_eq!(
        check(&fixture.json(true).1, "github_pull_requests")["status"],
        "warning"
    );
    fixture.tool("gh", "if [ \"$1\" = auth ]; then exit 0; fi\nprintf '%s' '{\"nameWithOwner\":\"example/repo\",\"url\":\"https://github.com/different/repo\"}'");
    assert_eq!(
        check(&fixture.json(true).1, "github_repository")["status"],
        "warning"
    );
    fixture.tool("gh", "if [ \"$1\" = auth ]; then exit 0; fi\nprintf '%s' '{\"nameWithOwner\":\"example/repo\",\"url\":\"https://enterprise.invalid/example/repo\"}'");
    assert_eq!(
        check(&fixture.json(true).1, "github_repository")["status"],
        "warning"
    );
    fixture.tool("gh", "if [ \"$1\" = auth ]; then exit 0; fi\nexit 1");
    assert_eq!(
        check(&fixture.json(true).1, "github_repository")["status"],
        "warning"
    );
}

#[test]
fn wal_header_is_reported_unverified_without_creating_shared_memory() {
    let fixture = Fixture::new();
    assert!(fixture
        .command()
        .args(["init", "--yes"])
        .status()
        .unwrap()
        .success());
    let database = fixture.home.join(".switchbard/switchbard.sqlite3");
    let mut bytes = fs::read(&database).unwrap();
    bytes[18] = 2;
    bytes[19] = 2;
    fs::write(&database, &bytes).unwrap();
    let (output, report) = fixture.json(false);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(check(&report, "database")["status"], "error");
    assert!(check(&report, "database")["message"]
        .as_str()
        .unwrap()
        .contains("WAL"));
    assert_eq!(fs::read(&database).unwrap(), bytes);
    assert!(!database.with_file_name("switchbard.sqlite3-shm").exists());
    assert!(!database.with_file_name("switchbard.sqlite3-wal").exists());
}

#[test]
fn sb_mismatch_and_timeout_do_not_block_local_tasks_and_hanging_git_is_bounded() {
    let fixture = Fixture::new();
    fixture.tool("sb", "printf 'commit=unknown\\n'");
    assert_eq!(check(&fixture.json(false).1, "sb")["status"], "warning");
    fixture.tool("sb", "exec /bin/sleep 30");
    let start = Instant::now();
    let (output, report) = fixture.json(false);
    assert!(output.status.success());
    assert!(start.elapsed().as_secs() < 9);
    assert!(check(&report, "sb")["message"]
        .as_str()
        .unwrap()
        .contains("time limit"));
    fs::remove_file(fixture.bin.join("sb")).unwrap();
    fixture.tool("git", "exec /bin/sleep 30");
    let start = Instant::now();
    assert_eq!(fixture.json(false).0.status.code(), Some(1));
    assert!(start.elapsed().as_secs() < 9);
}

#[test]
fn timed_out_probe_kills_its_owned_process_group() {
    let fixture = Fixture::new();
    let marker = fixture.home.join("child-pid");
    fixture.tool(
        "git",
        &format!(
            "sleep 30 & child=$!; printf '%s' \"$child\" > '{}' ; wait \"$child\"",
            marker.display()
        ),
    );
    let (output, _report) = fixture.json(false);
    assert_eq!(output.status.code(), Some(1));
    let child: i32 = fs::read_to_string(&marker).unwrap().parse().unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    while Instant::now() < deadline {
        let status = Command::new("kill")
            .args(["-0", &child.to_string()])
            .status()
            .unwrap();
        if !status.success() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("timed-out probe descendant still exists: {child}");
}

#[test]
fn explicit_database_override_is_respected_and_empty_override_is_rejected() {
    let fixture = Fixture::new();
    let path = fixture.home.join("custom.sqlite3");
    let output = fixture
        .command()
        .env("SWITCHBARD_DATABASE", &path)
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(Path::new(report["database"].as_str().unwrap()), path);
    assert!(!path.exists());
    let output = fixture
        .command()
        .env("SWITCHBARD_DATABASE", "")
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}
