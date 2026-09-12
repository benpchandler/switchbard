use super::*;
use crate::storage::{with_test_database, MigrationPlan, SourceDocument, Store};

fn fixture_root(dir: &tempfile::TempDir) -> PathBuf {
    let root = dir.path().join("repo");
    fs::create_dir_all(root.join("backlog/tasks")).expect("fixture layout");
    fs::write(root.join("backlog/config.yml"), "statuses: []\n").expect("fixture config");
    root
}

fn migrate(root: &Path, path: &Path, sources: &[(&str, &str)]) {
    let mut store = Store::open(path).expect("open isolated database");
    let repo = store.bind_repository(root).expect("bind fixture");
    let sources = sources
        .iter()
        .map(|(kind, locator)| SourceDocument {
            kind: (*kind).into(),
            locator: (*locator).into(),
            path: root.join(locator),
        })
        .collect();
    let plan = MigrationPlan::capture(
        repo,
        vec![
            "project".into(),
            "initiative".into(),
            "task".into(),
            "goals".into(),
            "ranking".into(),
        ],
        sources,
    )
    .expect("capture fixture");
    store.apply_migration(&plan).expect("cut over fixture");
}

#[test]
fn hierarchy_cutover_preserves_projection_and_custom_content_without_file_writes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = fixture_root(&dir);
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        fs::create_dir_all(root.join(PROJECTS_DIR)).expect("project dir");
        fs::create_dir_all(root.join(INITIATIVES_DIR)).expect("initiative dir");
        let project = "---\nname: Alpha\nstatus: Planned\ninitiative: North\ncustom:\n  null_value: null\n  items: [one, 'two']\n  empty: {}\n# retain this comment\n---\n\n## Custom section\nUnicode café.\n";
        let initiative = "---\nname: North\nstatus: Planned\ncustom: [1, 2]\n---\n\nIntent.\n";
        fs::write(root.join("backlog/projects/Alpha.md"), project).expect("project fixture");
        fs::write(root.join("backlog/initiatives/North.md"), initiative)
            .expect("initiative fixture");
        let before = load_backlog_repo(&root).expect("legacy read");
        migrate(
            &root,
            &db,
            &[
                ("project", "backlog/projects/Alpha.md"),
                ("initiative", "backlog/initiatives/North.md"),
            ],
        );
        let after = load_backlog_repo(&root).expect("central read");
        assert_eq!(before.project_defs, after.project_defs);
        assert_eq!(before.initiative_defs, after.initiative_defs);
        let project_outcome = edit_project_def(
            &root,
            "Alpha",
            &ProjectDefPatch {
                status: Some("Completed".into()),
                ..Default::default()
            },
        )
        .expect("central edit");
        let initiative_outcome = edit_initiative_def(
            &root,
            "North",
            &InitiativeDefPatch {
                status: Some("In Progress".into()),
                ..Default::default()
            },
        )
        .expect("central initiative edit");
        assert_eq!(project_outcome, WriteOutcome::Changed);
        assert_eq!(initiative_outcome, WriteOutcome::Changed);
        let store = Store::open(&db).expect("reopen");
        let repo = store.repository(&root).expect("resolve").expect("binding");
        let project_doc = store
            .read(&repo, "project", "backlog/projects/Alpha.md")
            .expect("read")
            .expect("document");
        assert_eq!(
            project_doc.content,
            project
                .replace("status: Planned", "status: Completed")
                .as_bytes()
        );
        let initiative_doc = store
            .read(&repo, "initiative", "backlog/initiatives/North.md")
            .expect("read")
            .expect("document");
        assert_eq!(
            initiative_doc.content,
            initiative
                .replace("status: Planned", "status: In Progress")
                .as_bytes()
        );
        assert_eq!(
            fs::read_to_string(root.join("backlog/projects/Alpha.md")).expect("source"),
            project
        );
        assert_eq!(
            fs::read_to_string(root.join("backlog/initiatives/North.md")).expect("source"),
            initiative
        );
        assert_eq!(
            load_backlog_repo(&root).expect("reload").project_defs[0].status,
            "Completed"
        );
    });
}

