//! Project outline over authoritative migrated planning data, driven through real keys.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use switchbard_core::storage::{MigrationPlan, SourceDocument, Store};
use switchbard_core::{
    apply_planning_migration, edit_backlog_task, load_backlog_repo, prepare_planning_migration,
    set_task_planning, BacklogTaskPatch, PlanningState,
};

fn migrate(h: &Harness) {
    let mut store = Store::open_default().unwrap();
    let repo = store.bind_repository(&h.root).unwrap();
    let mut sources: Vec<_> = load_backlog_repo(&h.root)
        .unwrap()
        .tasks
        .iter()
        .map(|task| SourceDocument {
            kind: "task".into(),
            locator: task
                .path
                .strip_prefix(&h.root)
                .unwrap()
                .to_str()
                .unwrap()
                .into(),
            path: task.path.clone(),
        })
        .collect();
    for (kind, locator) in [
        ("config", "backlog/config.yml"),
        ("ranking", "backlog/ranking.yml"),
    ] {
        if h.root.join(locator).exists() {
            sources.push(SourceDocument {
                kind: kind.into(),
                locator: locator.into(),
                path: h.root.join(locator),
            });
        }
    }
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
    let preview = prepare_planning_migration(&h.root).unwrap();
    apply_planning_migration(&h.root, &preview, &h.root.join("migration-backup")).unwrap();
}

fn fixture() -> Harness {
    let mut h = Harness::new();
    seed_project(&h.root, "Atlas", "In Progress", None);
    seed_project(&h.root, "Beacon", "Planned", None);
    for (id, project) in [
        ("TASK-1", "Atlas"),
        ("TASK-2", "Beacon"),
        ("TASK-3", "Atlas"),
    ] {
        edit_backlog_task(
            &h.root,
            id,
            &BacklogTaskPatch {
                project: Some(project.into()),
                ..Default::default()
            },
        )
        .unwrap();
        let _ = set_task_planning(&h.root, id, PlanningState::Planned).unwrap();
    }
    migrate(&h);
    h.app = open_app(&h.root, &h.config_path);
    h
}

fn command(h: &mut Harness, text: &str) -> String {
    h.press(KeyCode::Char(':'));
    h.type_text(text);
    h.press(KeyCode::Enter)
}

