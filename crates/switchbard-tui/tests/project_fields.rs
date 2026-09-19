//! Shared project values exercised through real keys and native storage.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use switchbard_core::{
    add_project_field_decl, edit_project_def, load_backlog_repo, FieldDecl, FieldKind,
    ProjectDefPatch,
};

fn fixture() -> Harness {
    let mut h = Harness::new();
    add_project_field_decl(
        &h.root,
        FieldDecl {
            name: "stage".into(),
            kind: FieldKind::Enum,
            values: vec!["Review".into(), "Approved".into()],
            groupable: true,
        },
    )
    .unwrap();
    add_project_field_decl(
        &h.root,
        FieldDecl {
            name: "note".into(),
            kind: FieldKind::Text,
            values: vec![],
            groupable: false,
        },
    )
    .unwrap();
    seed_project(&h.root, "Huntington", "In Progress", None);
    seed_in_project(&h.root, "First lender task", "To Do", "Huntington", None);
    seed_in_project(&h.root, "Second lender task", "To Do", "Huntington", None);
    let _ = edit_project_def(
        &h.root,
        "Huntington",
        &ProjectDefPatch {
            set_fields: vec![("stage".into(), "Review".into())],
            ..Default::default()
        },
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    select_task_titled(&mut h, "First lender task");
    h
}
fn editor(h: &mut Harness, field: &str) {
    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('f'));
    assert!(screen.contains("Shared project fields"), "{screen}");
    let label = h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .find(|option| option.label.starts_with(field))
        .unwrap()
        .label
        .clone();
    pick_labelled(h, &label);
}
#[test]
fn project_column_filter_and_shared_edit_leave_tasks_untouched() {
    let mut h = fixture();
    h.press(KeyCode::Char('c'));
    pick_labelled(&mut h, "project.stage");
    h.press(KeyCode::Esc);
    assert!(h.render().contains("Review"));
    editor(&mut h, "stage:");
    pick_labelled(&mut h, "Approved");
    h.tick_until_tasks_settle();
    let repo = load_backlog_repo(&h.root).unwrap();
    assert_eq!(repo.project_defs[0].custom["stage"], "Approved");
    assert!(repo.tasks.iter().all(|task| task.custom.is_empty()));
    h.press(KeyCode::Char('/'));
    for _ in 0..h.app.filter_text().len() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("project.stage:approved");
    h.press(KeyCode::Enter);
    assert_eq!(visible_titles(&h).len(), 2);
    assert!(h.render().contains("Approved"));
}
#[test]
fn project_edit_rejects_stale_and_keeps_other_changes() {
    let mut h = fixture();
    editor(&mut h, "stage:");
    let _ = edit_project_def(
        &h.root,
        "Huntington",
        &ProjectDefPatch {
            lead: Some("Sara".into()),
            ..Default::default()
        },
    )
    .unwrap();
    pick_labelled(&mut h, "Approved");
    assert!(h.app.status.contains("not saved"), "{}", h.app.status);
    let repo = load_backlog_repo(&h.root).unwrap();
    assert_eq!(repo.project_defs[0].custom["stage"], "Review");
    assert_eq!(repo.project_defs[0].lead.as_deref(), Some("Sara"));
}
#[test]
fn project_text_cancel_missing_project_and_narrow_render() {
    let mut h = fixture();
    editor(&mut h, "note:");
    h.type_text("Long Unicode 資金 value that remains scoped to the project");
    h.press(KeyCode::Esc);
    assert!(!load_backlog_repo(&h.root).unwrap().project_defs[0]
        .custom
        .contains_key("note"));
    editor(&mut h, "note:");
    h.type_text("Financing");
    h.press(KeyCode::Enter);
    h.tick_until_tasks_settle();
    assert_eq!(
        load_backlog_repo(&h.root).unwrap().project_defs[0].custom["note"],
        "Financing"
    );
    h.terminal.backend_mut().resize(40, 10);
    h.terminal
        .resize(ratatui::layout::Rect::new(0, 0, 40, 10))
        .unwrap();
    let screen = h.render();
    assert!(screen.contains("First lender"), "{screen}");
    h.press(KeyCode::Char('t'));
    let screen = h.press(KeyCode::Char('f'));
    assert!(screen.contains("Shared project fields"), "{screen}");
    assert!(screen.contains("stage: Review"), "{screen}");
    h.press(KeyCode::Esc);
    select_task_titled(&mut h, "Fix login redirect loop");
    h.press(KeyCode::Char('t'));
    h.press(KeyCode::Char('f'));
    assert!(h.app.status.contains("Link a project first"));
}

