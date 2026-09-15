//! Planning, criterion coverage and execution remain independent on real task files.
mod harness;
use crossterm::event::KeyCode;
use harness::{open_app, select_task_titled, Harness};
use ratatui::{backend::TestBackend, Terminal};
use switchbard_core::{
    edit_backlog_task, load_backlog_repo, set_task_planning, BacklogTaskPatch, PlanningState,
};
use switchbard_tui::{app::Mode, columns::Column};

fn set_planning(h: &mut Harness, value: &str) {
    h.type_text("tl");
    h.type_text(value);
    h.press(KeyCode::Enter);
}
fn fresh_defaults(h: &mut Harness) {
    std::fs::remove_file(h.root.join("views.lua")).unwrap();
    h.app = open_app(&h.root, &h.config_path);
}
#[test]
fn default_columns_show_planning_and_checklist_and_saved_columns_survive() {
    let mut h = Harness::new();
    assert_eq!(
        h.app.state.columns,
        vec![Column::Id, Column::Status, Column::Priority, Column::Title]
    );
    fresh_defaults(&mut h);
    let screen = h.render();
    assert!(
        screen.contains("planning") && screen.contains("checklist"),
        "{screen}"
    );
    assert!(h.app.state.columns.contains(&Column::Priority));
    assert!(screen.contains("0/1 0.0%"), "{screen}");
    h.terminal = Terminal::new(TestBackend::new(50, 12)).unwrap();
    let screen = h.render();
    assert!(!screen.is_empty());
}
#[test]
fn planning_picker_changes_only_planning_and_refreshes_planned_order() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let original = h.app.selected_task().unwrap().clone();
    set_planning(&mut h, "Planned");
    let screen = h.render();
    assert!(screen.contains("Planned"), "{screen}");
    let repo = load_backlog_repo(&h.root).unwrap();
    let task = repo.tasks.iter().find(|t| t.id == original.id).unwrap();
    assert_eq!(task.planning, PlanningState::Planned);
    assert_eq!(task.status, original.status);
    assert_eq!(task.acceptance_criteria, original.acceptance_criteria);
    assert!(h.app.top.contains(&original.id));
    h.type_text("trx");
    let repo = load_backlog_repo(&h.root).unwrap();
    let task = repo.tasks.iter().find(|t| t.id == original.id).unwrap();
    assert_eq!(task.planning, PlanningState::Considering);
    assert_eq!(task.status, original.status);
    assert!(!h.app.top.contains(&original.id));
    h.app = open_app(&h.root, &h.config_path);
    select_task_titled(&mut h, "Add dark theme");
    assert_eq!(
        h.app.selected_task().unwrap().planning,
        PlanningState::Considering
    );
}
#[test]
fn completed_checklist_shows_review_without_completing_and_zero_is_unmeasured() {
    let mut h = Harness::new();
    fresh_defaults(&mut h);
    let tasks = load_backlog_repo(&h.root).unwrap().tasks;
    for task in &tasks {
        if task.id == "TASK-1" {
            switchbard_core::set_backlog_acceptance_checked(&h.root, &task.id, 1, true).unwrap();
        } else if task.id == "TASK-2" {
            edit_backlog_task(
                &h.root,
                &task.id,
                &BacklogTaskPatch {
                    status: Some("Done".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        } else {
            switchbard_core::revise_backlog_acceptance_criteria(&h.root, &task.id, &[], &[1])
                .unwrap();
        }
    }
    h.press(KeyCode::Char('r'));
    let screen = h.render();
    assert!(screen.contains("Review 1/1 100%"), "{screen}");
    assert!(screen.contains("Unmeasured"), "{screen}");
    let repo = load_backlog_repo(&h.root).unwrap();
    assert_eq!(
        repo.tasks.iter().find(|t| t.id == "TASK-1").unwrap().status,
        "In Progress"
    );
    select_task_titled(&mut h, "Add dark theme");
    assert_eq!(h.app.selected_task().unwrap().status, "Done");
    assert_eq!(
        h.app
            .cell(Column::Checklist, h.app.selected_task().unwrap()),
        "0/1 0.0%"
    );
}
#[test]
fn detail_planning_uses_snapshot_guard_and_returns_to_detail_focus() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    let planning = 6;
    for _ in 0..planning {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
    let id = h.app.selected_task().unwrap().id.clone();
    edit_backlog_task(
        &h.root,
        &id,
        &BacklogTaskPatch {
            title: Some("Changed elsewhere".into()),
            ..Default::default()
        },
    )
    .unwrap();
    h.type_text("Planned");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("changed"), "{screen}");
    assert_eq!(
        load_backlog_repo(&h.root)
            .unwrap()
            .tasks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .planning,
        PlanningState::Considering
    );
    assert_eq!(h.app.mode, Mode::DetailFocus);
}
#[test]
fn planned_filter_group_and_order_work_independently_of_status() {
    let mut h = Harness::new();
    let _ = set_task_planning(&h.root, "TASK-1", PlanningState::Considering).unwrap();
    let _ = set_task_planning(&h.root, "TASK-2", PlanningState::Planned).unwrap();
    let _ = set_task_planning(&h.root, "TASK-3", PlanningState::Planned).unwrap();
    h.press(KeyCode::Char('r'));
    select_task_titled(&mut h, "Write onboarding guide");
    h.type_text("tr1");
    assert_eq!(h.app.top, vec!["TASK-3", "TASK-2"]);
    h.type_text("tra");
    assert_eq!(h.app.top, vec!["TASK-2", "TASK-3"]);
    assert!(h.app.status.contains("#2 of 2"), "{}", h.app.status);

    h.press(KeyCode::Char('/'));
    for _ in 0..h.app.filter_text().len() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("planning:considering");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("Fix login"), "{screen}");
    assert!(!screen.contains("Add dark theme"), "{screen}");
    h.press(KeyCode::Char('/'));
    h.press(KeyCode::Esc);
    h.app.state.filter.clear();
    h.press(KeyCode::Char(':'));
    h.type_text("outline planning");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("Considering"), "{screen}");
    assert!(screen.contains("Planned"), "{screen}");
}