#[test]
fn hierarchy_empty_cutover_creates_without_legacy_directories_and_renames_stably() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = fixture_root(&dir);
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        migrate(&root, &db, &[]);
        let path = create_project_def(
            &root,
            &NewProjectDef {
                name: "Alpha".into(),
                ..Default::default()
            },
        )
        .expect("central create");
        create_initiative_def(
            &root,
            &NewInitiativeDef {
                name: "North".into(),
                ..Default::default()
            },
        )
        .expect("central initiative create");
        assert!(!root.join(PROJECTS_DIR).exists());
        assert!(!root.join(INITIATIVES_DIR).exists());
        assert!(!path.exists());
        let store = Store::open(&db).expect("open");
        let repo = store.repository(&root).expect("resolve").expect("binding");
        let before = store
            .read(&repo, "project", "backlog/projects/Alpha.md")
            .expect("read")
            .expect("document");
        assert!(
            rename_project(&root, "Alpha", "Beta")
                .expect("rename")
                .def_renamed
        );
        let after = store
            .read(&repo, "project", "backlog/projects/Alpha.md")
            .expect("read")
            .expect("document");
        assert_eq!(before.id, after.id);
        assert_eq!(
            load_backlog_repo(&root).expect("reload").project_defs[0].name,
            "Beta"
        );
        let sequence = store.change_sequence().expect("sequence");
        assert_eq!(
            edit_project_def(&root, "Beta", &ProjectDefPatch::default()).expect("noop"),
            WriteOutcome::Unchanged
        );
        assert_eq!(sequence, store.change_sequence().expect("sequence"));
    });
}

#[test]
fn hierarchy_database_failure_never_reads_or_writes_legacy_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = fixture_root(&dir);
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        let path = create_project_def(
            &root,
            &NewProjectDef {
                name: "Alpha".into(),
                ..Default::default()
            },
        )
        .expect("legacy fixture");
        let original = fs::read(&path).expect("original");
        fs::write(&db, "not a SQLite database").expect("corrupt fixture");
        assert!(load_backlog_repo(&root).is_err());
        assert!(rename_project(&root, "Alpha", "Beta").is_err());
        assert!(edit_project_def(
            &root,
            "Alpha",
            &ProjectDefPatch {
                status: Some("Completed".into()),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(fs::read(&path).expect("unchanged source"), original);
    });
}

#[test]
fn hierarchy_project_rename_refuses_mixed_authority_before_touching_any_record() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = fixture_root(&dir);
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        let path = create_project_def(
            &root,
            &NewProjectDef {
                name: "Alpha".into(),
                ..Default::default()
            },
        )
        .expect("project");
        let task_path = root.join("backlog/tasks/task-1.md");
        let task = "---\nid: TASK-1\ntitle: Member\nstatus: To Do\nproject: Alpha\n---\n";
        fs::write(&task_path, task).expect("task fixture");
        let mut store = Store::open(&db).expect("database");
        let repo = store.bind_repository(&root).expect("bind");
        let plan = MigrationPlan::capture(
            repo.clone(),
            vec!["project".into()],
            vec![SourceDocument {
                kind: "project".into(),
                locator: "backlog/projects/Alpha.md".into(),
                path: path.clone(),
            }],
        )
        .expect("capture");
        store.apply_migration(&plan).expect("project-only cutover");
        let original = store
            .read(&repo, "project", "backlog/projects/Alpha.md")
            .expect("read");
        let error = rename_project(&root, "Alpha", "Beta").expect_err("mixed rename refused");
        assert!(
            error.to_string().contains("migrate task, goals, ranking"),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(&task_path).expect("unchanged task"),
            task
        );
        assert_eq!(
            store
                .read(&repo, "project", "backlog/projects/Alpha.md")
                .expect("read"),
            original
        );
        assert!(path.exists());
        assert!(!root.join("backlog/projects/Beta.md").exists());
    });
}

