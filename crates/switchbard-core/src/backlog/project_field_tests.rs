use super::*;
use crate::storage::{with_test_database, MigrationPlan, SourceDocument, Store};
use crate::{
    create_project_def, edit_project_def, edit_project_def_if_unchanged, load_backlog_repo,
    FieldKind, NewProjectDef, ProjectDefPatch,
};
use std::fs;

fn decl(name: &str, kind: FieldKind, values: &[&str]) -> FieldDecl {
    FieldDecl {
        name: name.into(),
        kind,
        values: values.iter().map(|v| (*v).into()).collect(),
        groupable: true,
    }
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("backlog/tasks")).unwrap();
    fs::write(
        dir.path().join("backlog/config.yml"),
        "# retain comment\nstatuses: [To Do, Done]\n",
    )
    .unwrap();
    dir
}

#[test]
fn project_fields_are_isolated_and_preserve_unrelated_config_and_content() {
    let dir = fixture();
    let root = dir.path();
    super::super::field_config::add_field_decl(root, decl("stage", FieldKind::Enum, &["task"]))
        .unwrap();
    let task_config = fs::read_to_string(root.join("backlog/config.yml")).unwrap();
    add_project_field_decl(root, decl("stage", FieldKind::Enum, &["project", "closed"])).unwrap();
    let path = create_project_def(
        root,
        &NewProjectDef {
            name: "Bank".into(),
            set_fields: vec![("stage".into(), " project ".into())],
            ..Default::default()
        },
    )
    .unwrap();
    let original = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        original.replace(
            "status: Planned",
            "status: Planned\nunknown: [retain, this]\n# project comment",
        ),
    )
    .unwrap();
    let repo = load_backlog_repo(root).unwrap();
    assert_eq!(repo.fields[0].values, ["task"]);
    assert_eq!(repo.project_defs[0].custom["stage"], "project");
    let _ = edit_project_def(
        root,
        "Bank",
        &ProjectDefPatch {
            set_fields: vec![("stage".into(), "closed".into())],
            ..Default::default()
        },
    )
    .unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("unknown: [retain, this]\n# project comment"));
    let _ = edit_project_def(
        root,
        "Bank",
        &ProjectDefPatch {
            unset_fields: vec!["stage".into()],
            ..Default::default()
        },
    )
    .unwrap();
    remove_project_field_decl(root, "stage").unwrap();
    assert!(declared_project_fields(root).unwrap().is_empty());
    assert!(fs::read_to_string(root.join("backlog/config.yml"))
        .unwrap()
        .starts_with(&task_config));
}

