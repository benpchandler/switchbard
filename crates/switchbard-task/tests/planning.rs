//! Actual CLI planning and migration journeys against an isolated central store.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
use switchbard_core::storage::{MigrationPlan, SourceDocument, Store};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
        let config = root.join("backlog/config.yml");
        std::fs::write(&config, "# retained comment\nstatuses: [Icebox, To Do, In Progress, Done]\ndefault_status: To Do\n").unwrap();
        let ranking = root.join("backlog/ranking.yml");
        std::fs::write(&ranking, "expedite: [TASK-2]\n").unwrap();
        let mut sources = vec![
            source(&root, &config, "config"),
            source(&root, &ranking, "ranking"),
        ];
        for (id, status, parent, checks) in [
            ("TASK-1", "To Do", "", "- [x] #1 Parent result\n"),
            (
                "TASK-1.1",
                "In Progress",
                "parent_task_id: TASK-1\n",
                "- [ ] #1 Child one\n- [ ] #2 Child two\n- [ ] #3 Child three\n",
            ),
            ("TASK-2", "Icebox", "", ""),
        ] {
            let path = root.join(format!("backlog/tasks/{id}.md"));
            std::fs::write(&path, format!("---\nid: {id}\ntitle: {id} result\nstatus: {status}\n{parent}---\n\n## Acceptance Criteria\n<!-- AC:BEGIN -->\n{checks}<!-- AC:END -->\n")).unwrap();
            sources.push(source(&root, &path, "task"));
        }
        let db = dir.path().join("db.sqlite3");
        let mut store = Store::open(&db).unwrap();
        let repo = store.bind_repository(&root).unwrap();
        store
            .apply_migration(
                &MigrationPlan::capture(
                    repo,
                    vec!["task".into(), "config".into(), "ranking".into()],
                    sources,
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            _dir: dir,
            root,
            db,
        }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sb"))
            .env("SWITCHBARD_DATABASE", &self.db)
            .arg("--repo")
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn migrate(&self) -> Value {
        let plan = self.root.join("preview.json");
        self.ok(&["planning", "preview", "--out", plan.to_str().unwrap()]);
        serde_json::from_str(&self.ok(&[
            "planning",
            "apply",
            "--plan",
            plan.to_str().unwrap(),
            "--backup-dir",
            self.root.join("backup").to_str().unwrap(),
        ]))
        .unwrap()
    }
}
fn source(root: &Path, path: &Path, kind: &str) -> SourceDocument {
    SourceDocument {
        kind: kind.into(),
        locator: path.strip_prefix(root).unwrap().to_str().unwrap().into(),
        path: path.into(),
    }
}
fn task<'a>(list: &'a Value, id: &str) -> &'a Value {
    list["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .unwrap()
}

#[test]
fn migrate_and_work_without_conflating_planning_status_or_checks() {
    let f = Fixture::new();
    let receipt = f.migrate();
    assert_eq!(receipt["tasks_changed"], 3);
    assert!(Path::new(receipt["backup_path"].as_str().unwrap()).is_file());
    let list: Value = serde_json::from_str(&f.ok(&["planning", "list"])).unwrap();
    assert_eq!(task(&list, "TASK-1")["planning"], "Planned");
    assert_eq!(task(&list, "TASK-1")["status"], "Not started");
    assert_eq!(task(&list, "TASK-1")["percentage"], 25.0);
    assert_eq!(task(&list, "TASK-2")["planning"], "Considering");
    assert!(task(&list, "TASK-2")["percentage"].is_null());
    f.ok(&["planning", "set", "2", "Planned"]);
    f.ok(&["planning", "rank", "2", "--top"]);
    let list: Value = serde_json::from_str(&f.ok(&["planning", "list"])).unwrap();
    assert_eq!(list["planned_order"][0], "TASK-2");
    assert_eq!(task(&list, "TASK-2")["status"], "Not started");
    for n in ["1", "2", "3"] {
        f.ok(&["edit", "1.1", "--check-ac", n]);
    }
    let list: Value = serde_json::from_str(&f.ok(&["planning", "list"])).unwrap();
    assert_eq!(task(&list, "TASK-1")["percentage"], 100.0);
    assert_eq!(task(&list, "TASK-1")["status"], "Not started");
    assert_eq!(task(&list, "TASK-1")["needs_review"], true);
    f.ok(&["edit", "1", "-s", "Done"]);
    let list: Value = serde_json::from_str(&f.ok(&["planning", "list"])).unwrap();
    assert_eq!(task(&list, "TASK-1")["needs_review"], false);
    let id = f.ok(&["create", "New consideration"]);
    let view = f.ok(&["view", id.trim()]);
    assert!(view.contains("Planning: Considering"), "{view}");
    assert!(view.contains("Status: Not started"), "{view}");
}

#[test]
fn stale_preview_refuses_and_repeating_success_is_a_noop() {
    let f = Fixture::new();
    let plan = f.root.join("stale.json");
    f.ok(&["planning", "preview", "--out", plan.to_str().unwrap()]);
    f.ok(&["edit", "1", "--title", "Changed after preview"]);
    let out = f.run(&[
        "planning",
        "apply",
        "--plan",
        plan.to_str().unwrap(),
        "--backup-dir",
        f.root.join("backup").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(f.ok(&["view", "1"]).contains("Changed after preview"));
    f.migrate();
    let plan = f.root.join("again.json");
    let preview: Value =
        serde_json::from_str(&f.ok(&["planning", "preview", "--out", plan.to_str().unwrap()]))
            .unwrap();
    assert_eq!(preview["already_migrated"], true);
    let out: Value = serde_json::from_str(&f.ok(&[
        "planning",
        "apply",
        "--plan",
        plan.to_str().unwrap(),
        "--backup-dir",
        f.root.join("backup-again").to_str().unwrap(),
    ]))
    .unwrap();
    assert_eq!(out["already_applied"], true);
}