#[test]
fn project_sort_group_clear_and_same_named_task_field_are_independent() {
    let mut h = fixture();
    declare_field(&h.root, "stage", FieldKind::Text, &[], true);
    let id = load_backlog_repo(&h.root)
        .unwrap()
        .tasks
        .iter()
        .find(|task| task.title == "First lender task")
        .unwrap()
        .id
        .clone();
    let _ = switchbard_core::edit_backlog_task(
        &h.root,
        &id,
        &switchbard_core::BacklogTaskPatch {
            set_custom: vec![("stage".into(), "Task stage".into())],
            ..Default::default()
        },
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.press(KeyCode::Char('s'));
    pick_labelled(&mut h, "project.stage");
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Enter);
    assert_eq!(visible_titles(&h)[0], "First lender task");
    h.type_text(":group project.stage");
    h.press(KeyCode::Enter);
    assert!(h.render().contains("Review"));
    select_task_titled(&mut h, "First lender task");
    editor(&mut h, "stage:");
    pick_labelled(&mut h, "Clear value");
    h.tick_until_tasks_settle();
    let repo = load_backlog_repo(&h.root).unwrap();
    assert!(!repo.project_defs[0].custom.contains_key("stage"));
    assert_eq!(
        repo.tasks.iter().find(|task| task.id == id).unwrap().custom["stage"],
        "Task stage"
    );
}

#[test]
fn absent_schema_and_implicit_project_explain_recovery() {
    let mut h = Harness::new();
    seed_project(&h.root, "Defined", "Planned", None);
    seed_in_project(&h.root, "Defined task", "To Do", "Defined", None);
    seed_in_project(&h.root, "Implicit task", "To Do", "Implicit", None);
    h.app = open_app(&h.root, &h.config_path);
    select_task_titled(&mut h, "Defined task");
    h.type_text("tf");
    assert!(h.app.status.contains("No project fields declared"));
    select_task_titled(&mut h, "Implicit task");
    h.type_text("tf");
    assert!(h.app.status.contains("has no definition"));
}

#[test]
fn invalid_date_and_person_keep_draft_without_writes() {
    let mut h = fixture();
    for (name, kind, invalid) in [
        ("deadline", FieldKind::Date, "tomorrow"),
        ("owner", FieldKind::Person, "Not A Token"),
    ] {
        add_project_field_decl(
            &h.root,
            FieldDecl {
                name: name.into(),
                kind,
                values: vec![],
                groupable: false,
            },
        )
        .unwrap();
        h.app = open_app(&h.root, &h.config_path);
        select_task_titled(&mut h, "First lender task");
        editor(&mut h, &format!("{name}:"));
        h.type_text(invalid);
        h.press(KeyCode::Enter);
        assert!(h.app.status.contains("not saved"));
        assert_eq!(h.app.input, invalid);
        assert!(!load_backlog_repo(&h.root).unwrap().project_defs[0]
            .custom
            .contains_key(name));
        h.press(KeyCode::Esc);
    }
}

#[test]
fn migrated_project_edit_uses_database_and_leaves_source_file_unchanged() {
    const CHILD: &str = "SWITCHBARD_PROJECT_FIELDS_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "migrated_project_edit_uses_database_and_leaves_source_file_unchanged",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("SWITCHBARD_DATABASE", dir.path().join("state.sqlite3"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let mut h = fixture();
    let repo = load_backlog_repo(&h.root).unwrap();
    let path = repo.project_defs[0].path.clone();
    let original = std::fs::read(&path).unwrap();
    let mut store = switchbard_core::storage::Store::open_default().unwrap();
    let binding = store.bind_repository(&h.root).unwrap();
    let sources = vec![switchbard_core::storage::SourceDocument {
        kind: "project".into(),
        locator: path.strip_prefix(&h.root).unwrap().to_str().unwrap().into(),
        path: path.clone(),
    }];
    store
        .apply_migration(
            &switchbard_core::storage::MigrationPlan::capture(
                binding,
                vec!["project".into()],
                sources,
            )
            .unwrap(),
        )
        .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    select_task_titled(&mut h, "First lender task");
    editor(&mut h, "stage:");
    pick_labelled(&mut h, "Approved");
    h.tick_until_tasks_settle();
    assert_eq!(
        load_backlog_repo(&h.root).unwrap().project_defs[0].custom["stage"],
        "Approved"
    );
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn project_priority_is_distinct_from_task_priority_in_columns_and_filters() {
    let mut h = fixture();
    add_project_field_decl(
        &h.root,
        FieldDecl {
            name: "priority".into(),
            kind: FieldKind::Text,
            values: vec![],
            groupable: true,
        },
    )
    .unwrap();
    let _ = edit_project_def(
        &h.root,
        "Huntington",
        &ProjectDefPatch {
            set_fields: vec![("priority".into(), "Preferred".into())],
            ..Default::default()
        },
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.press(KeyCode::Char('c'));
    pick_labelled(&mut h, "project.priority");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.type_text("project.priority:preferred pri:medium");
    h.press(KeyCode::Enter);
    assert_eq!(
        visible_titles(&h),
        ["First lender task", "Second lender task"]
    );
    assert!(h.render().contains("Preferred"));
    assert!(load_backlog_repo(&h.root)
        .unwrap()
        .tasks
        .iter()
        .filter(|task| task.project.as_deref() == Some("Huntington"))
        .all(|task| task.priority == "medium"));
}
