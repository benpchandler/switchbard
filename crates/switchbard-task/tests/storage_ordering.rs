use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use switchbard_core::storage::{MigrationPlan, SourceDocument, Store};

const YAML: &str = "# preserve this comment\nranked: [alpha:TASK-1, unknown:TASK-9]\ncustom: {nested: [one, null]}\n";

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    database: PathBuf,
    config: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("ordering.yml");
        std::fs::write(&source, YAML).unwrap();
        let config = temp.path().join("config.toml");
        std::fs::write(
            &config,
            format!("[[repos]]\nname = 'alpha'\npath = '{}'\n", root.display()),
        )
        .unwrap();
        let database = temp.path().join("state.sqlite3");
        Self {
            _temp: temp,
            root,
            database,
            config,
            source,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sb"))
            .env("SWITCHBARD_DATABASE", &self.database)
            .arg("--repo")
            .arg(&self.root)
            .args(["storage", "--database"])
            .arg(&self.database)
            .args(["ordering", "--config"])
            .arg(&self.config)
            .args(args)
            .output()
            .unwrap()
    }

    fn preview(&self) -> serde_json::Value {
        let output = self.run(&["--migrate-from", self.source.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn apply(&self, digest: &str) -> Output {
        self.run(&[
            "--migrate-from",
            self.source.to_str().unwrap(),
            "--apply",
            "--source-digest",
            digest,
        ])
    }
}

#[test]
fn ordering_cli_preview_checks_exact_source_and_preserves_original_with_private_backup() {
    let fixture = Fixture::new();
    let preview = fixture.preview();
    assert_eq!(preview["content"], YAML);
    assert!(
        !fixture.database.exists(),
        "preview must not create a database"
    );
    std::fs::write(&fixture.source, "ranked: [changed:TASK-2]\n").unwrap();
    assert!(!fixture
        .apply(preview["digest"].as_str().unwrap())
        .status
        .success());
    assert!(
        !fixture.database.exists(),
        "digest mismatch must not activate storage"
    );
    std::fs::write(&fixture.source, YAML).unwrap();
    let output = fixture.apply(preview["digest"].as_str().unwrap());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "central");
    let backup = Path::new(result["recovery"].as_str().unwrap());
    assert!(backup.is_file());
    assert!(Store::open_existing(backup)
        .unwrap()
        .unwrap()
        .workspace_ordering()
        .unwrap()
        .is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(backup.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    assert_eq!(std::fs::read_to_string(&fixture.source).unwrap(), YAML);
    assert!(
        !fixture
            .apply(preview["digest"].as_str().unwrap())
            .status
            .success(),
        "cannot reactivate a stale source"
    );
}

#[test]
fn ordering_cli_resolves_configured_repos_and_replaces_central_raw_yaml_with_sequence_guard() {
    let fixture = Fixture::new();
    let locator = "backlog/tasks/task-1 - First.md";
    let path = fixture.root.join(locator);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "---\nid: TASK-1\ntitle: First\n---\n").unwrap();
    let mut store = Store::open(&fixture.database).unwrap();
    let repo = store.bind_repository(&fixture.root).unwrap();
    let plan = MigrationPlan::capture(
        repo.clone(),
        vec!["task".into()],
        vec![SourceDocument {
            kind: "task".into(),
            locator: locator.into(),
            path,
        }],
    )
    .unwrap();
    store.apply_migration(&plan).unwrap();
    let preview = fixture.preview();
    let applied = fixture.apply(preview["digest"].as_str().unwrap());
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(result["resolved"], 1);
    assert_eq!(result["unresolved"], 1);
    let sequence = result["sequence"].as_u64().unwrap();
    let shown = fixture.run(&[]);
    assert!(shown.status.success());
    assert_eq!(shown.stdout, YAML.as_bytes());
    assert!(String::from_utf8(shown.stderr)
        .unwrap()
        .contains(&format!("sequence: {sequence}")));
    let edit_file = fixture.root.join("edited.yml");
    let edited = format!("{YAML}another_custom_field: {{flexible: true}}\n");
    std::fs::write(&edit_file, &edited).unwrap();
    let output = fixture.run(&[
        "--write-from",
        edit_file.to_str().unwrap(),
        "--expected-sequence",
        &sequence.to_string(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated = store.workspace_ordering().unwrap().unwrap();
    assert_eq!(updated.content, edited.as_bytes());
    assert!(updated.entries[0].target.is_some());
    assert_eq!(std::fs::read_to_string(&fixture.source).unwrap(), YAML);
    assert!(!fixture
        .run(&[
            "--write-from",
            edit_file.to_str().unwrap(),
            "--expected-sequence",
            &sequence.to_string()
        ])
        .status
        .success());
    assert_eq!(store.workspace_ordering().unwrap().unwrap(), updated);
    let now = store.change_sequence().unwrap();
    std::fs::write(&edit_file, "ranked: invalid-shape").unwrap();
    assert!(!fixture
        .run(&[
            "--write-from",
            edit_file.to_str().unwrap(),
            "--expected-sequence",
            &now.to_string()
        ])
        .status
        .success());
    assert_eq!(store.workspace_ordering().unwrap().unwrap(), updated);
    assert_eq!(store.change_sequence().unwrap(), now);
}