fn rename_fixture(root: &Path, db: &Path, malformed_goal_scope: bool) -> Vec<(PathBuf, Vec<u8>)> {
    let project = create_project_def(
        root,
        &NewProjectDef {
            name: "Alpha".into(),
            ..Default::default()
        },
    )
    .expect("project fixture");
    let task_path = root.join("backlog/tasks/task-1.md");
    fs::write(&task_path, "---\nid: TASK-1\ntitle: Member\nstatus: To Do\nproject: Alpha\ncustom: {nested: [one, null]}\n---\n\n## Custom\nPreserve.\n").expect("task fixture");
    let scope = if malformed_goal_scope {
        "'Alpha'"
    } else {
        "Alpha"
    };
    let goals_path = root.join("backlog/goals.yml");
    fs::write(&goals_path, format!("goals:\n  - name: Goal\n    unit: things\n    scope: {scope}\n    inputs:\n      tasks: []\n      projects: ['Alpha']\n      custom: {{nested: null}}\n    weeks: {{}}\n")).expect("goals fixture");
    let ranking_path = root.join("backlog/ranking.yml");
    fs::write(&ranking_path, "expedite: []\nprojects:\n  - Alpha\ntasks:\n  Alpha:\n    - TASK-1\nroot_tasks: []\nsubissues: {}\ncustom: {empty: {}}\n").expect("ranking fixture");
    let originals = [project, task_path, goals_path, ranking_path]
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).expect("original");
            (path, bytes)
        })
        .collect();
    migrate(
        root,
        db,
        &[
            ("project", "backlog/projects/Alpha.md"),
            ("task", "backlog/tasks/task-1.md"),
            ("goals", "backlog/goals.yml"),
            ("ranking", "backlog/ranking.yml"),
        ],
    );
    originals
}

#[test]
fn hierarchy_central_project_rename_updates_all_links_atomically_and_preserves_extensions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = fixture_root(&dir);
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        let originals = rename_fixture(&root, &db, false);
        let report = rename_project(&root, "Alpha", "Beta").expect("atomic rename");
        assert!(report.def_renamed && report.goals_updated && report.ranking_updated);
        assert_eq!(report.tasks_updated, 1);
        let repo = load_backlog_repo(&root).expect("read renamed repo");
        assert_eq!(repo.project_defs[0].name, "Beta");
        assert_eq!(repo.tasks[0].project.as_deref(), Some("Beta"));
        assert_eq!(repo.goals[0].scope.as_deref(), Some("Beta"));
        assert_eq!(repo.goals[0].inputs.projects, ["Beta"]);
        assert_eq!(repo.ranking.projects, ["Beta"]);
        assert_eq!(repo.ranking.tasks["Beta"], ["TASK-1"]);
        let store = Store::open(&db).expect("reopen");
        let repo_id = store.repository(&root).expect("resolve").expect("binding");
        let task = store
            .read(&repo_id, "task", "backlog/tasks/task-1.md")
            .expect("read")
            .expect("task");
        let task_text = String::from_utf8(task.content).expect("UTF8");
        assert!(task_text.contains("custom: {nested: [one, null]}\n"));
        assert!(task_text.ends_with("---\n\n## Custom\nPreserve.\n"));
        let goals = store
            .read(&repo_id, "goals", "backlog/goals.yml")
            .expect("read")
            .expect("goals");
        assert!(String::from_utf8(goals.content)
            .expect("UTF8")
            .contains("custom: {nested: null}"));
        for (path, bytes) in originals {
            assert_eq!(fs::read(path).expect("retained source"), bytes);
        }
    });
}

#[test]
fn hierarchy_central_project_rename_late_failure_rolls_back_every_kind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = fixture_root(&dir);
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        let originals = rename_fixture(&root, &db, true);
        let store = Store::open(&db).expect("open");
        let repo = store.repository(&root).expect("resolve").expect("binding");
        let snapshot = || {
            ["project", "task", "goals", "ranking"]
                .into_iter()
                .flat_map(|kind| store.list(&repo, kind).expect("snapshot"))
                .collect::<Vec<_>>()
        };
        let before = snapshot();
        let sequence = store.change_sequence().expect("sequence");
        let error =
            rename_project(&root, "Alpha", "Beta").expect_err("late unsupported goals structure");
        assert!(error.to_string().contains("scope:"), "{error}");
        assert_eq!(
            snapshot(),
            before,
            "draft task/project edits must not become durable"
        );
        assert_eq!(store.change_sequence().expect("sequence"), sequence);
        for (path, bytes) in originals {
            assert_eq!(fs::read(path).expect("retained source"), bytes);
        }
    });
}
