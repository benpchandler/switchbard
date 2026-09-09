use super::*;
use crate::backlog::{goals, ranking, status_config};
use crate::storage::{with_test_database, MigrationPlan, SourceDocument};
use std::fs;

fn migrate(root: &Path, db: &Path, sources: &[(&str, &str)]) {
    let mut store = Store::open(db).expect("open fixture database");
    let repo = store.bind_repository(root).expect("bind");
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
        vec!["goals".into(), "ranking".into(), "config".into()],
        sources,
    )
    .expect("capture");
    store.apply_migration(&plan).expect("migrate");
}

#[test]
fn aggregate_cutover_preserves_raw_extensions_and_reads_changed_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("repo");
    fs::create_dir_all(root.join("backlog/tasks")).expect("layout");
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        let config = "statuses: []\ntask_prefix: CUSTOM\ncustom: {empty: {}, nothing: null}\n# config comment\n";
        let goals = "# goals comment\ncustom: {list: [one, two]}\ngoals:\n  - name: Goal\n    unit: items\n    measure: manual\n    custom: {nested: null}\n    inputs:\n      tasks: ['TASK-1']\n      projects: []\n      custom: {preserve: [1, 2]}\n    weeks:\n      2026-09-07:\n        target: 2\n        checkins: []\n";
        let ranking = "# rank comment\ncustom: {nested: [true, null]}\nexpedite: ['TASK-1']\nprojects: []\ntasks: {}\nroot_tasks: []\nsubissues: {}\n";
        for (name, text) in [
            ("config.yml", config),
            ("goals.yml", goals),
            ("ranking.yml", ranking),
        ] {
            fs::write(root.join("backlog").join(name), text).expect("source fixture");
        }
        let before = crate::backlog::load_backlog_repo(&root).expect("legacy read");
        migrate(
            &root,
            &db,
            &[
                ("goals", "backlog/goals.yml"),
                ("ranking", "backlog/ranking.yml"),
                ("config", "backlog/config.yml"),
            ],
        );
        let after = crate::backlog::load_backlog_repo(&root).expect("central read");
        assert_eq!(before.goals, after.goals);
        assert_eq!(before.ranking, after.ranking);
        goals::check_in_goal(&root, "Goal", "2026-09-07", "2026-09-08", 1).expect("check in");
        assert_eq!(
            goals::attach_goal_inputs(&root, "Goal", &["TASK-2".into()], &[]).expect("attach"),
            1
        );
        assert!(ranking::unexpedite_task(&root, "TASK-1")
            .expect("unexpedite")
            .changed());
        status_config::add_standard_statuses(&root).expect("statuses");
        let current = read(&root, "goals", "backlog/goals.yml")
            .expect("read")
            .expect("central")
            .expect("content");
        assert!(current.contains("custom: {preserve: [1, 2]}"));
        assert!(current.contains("# goals comment\ncustom: {list: [one, two]}"));
        assert!(current.contains("{ date: 2026-09-08, value: 1 }"));
        assert!(current.contains("tasks: ['TASK-1', 'TASK-2']"));
        let current = read(&root, "ranking", "backlog/ranking.yml")
            .expect("read")
            .expect("central")
            .expect("content");
        assert!(current.contains("custom: {nested: [true, null]}"));
        assert!(current.contains("expedite: []"));
        let current = status_config::read_config(&root)
            .expect("config")
            .expect("content");
        assert!(current.ends_with(
            "task_prefix: CUSTOM\ncustom: {empty: {}, nothing: null}\n# config comment\n"
        ));
        for (name, text) in [
            ("config.yml", config),
            ("goals.yml", goals),
            ("ranking.yml", ranking),
        ] {
            assert_eq!(
                fs::read_to_string(root.join("backlog").join(name)).expect("retained source"),
                text
            );
        }
    });
}

#[test]
fn aggregate_empty_authority_creates_without_backlog_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("repo");
    fs::create_dir(&root).expect("root");
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        migrate(&root, &db, &[]);
        goals::create_goal(
            &root,
            &goals::NewGoal {
                name: "First".into(),
                unit: "items".into(),
                measure: goals::GoalMeasure::Manual,
                scope: None,
                week: "2026-09-07".into(),
                target: 1,
            },
        )
        .expect("create central goal");
        status_config::add_standard_statuses(&root).expect("create central config");
        assert!(!root.join("backlog").exists());
        let mut warnings = Vec::new();
        assert_eq!(
            goals::load_goals(&root, &mut warnings).expect("read")[0].name,
            "First"
        );
    });
}

#[test]
fn aggregate_command_cas_rejects_stale_noop_and_absent_creation_without_losing_concurrent_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("repo");
    fs::create_dir(&root).expect("root");
    let db = dir.path().join("state.sqlite3");
    with_test_database(&db, || {
        migrate(&root, &db, &[]);
        let mutate_other = |content: &str| {
            let mut store = Store::open(&db).expect("other connection");
            let repo = store.repository(&root).expect("resolve").expect("binding");
            store
                .mutate(&repo, "goals", "backlog/goals.yml", None, |_| {
                    Ok(Some(content.as_bytes().to_vec()))
                })
                .expect("concurrent write");
        };
        let absent = with_edit(&root, "goals", "backlog/goals.yml", |edit| {
            mutate_other("goals: []\ncustom: first\n");
            assert!(edit.stage("goals: []\ncustom: lost\n"));
            Ok(())
        });
        assert!(absent.is_err());
        let stale_noop = with_edit(&root, "goals", "backlog/goals.yml", |_| {
            mutate_other("goals: []\ncustom: second\n");
            Ok(())
        });
        assert!(stale_noop.is_err());
        assert_eq!(
            read(&root, "goals", "backlog/goals.yml")
                .expect("read")
                .flatten()
                .expect("text"),
            "goals: []\ncustom: second\n"
        );
        let rejected: Result<()> = with_edit(&root, "goals", "backlog/goals.yml", |edit| {
            assert!(edit.stage("goals: []\ncustom: rejected\n"));
            anyhow::bail!("later validation rejected staged command")
        });
        assert!(rejected.is_err());
        assert_eq!(
            read(&root, "goals", "backlog/goals.yml")
                .expect("read")
                .flatten()
                .expect("text"),
            "goals: []\ncustom: second\n"
        );
    });
}