#[test]
fn descendant_criteria_are_weighted_individually_and_detail_edits_keep_execution_status() {
    let mut h = Harness::new();
    harness::seed_project(&h.root, "Coverage", "In Progress", None);
    switchbard_core::revise_backlog_acceptance_criteria(&h.root, "TASK-1", &[], &[1]).unwrap();
    for title in ["Four checks A", "Four checks B"] {
        harness::seed_in_project(&h.root, title, "To Do", "Coverage", Some("TASK-1"));
        let repo = load_backlog_repo(&h.root).unwrap();
        let child = repo.tasks.iter().find(|t| t.title == title).unwrap();
        edit_backlog_task(
            &h.root,
            &child.id,
            &BacklogTaskPatch {
                append_acceptance_criteria: vec!["Second".into(), "Third".into(), "Fourth".into()],
                ..Default::default()
            },
        )
        .unwrap();
        if title.ends_with('A') {
            switchbard_core::set_backlog_acceptance_checked(&h.root, &child.id, 1, true).unwrap();
        }
    }
    h.press(KeyCode::Char('r'));
    select_task_titled(&mut h, "Fix login redirect loop");
    assert_eq!(
        h.app
            .cell(Column::Checklist, h.app.selected_task().unwrap()),
        "1/8 12.5%"
    );
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..6 {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
    h.type_text("Considering");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("execution status unchanged"), "{screen}");
    let repo = load_backlog_repo(&h.root).unwrap();
    let parent = repo.tasks.iter().find(|t| t.id == "TASK-1").unwrap();
    assert_eq!(parent.planning, PlanningState::Considering);
    assert_eq!(parent.status, "In Progress");
    assert_eq!(h.app.cell(Column::Checklist, parent), "1/8 12.5%");
}

#[test]
fn status_choices_do_not_bypass_cancellation_and_empty_filter_stays_usable() {
    let mut h = Harness::new();
    let path = h.root.join("backlog/config.yml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, text.replace("\"Done\"]", "\"Done\", \"Canceled\"]")).unwrap();
    h.type_text("ts");
    let screen = h.render();
    assert!(!screen.contains("Canceled"), "{screen}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.type_text("planning:unknown");
    h.press(KeyCode::Enter);
    assert!(h.app.selected_task().is_none());
    h.terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    h.type_text("tl");
    assert!(!h.render().is_empty());
}

#[test]
fn planning_sort_is_available_from_hidden_column_and_survives_saved_view_reload() {
    let mut h = Harness::new();
    h.type_text("spl");
    h.press(KeyCode::Char('a'));
    assert_eq!(h.app.state.sort.unwrap().column, Column::Planning);
    let rows = harness::screen_rows(&h);
    assert_eq!(rows.last().unwrap(), "Fix login redirect loop");
    h.type_text("vs1");
    h.app = open_app(&h.root, &h.config_path);
    assert_eq!(h.app.state.sort.unwrap().column, Column::Planning);
    let screen = h.render();
    assert!(screen.contains("planning"), "{screen}");
}