fn isolated(name: &str) -> bool {
    if std::env::var_os("SWITCHBARD_PLANNING_OUTLINE_CHILD").is_some() {
        return false;
    }
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("SWITCHBARD_PLANNING_OUTLINE_CHILD", "1")
        .env("SWITCHBARD_DATABASE", dir.path().join("state.sqlite3"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

fn filter(h: &mut Harness, query: &str) -> String {
    h.press(KeyCode::Char('/'));
    for _ in 0..h.app.filter_text().len() {
        h.press(KeyCode::Backspace);
    }
    h.type_text(query);
    h.press(KeyCode::Enter)
}

fn task_ids(h: &Harness) -> Vec<String> {
    h.app
        .rows
        .iter()
        .enumerate()
        .filter_map(|(row, _)| h.app.task(row).map(|task| task.id.clone()))
        .collect()
}

fn snapshot(h: &Harness) -> String {
    let repo = load_backlog_repo(&h.root).unwrap();
    format!("{:?} {:?}", repo.tasks, repo.ranking)
}

#[test]
fn migrated_planned_tasks_honor_project_outline() {
    if isolated("migrated_planned_tasks_honor_project_outline") {
        return;
    }
    let mut h = fixture();
    let before = snapshot(&h);
    let planned = h.app.top.clone();
    let screen = command(&mut h, "outline project");
    assert!(
        screen.contains("Atlas ·"),
        "project headings must appear for Planned tasks: {screen}"
    );
    assert!(screen.contains("Beacon ·"), "{screen}");
    assert_eq!(
        screen_rows(&h),
        [
            "# Planned · 3",
            "  # Atlas · In Progress · 0/2",
            "Fix login redirect loop",
            "Write onboarding guide",
            "  # Beacon · Planned · 0/1",
            "Add dark theme"
        ]
    );
    assert_eq!(h.app.top, planned);
    h.press(KeyCode::Char('G'));
    let selected = h.app.selected_task().unwrap().id.clone();
    let mut displayed = task_ids(&h);
    displayed.sort();
    let mut expected = planned.clone();
    expected.sort();
    assert_eq!(displayed, expected, "outline loses or duplicates no task");
    command(&mut h, "paint heading:Atlas=green");
    assert_eq!(cell_fg(&h, "Atlas ·"), Some(ratatui::style::Color::Green));
    for _ in 0..2 {
        command(&mut h, "outline off");
        assert_eq!(task_ids(&h), planned, "flat Planned order must be exact");
        assert_eq!(h.app.selected_task().unwrap().id, selected);
        command(&mut h, "outline project");
        assert_eq!(h.app.selected_task().unwrap().id, selected);
    }
    command(&mut h, "outline planning");
    let rows = screen_rows(&h);
    assert_eq!(
        rows.iter().filter(|row| row.contains("# Planned")).count(),
        1,
        "{rows:?}"
    );
    assert_eq!(
        snapshot(&h),
        before,
        "outline changes must not mutate tasks/ranking"
    );
}

#[test]
fn mixed_planning_keeps_groups_missing_projects_filters_and_navigation() {
    if isolated("mixed_planning_keeps_groups_missing_projects_filters_and_navigation") {
        return;
    }
    let mut h = fixture();
    seed_in_project(&h.root, "Investigate Beacon", "Not started", "Beacon", None);
    seed(&h.root, "Unassigned work", "Not started", &[]);
    let _ = set_task_planning(&h.root, "TASK-4", PlanningState::Considering).unwrap();
    let _ = set_task_planning(&h.root, "TASK-5", PlanningState::Planned).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    let before = snapshot(&h);
    let screen = command(&mut h, "outline project");
    assert!(screen.contains("Other tasks"), "{screen}");
    let rows = screen_rows(&h);
    assert_eq!(
        rows.iter().filter(|row| row.contains("# Beacon ·")).count(),
        2,
        "{rows:?}"
    );
    let unassigned = rows.iter().position(|row| row == "  # no project").unwrap();
    assert_eq!(rows[unassigned + 1], "Unassigned work");
    assert_eq!(rows[0], "# Planned · 4");
    let other = rows.iter().position(|row| row == "# Other tasks").unwrap();
    assert_eq!(rows[other + 1], "  # Beacon · Planned · 0/2");
    let mut displayed = task_ids(&h);
    displayed.sort();
    assert_eq!(
        displayed,
        ["TASK-1", "TASK-2", "TASK-3", "TASK-4", "TASK-5"]
    );
    command(&mut h, "outline planning,project");
    let nested = screen_rows(&h);
    assert_eq!(
        nested
            .iter()
            .filter(|row| row.contains("# Planned"))
            .count(),
        1,
        "{nested:?}"
    );
    assert!(
        nested
            .iter()
            .any(|row| row == "  # Atlas · In Progress · 0/2"),
        "{nested:?}"
    );
    assert!(
        nested
            .iter()
            .any(|row| row == "    # Beacon · Planned · 0/2"),
        "{nested:?}"
    );
    command(&mut h, "outline project");
    let screen = filter(&mut h, "planning:considering");
    assert!(
        screen.contains("Beacon ·") && screen.contains("Investigate Beacon"),
        "{screen}"
    );
    assert!(!screen.contains("Planned · 4"), "{screen}");
    let screen = filter(&mut h, "title:never-matches");
    assert!(h.app.rows.is_empty(), "{screen}");
    filter(&mut h, "");
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(46, 10)).unwrap();
    let screen = h.render();
    assert!(screen.contains("Atlas"), "{screen}");
    h.press(KeyCode::Char('G'));
    assert_eq!(h.selected_title(), "Investigate Beacon");
    assert!(h.render().contains("Beacon"));
    h.press(KeyCode::Char('g'));
    assert_eq!(h.selected_title(), "Fix login redirect loop");
    h.type_text("vp");
    assert!(!screen_rows(&h)
        .iter()
        .any(|row| row.starts_with("# Planned")));
    assert_eq!(
        snapshot(&h),
        before,
        "projection and navigation are read-only"
    );
}

#[test]
fn legacy_expedite_stays_flat_above_project_groups() {
    let mut h = Harness::new();
    seed_project(&h.root, "Atlas", "In Progress", None);
    seed_in_project(&h.root, "Atlas task", "To Do", "Atlas", None);
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h.type_text("t1");
    let expedited = h.selected_title();
    command(&mut h, "outline project");
    let rows = screen_rows(&h);
    assert_eq!(rows[0], "# top · 1");
    assert_eq!(rows[1], expedited);
    assert!(
        rows.iter().any(|row| row.starts_with("# Atlas ·")),
        "{rows:?}"
    );
}