#[test]
fn project_fields_native_boundary_refuses_invalid_and_conflicting_values_atomically() {
    let dir = fixture();
    let root = dir.path();
    for name in [
        "name",
        "status",
        "target_date",
        "initiative",
        "lead",
        "description",
        "Bad-Name",
    ] {
        assert!(
            add_project_field_decl(root, decl(name, FieldKind::Text, &[])).is_err(),
            "{name}"
        );
    }
    for (name, kind, values) in [
        ("stage", FieldKind::Enum, vec!["approved"]),
        ("renewal", FieldKind::Date, vec![]),
        ("contact", FieldKind::Person, vec![]),
        ("note", FieldKind::Text, vec![]),
    ] {
        add_project_field_decl(root, decl(name, kind, &values)).unwrap();
    }
    let path = create_project_def(
        root,
        &NewProjectDef {
            name: "Bank".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let original = fs::read(&path).unwrap();
    for (name, value) in [
        ("stage", "bad"),
        ("renewal", "2026-02-30"),
        ("contact", "Bad Person"),
        ("note", ""),
        ("note", "line\nbreak"),
        ("missing", "x"),
    ] {
        assert!(edit_project_def(
            root,
            "Bank",
            &ProjectDefPatch {
                status: Some("Completed".into()),
                set_fields: vec![(name.into(), value.into())],
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    assert!(edit_project_def(
        root,
        "Bank",
        &ProjectDefPatch {
            set_fields: vec![("stage".into(), "approved".into())],
            unset_fields: vec!["stage".into()],
            ..Default::default()
        }
    )
    .is_err());
    assert!(edit_project_def(
        root,
        "Bank",
        &ProjectDefPatch {
            unset_fields: vec!["missing".into()],
            ..Default::default()
        }
    )
    .is_err());
    assert!(create_project_def(
        root,
        &NewProjectDef {
            name: "Invalid".into(),
            set_fields: vec![("stage".into(), "bad".into())],
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(load_backlog_repo(root).unwrap().project_defs.len(), 1);
}

#[test]
fn project_fields_schema_guards_include_completed_and_canceled_projects() {
    for status in ["Completed", "Canceled"] {
        let dir = fixture();
        let root = dir.path();
        add_project_field_decl(root, decl("stage", FieldKind::Enum, &["draft", "signed"])).unwrap();
        create_project_def(
            root,
            &NewProjectDef {
                name: "Bank".into(),
                status: status.into(),
                set_fields: vec![("stage".into(), "signed".into())],
                ..Default::default()
            },
        )
        .unwrap();
        let config = fs::read(root.join("backlog/config.yml")).unwrap();
        assert!(remove_project_field_decl(root, "stage")
            .unwrap_err()
            .to_string()
            .contains("Bank"));
        assert!(edit_project_field_decl(
            root,
            "stage",
            &FieldEditPatch {
                values: Some(vec!["draft".into()]),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(fs::read(root.join("backlog/config.yml")).unwrap(), config);
        edit_project_field_decl(
            root,
            "stage",
            &FieldEditPatch {
                values: Some(vec!["signed".into(), "draft".into(), "review".into()]),
                ..Default::default()
            },
        )
        .unwrap();
    }
}

#[test]
fn project_fields_snapshot_rejects_stale_edits_without_clobbering_other_changes() {
    let dir = fixture();
    let root = dir.path();
    add_project_field_decl(root, decl("stage", FieldKind::Text, &[])).unwrap();
    create_project_def(
        root,
        &NewProjectDef {
            name: "Bank".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let before = load_backlog_repo(root).unwrap().project_defs.remove(0);
    let _ = edit_project_def(
        root,
        "Bank",
        &ProjectDefPatch {
            lead: Some("other".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let patch = ProjectDefPatch {
        set_fields: vec![("stage".into(), "review".into())],
        ..Default::default()
    };
    assert!(edit_project_def_if_unchanged(root, "Bank", &patch, &before).is_err());
    let current = load_backlog_repo(root).unwrap().project_defs.remove(0);
    assert_eq!(current.lead.as_deref(), Some("other"));
    assert!(current.custom.is_empty());
    let _ = edit_project_def_if_unchanged(root, "Bank", &patch, &current).unwrap();
}

#[test]
fn project_fields_central_cutover_uses_documents_and_schema_guards_without_legacy_writes() {
    let dir = fixture();
    let root = dir.path();
    let db = root.join("state.sqlite3");
    with_test_database(&db, || {
        add_project_field_decl(root, decl("stage", FieldKind::Enum, &["draft", "signed"])).unwrap();
        let path = create_project_def(
            root,
            &NewProjectDef {
                name: "Bank".into(),
                set_fields: vec![("stage".into(), "draft".into())],
                ..Default::default()
            },
        )
        .unwrap();
        let original_project = fs::read(&path).unwrap();
        let original_config = fs::read(root.join("backlog/config.yml")).unwrap();
        let mut store = Store::open(&db).unwrap();
        let repo = store.bind_repository(root).unwrap();
        let sources = [
            ("project", "backlog/projects/Bank.md"),
            ("config", "backlog/config.yml"),
        ]
        .into_iter()
        .map(|(kind, locator)| SourceDocument {
            kind: kind.into(),
            locator: locator.into(),
            path: root.join(locator),
        })
        .collect();
        let plan =
            MigrationPlan::capture(repo, vec!["project".into(), "config".into()], sources).unwrap();
        store.apply_migration(&plan).unwrap();
        let _ = edit_project_def(
            root,
            "Bank",
            &ProjectDefPatch {
                status: Some("Completed".into()),
                set_fields: vec![("stage".into(), "signed".into())],
                ..Default::default()
            },
        )
        .unwrap();
        let after = load_backlog_repo(root).unwrap();
        assert_eq!(after.project_defs[0].custom["stage"], "signed");
        assert!(remove_project_field_decl(root, "stage").is_err());
        assert!(edit_project_field_decl(
            root,
            "stage",
            &FieldEditPatch {
                values: Some(vec!["draft".into()]),
                ..Default::default()
            }
        )
        .is_err());
        add_project_field_decl(root, decl("contact", FieldKind::Person, &[])).unwrap();
        assert_eq!(declared_project_fields(root).unwrap().len(), 2);
        assert_eq!(fs::read(&path).unwrap(), original_project);
        assert_eq!(
            fs::read(root.join("backlog/config.yml")).unwrap(),
            original_config
        );
    });
}

#[test]
fn project_fields_schema_mutation_refuses_loss_and_handles_commented_headers() {
    let dir = fixture();
    let root = dir.path();
    let path = root.join("backlog/config.yml");
    for text in [
        "statuses: []\nproject_fields: broken\n",
        "statuses: []\nproject_fields:\n  - name: stage\n    kind: unexpected\n",
        "statuses: []\nproject_fields:\n  - name: stage\n    kind: text\n    extension: retain\n",
        "statuses: [\n",
        "statuses: []\nproject_fields:\n  - name: stage\n    kind: enum\n    values: [valid, '']\n",
        "statuses: []\nproject_fields:\n  - name: lead\n    kind: text\n",
    ] {
        fs::write(&path, text).unwrap();
        assert!(add_project_field_decl(root, decl("contact", FieldKind::Person, &[])).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
    fs::write(&path, "statuses: []\nproject_fields: # project declarations\n  - name: stage\n    kind: text\n\n  # keep comment\n  - name: contact\n    kind: person\n# other configuration\nother: preserved\n").unwrap();
    add_project_field_decl(root, decl("renewal", FieldKind::Date, &[])).unwrap();
    assert_eq!(declared_project_fields(root).unwrap().len(), 3);
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("# keep comment"));
    assert!(after.contains("# other configuration"));
    assert!(after.contains("other: preserved"));
}

#[test]
fn project_fields_may_use_task_builtin_names_without_shadowing_tasks() {
    let dir = fixture();
    let root = dir.path();
    fs::write(root.join("backlog/tasks/TASK-1.md"), "---\nid: TASK-1\ntitle: Verify lender\nstatus: To Do\npriority: high\nproject: Bank\n---\n").unwrap();
    for name in ["priority", "due", "tasks", "project"] {
        add_project_field_decl(root, decl(name, FieldKind::Enum, &["critical", "normal"])).unwrap();
        assert!(super::super::field_config::add_field_decl(
            root,
            decl(name, FieldKind::Enum, &["critical", "normal"])
        )
        .is_err());
    }
    create_project_def(
        root,
        &NewProjectDef {
            name: "Bank".into(),
            set_fields: vec![
                ("priority".into(), "critical".into()),
                ("due".into(), "normal".into()),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    let repo = load_backlog_repo(root).unwrap();
    assert_eq!(repo.tasks[0].priority, "high");
    assert_eq!(repo.project_defs[0].custom["priority"], "critical");
    assert_eq!(repo.project_defs[0].custom["due"], "normal");
    assert_eq!(declared_project_fields(root).unwrap().len(), 4);
    let _ = edit_project_def(
        root,
        "Bank",
        &ProjectDefPatch {
            set_fields: vec![("priority".into(), "normal".into())],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        load_backlog_repo(root).unwrap().project_defs[0].custom["priority"],
        "normal"
    );
}
