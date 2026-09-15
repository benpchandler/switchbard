//! TASK-222: the focusable, editable task detail pane. See
//! `docs/tui-detail-edit-evidence.md` for the full state-matrix mapping.

mod harness;

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use harness::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use switchbard_tui::app::{DetailInputKind, Mode, Pane};
use switchbard_tui::page::Page;

/// Row indices for the default one-acceptance-item fixture, in the pane's
/// fixed order: Title, Status, Priority, Project, Due date, Labels,
/// Description, then acceptance items.
const TITLE: usize = 0;
const STATUS: usize = 1;
const PRIORITY: usize = 2;
const PROJECT: usize = 3;
const DUE_DATE: usize = 4;
const LABELS: usize = 5;
const DESCRIPTION: usize = 6;
const FIRST_ACCEPTANCE: usize = 7;

#[test]
fn second_open_or_right_focuses_the_pane() {
    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::Browse);
    assert_eq!(h.app.pane, Pane::Detail);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::DetailFocus);

    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('l'));
    assert_eq!(h.app.mode, Mode::DetailFocus);

    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Right);
    assert_eq!(h.app.mode, Mode::DetailFocus);
}

#[test]
fn esc_unfocuses_then_a_second_back_closes_the_pane() {
    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::DetailFocus);
    h.press(KeyCode::Esc);
    assert_eq!(h.app.mode, Mode::Browse);
    assert_eq!(
        h.app.pane,
        Pane::Detail,
        "the pane stays open, just unfocused"
    );
    h.press(KeyCode::Esc);
    assert_eq!(h.app.pane, Pane::None);
}

#[test]
fn j_k_move_the_cursor_without_moving_list_selection() {
    let mut h = Harness::new();
    let before = h.selected_title();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.detail_cursor, PRIORITY);
    assert_eq!(
        h.selected_title(),
        before,
        "list selection must not move while the pane is focused"
    );
    h.press(KeyCode::Char('k'));
    assert_eq!(h.app.detail_cursor, STATUS);
}

#[test]
fn cursor_highlight_follows_j_and_k() {
    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    let title_row_bg = cell_bg(&h, "status:");
    h.press(KeyCode::Char('j'));
    let status_row_bg = cell_bg(&h, "status:");
    assert_ne!(
        title_row_bg, status_row_bg,
        "moving the cursor onto the status row should change its highlight"
    );
}

#[test]
fn title_edit_prefills_saves_and_round_trips_non_ascii() {
    let mut h = Harness::new();
    let title = "Résumé café 日本語";
    seed(&h.root, title, "To Do", &[]);
    h.app.tick();
    select_task_titled(&mut h, title);
    let id = h.app.selected_task().unwrap().id.clone();
    // Filter by id instead of the title text about to change: a title
    // filter would otherwise hide the very row being renamed the moment the
    // save lands, since "Résumé café 日本語" no longer matches.
    h.press(KeyCode::Char('/'));
    for _ in 0..h.app.filter_text().chars().count() {
        h.press(KeyCode::Backspace);
    }
    h.type_text(&format!("id:{id}"));
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.selected_task().map(|task| task.id.as_str()),
        Some(id.as_str())
    );

    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::DetailInput(DetailInputKind::Title));
    assert_eq!(h.app.input, title);
    for _ in 0..title.chars().count() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("Ünïcödé ✓ done");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("title saved"), "{screen}");
    assert_eq!(h.app.mode, Mode::DetailFocus);
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().title,
        "Ünïcödé ✓ done"
    );
}

#[test]
fn esc_during_an_edit_keeps_the_original_value() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.type_text(" mutated");
    let screen = h.press(KeyCode::Esc);
    assert!(screen.contains("edit cancelled"), "{screen}");
    assert_eq!(h.app.mode, Mode::DetailFocus);
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().title,
        "Add dark theme"
    );
}

#[test]
fn due_date_validates_clears_and_rejects_garbage() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..DUE_DATE {
        h.press(KeyCode::Char('j'));
    }
    assert_eq!(h.app.detail_cursor, DUE_DATE);
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.input, "",
        "no due date yet, so the input starts empty"
    );
    h.type_text("2026-13-40");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("due date must be"), "{screen}");
    assert!(h
        .app
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .due_date
        .is_none());
    assert_eq!(h.app.mode, Mode::DetailInput(DetailInputKind::DueDate));

    for _ in 0.."2026-13-40".len() {
        h.press(KeyCode::Backspace);
    }
    h.type_text("2026-10-05");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .due_date
            .as_deref(),
        Some("2026-10-05")
    );

    h.press(KeyCode::Enter);
    assert_eq!(h.app.input, "2026-10-05");
    for _ in 0.."2026-10-05".len() {
        h.press(KeyCode::Backspace);
    }
    h.press(KeyCode::Enter);
    assert!(h
        .app
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .due_date
        .is_none());
}

#[test]
fn status_priority_and_project_pickers_write_through_the_native_layer() {
    let mut h = Harness::new();
    seed_project(&h.root, "Delivery", "In Progress", None);
    h.app.tick();
    select_task_titled(&mut h, "Add dark theme");
    let id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);

    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.detail_cursor, STATUS);
    h.press(KeyCode::Enter);
    h.type_text("Done");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::DetailFocus);
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().status,
        "Done"
    );

    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.detail_cursor, PRIORITY);
    h.press(KeyCode::Enter);
    h.type_text("high");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().priority,
        "high"
    );

    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.detail_cursor, PROJECT);
    h.press(KeyCode::Enter);
    h.type_text("Delivery");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .project
            .as_deref(),
        Some("Delivery")
    );
}

#[test]
fn labels_multi_select_toggles_and_a_new_label_can_be_added() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Fix login redirect loop");
    let id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..LABELS {
        h.press(KeyCode::Char('j'));
    }
    assert_eq!(h.app.detail_cursor, LABELS);
    h.press(KeyCode::Enter);
    h.type_text("auth");
    h.press(KeyCode::Enter);
    assert!(!h
        .app
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .labels
        .iter()
        .any(|l| l == "auth"));
    assert_eq!(h.app.mode, Mode::PickValue, "the labels panel stays open");

    h.press(KeyCode::Char('n'));
    assert_eq!(h.app.mode, Mode::DetailInput(DetailInputKind::NewLabel));
    h.type_text("urgent");
    h.press(KeyCode::Enter);
    assert!(h
        .app
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .labels
        .iter()
        .any(|l| l == "urgent"));

    h.press(KeyCode::Esc);
    assert_eq!(h.app.mode, Mode::DetailFocus);
}

#[test]
fn esc_from_a_new_label_capture_returns_to_the_labels_picker() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Fix login redirect loop");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..LABELS {
        h.press(KeyCode::Char('j'));
    }
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('n'));
    assert_eq!(h.app.mode, Mode::DetailInput(DetailInputKind::NewLabel));
    h.type_text("partial");
    h.press(KeyCode::Esc);
    assert_eq!(
        h.app.mode,
        Mode::PickValue,
        "canceling a new label should return to the labels picker, not the pane's own cursor \
         -- the same place a successful add already reopens into"
    );
    assert!(h.app.input.is_empty());
    assert!(matches!(
        h.app.picker.as_ref().map(|picker| picker.purpose.clone()),
        Some(switchbard_tui::picker::PickerPurpose::DetailLabels(_))
    ));
}

#[test]
fn space_and_enter_toggle_an_acceptance_row() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..FIRST_ACCEPTANCE {
        h.press(KeyCode::Char('j'));
    }
    assert_eq!(h.app.detail_cursor, FIRST_ACCEPTANCE);

    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("[x] It works"), "{screen}");
    assert!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .acceptance_criteria[0]
            .checked
    );

    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("[ ] It works"), "{screen}");
    assert!(
        !h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .acceptance_criteria[0]
            .checked
    );
}

#[test]
fn stale_draft_blocks_an_acceptance_toggle_and_leaves_the_file_untouched() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let path = h.app.selected_task().unwrap().path.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..FIRST_ACCEPTANCE {
        h.press(KeyCode::Char('j'));
    }
    // Space opens and commits an acceptance toggle in one keystroke, so the
    // only window for an external writer to land a change is between
    // focus (the snapshot) and this press -- there is no separate "editor
    // open" step to snapshot at, the way there is for every other field.
    std::fs::write(&path, "tampered content, not a valid task file\n").unwrap();
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("changed on disk"), "{screen}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "tampered content, not a valid task file\n",
        "the stale toggle must not have touched the file"
    );
}

#[test]
fn stale_draft_blocks_a_picker_driven_field_pick() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let path = h.app.selected_task().unwrap().path.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.detail_cursor, STATUS);
    h.press(KeyCode::Enter);
    std::fs::write(&path, "tampered content, not a valid task file\n").unwrap();
    h.type_text("Done");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("changed on disk"), "{screen}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "tampered content, not a valid task file\n",
        "the stale pick must not have touched the file"
    );
}

#[test]
fn background_reload_refreshes_the_stale_guard_while_cursor_focused() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..FIRST_ACCEPTANCE {
        h.press(KeyCode::Char('j'));
    }
    assert_eq!(h.app.detail_cursor, FIRST_ACCEPTANCE);

    // An external writer (`sb edit` in another terminal, an agent) lands a
    // real, valid edit while the pane is focused but no editor/picker is
    // open. `Harness::new()` never calls `tick()` itself, so this is this
    // app's first call and bypasses the once-a-second reload throttle the
    // way the app's own real event loop would after enough wall-clock time.
    switchbard_core::edit_backlog_task(
        &h.root,
        &id,
        &switchbard_core::BacklogTaskPatch {
            priority: Some("high".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    h.app.tick();
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().priority,
        "high",
        "the reload should have picked up the external edit"
    );
    assert_eq!(
        h.app.mode,
        Mode::DetailFocus,
        "cursor focus must survive a background reload"
    );

    // Before the fix, this would refuse with "changed on disk" because the
    // snapshot taken at focus-entry still predated the external edit above.
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("[x] It works"), "{screen}");
    assert!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .acceptance_criteria[0]
            .checked,
        "the toggle should land once the snapshot is refreshed on reload"
    );
}

#[test]
fn read_only_task_shows_fields_but_refuses_every_edit() {
    let mut h = Harness::new();
    seed(&h.root, "Migrated into drafts", "To Do", &[]);
    h.app.tick();
    let task = h
        .app
        .tasks()
        .iter()
        .find(|t| t.title == "Migrated into drafts")
        .unwrap()
        .clone();
    let drafts = h.root.join("backlog/drafts");
    std::fs::create_dir_all(&drafts).unwrap();
    std::fs::rename(&task.path, drafts.join(task.path.file_name().unwrap())).unwrap();
    // `tick()` throttles its own reload check to once a second; force an
    // immediate one the same way a user would (`r`), since this is the
    // second reload this test needs within the same instant.
    h.press(KeyCode::Char('r'));

    select_task_titled(&mut h, "Migrated into drafts");
    let id = h.app.selected_task().unwrap().id.clone();
    assert!(!h.app.selected_task().unwrap().editable());

    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("· read-only"), "{screen}");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::DetailFocus);

    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("read-only"), "{screen}");
    assert_eq!(h.app.mode, Mode::DetailFocus);
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().title,
        "Migrated into drafts"
    );

    for _ in 0..FIRST_ACCEPTANCE {
        h.press(KeyCode::Char('j'));
    }
    h.press(KeyCode::Char(' '));
    assert!(h
        .app
        .tasks()
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .acceptance_criteria[0]
        .checked
        .eq(&false));
}

#[test]
fn nothing_selected_makes_the_focus_gesture_a_no_op() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("zzz-does-not-match-anything-zzz");
    h.press(KeyCode::Enter);
    assert!(h.app.selected_task().is_none());

    h.press(KeyCode::Enter);
    assert_eq!(h.app.pane, Pane::Detail);
    let screen = h.render();
    assert!(screen.contains("nothing selected"), "{screen}");

    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::Browse, "no task to focus onto");
    assert!(h.app.status.contains("nothing selected"));
}

#[test]
fn stale_draft_on_disk_fails_the_save_and_keeps_the_typed_input() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let path = h.app.selected_task().unwrap().path.clone();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.input, "Add dark theme");

    // An external writer (another sbt session, `sb edit`, a script) lands a
    // change on disk between the edit opening and the save committing.
    std::fs::write(&path, "tampered content, not a valid task file\n").unwrap();

    h.type_text(" (typed)");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("changed on disk"), "{screen}");
    assert_eq!(h.app.mode, Mode::DetailInput(DetailInputKind::Title));
    assert_eq!(h.app.input, "Add dark theme (typed)");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "tampered content, not a valid task file\n",
        "the stale save must not have touched the file"
    );
}

#[test]
fn tab_while_focused_or_mid_edit_switches_page_and_cancels() {
    let mut h = Harness::new();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.page, Page::PullRequests);
    assert_eq!(h.app.pane, Pane::None);
    assert_eq!(h.app.mode, Mode::Browse);

    while h.app.page != Page::Tasks {
        h.press(KeyCode::Tab);
    }
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.type_text("half typed");
    h.press(KeyCode::Tab);
    assert_ne!(h.app.page, Page::Tasks);
    assert_eq!(h.app.pane, Pane::None);
    assert!(h.app.input.is_empty());

    while h.app.page != Page::Tasks {
        h.press(KeyCode::Tab);
    }
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Enter);
    assert!(h.app.picker.is_some());
    h.press(KeyCode::Tab);
    assert!(h.app.picker.is_none());
    assert_eq!(h.app.pane, Pane::None);
}

#[test]
fn filter_that_hides_the_selected_task_updates_the_unfocused_pane_live() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("Add dark theme"), "{screen}");
    assert_eq!(h.app.mode, Mode::Browse, "the pane is open but not focused");

    select_task_titled(&mut h, "Fix login redirect loop");
    let screen = h.render();
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
}

#[test]
fn zero_and_many_labels_render_as_none_or_a_joined_list() {
    let mut h = Harness::new();
    seed(&h.root, "No labels here", "To Do", &[]);
    seed(
        &h.root,
        "Many labels here",
        "To Do",
        &["a", "b", "c", "d", "e"],
    );
    h.app.tick();

    select_task_titled(&mut h, "No labels here");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("labels: Not set"), "{screen}");
    h.press(KeyCode::Esc);

    select_task_titled(&mut h, "Many labels here");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("labels: a, b, c, d, e"), "{screen}");
}

#[test]
fn empty_project_and_due_date_show_placeholders() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("project: Not set"), "{screen}");
    assert!(screen.contains("due date: Not set"), "{screen}");
}

#[test]
fn many_acceptance_items_scroll_the_cursor_into_view() {
    let mut h = Harness::new();
    let acceptance_criteria: Vec<String> =
        (1..=35).map(|n| format!("Acceptance number {n}")).collect();
    let task = switchbard_core::NewBacklogTask {
        title: "Many AC task".to_string(),
        description: String::new(),
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria,
        parent: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        project: None,
        dependencies: Vec::new(),
        due_date: None,
        custom: Vec::new(),
    };
    switchbard_core::create_task_allocating_id(&h.root, &task).unwrap();
    h.app.tick();

    select_task_titled(&mut h, "Many AC task");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    let last_acceptance_row = FIRST_ACCEPTANCE + 34;
    let mut screen = String::new();
    for _ in 0..last_acceptance_row {
        screen = h.press(KeyCode::Char('j'));
    }
    assert_eq!(h.app.detail_cursor, last_acceptance_row);
    assert!(screen.contains("Acceptance number 35"), "{screen}");
}

#[test]
fn cursor_stays_visible_after_resize() {
    let mut h = Harness::new();
    let acceptance_criteria: Vec<String> =
        (1..=35).map(|n| format!("Acceptance number {n}")).collect();
    let task = switchbard_core::NewBacklogTask {
        title: "Many AC task".to_string(),
        description: String::new(),
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria,
        parent: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        project: None,
        dependencies: Vec::new(),
        due_date: None,
        custom: Vec::new(),
    };
    switchbard_core::create_task_allocating_id(&h.root, &task).unwrap();
    h.app.tick();

    select_task_titled(&mut h, "Many AC task");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..(FIRST_ACCEPTANCE + 34) {
        h.press(KeyCode::Char('j'));
    }

    h.terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
    let screen = h.render();
    assert!(
        screen.contains("Acceptance number 35"),
        "cursor row should still be on screen after shrinking: {screen}"
    );
}

#[test]
fn long_description_pages_into_the_acceptance_section_without_moving_cursor_or_selection() {
    let mut h = Harness::new();
    let description: String = (1..=120).map(|n| format!("Detail line {n:03}\n")).collect();
    let task = switchbard_core::NewBacklogTask {
        title: "Long description task".to_string(),
        description,
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria: vec!["End of long description".to_string()],
        parent: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        project: None,
        dependencies: Vec::new(),
        due_date: None,
        custom: Vec::new(),
    };
    switchbard_core::create_task_allocating_id(&h.root, &task).unwrap();
    h.app.tick();

    select_task_titled(&mut h, "Long description task");
    let selected = h.selected_title();
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    for _ in 0..DESCRIPTION {
        h.press(KeyCode::Char('j'));
    }
    assert_eq!(h.app.detail_cursor, DESCRIPTION);
    let screen = h.render();
    assert!(screen.contains("Detail line 001"), "{screen}");

    let mut screen = String::new();
    for _ in 0..30 {
        screen = h.press(KeyCode::PageDown);
        if screen.contains("End of long description") {
            break;
        }
    }
    assert!(screen.contains("Acceptance criteria"), "{screen}");
    assert!(screen.contains("End of long description"), "{screen}");
    assert_eq!(
        h.app.detail_cursor, DESCRIPTION,
        "PageDown must not move the cursor"
    );
    assert_eq!(
        h.selected_title(),
        selected,
        "PageDown must not change task selection"
    );

    let mut top = String::new();
    for _ in 0..30 {
        top = h.press(KeyCode::PageUp);
        if top.contains("Detail line 001") {
            break;
        }
    }
    assert!(top.contains("Detail line 001"), "{top}");
    assert_eq!(h.app.detail_cursor, DESCRIPTION);
}

#[test]
fn long_unbroken_title_wraps_and_stays_on_screen() {
    let mut h = Harness::new();
    let title: String = "Q".repeat(500);
    seed(&h.root, &title, "To Do", &[]);
    h.app.tick();
    select_task_titled(&mut h, &title);
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Enter);
    assert_eq!(h.app.detail_cursor, TITLE);
    assert!(screen.contains(&"Q".repeat(20)), "{screen}");
    // A run long enough that it cannot appear in the truncated title-bar
    // (which also echoes the one-task filter text), only in a full-width
    // content row of the pane itself.
    let long_run = "Q".repeat(45);
    assert_ne!(
        cell_bg(&h, &long_run),
        Some(ratatui::style::Color::Reset),
        "the wrapped title should still carry the cursor highlight"
    );
}

#[test]
fn three_terminal_sizes_render_the_focused_pane_without_panicking() {
    for (width, height) in [(40u16, 12u16), (80, 24), (180, 50)] {
        let mut h = Harness::new();
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        select_task_titled(&mut h, "Fix login redirect loop");
        h.press(KeyCode::Enter);
        let screen = h.press(KeyCode::Enter);
        assert_eq!(h.app.mode, Mode::DetailFocus, "{width}x{height}: {screen}");
        for _ in 0..3 {
            h.press(KeyCode::Char('j'));
        }
        let _ = h.render();
    }
}

#[test]
fn description_row_shows_its_body_and_is_read_only_with_a_status_pointer() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    let id = h.app.selected_task().unwrap().id.clone();
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("description:"), "{screen}");
    assert!(
        screen.contains("Description of Add dark theme."),
        "{screen}"
    );
    h.press(KeyCode::Enter);
    for _ in 0..DESCRIPTION {
        h.press(KeyCode::Char('j'));
    }
    assert_eq!(h.app.detail_cursor, DESCRIPTION);
    let screen = h.press(KeyCode::Enter);
    assert_eq!(
        h.app.mode,
        Mode::DetailFocus,
        "the description row has nothing to open"
    );
    assert!(
        screen.contains(&format!(
            "edit the description with sb edit {id} --description"
        )),
        "{screen}"
    );
    assert_eq!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .description,
        format!("Description of Add dark theme.")
    );
}

#[test]
fn wheel_scrolls_the_detail_pane_and_is_bounded_without_touching_focus_or_selection() {
    let mut h = Harness::new();
    let description: String = (1..=120).map(|n| format!("Detail line {n:03}\n")).collect();
    let task = switchbard_core::NewBacklogTask {
        title: "Wheel task".to_string(),
        description,
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria: vec!["End of wheel task".to_string()],
        parent: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        project: None,
        dependencies: Vec::new(),
        due_date: None,
        custom: Vec::new(),
    };
    switchbard_core::create_task_allocating_id(&h.root, &task).unwrap();
    h.app.tick();
    select_task_titled(&mut h, "Wheel task");
    // A first `open` only opens the pane; the list keeps keyboard focus, and
    // the wheel must scroll the pane without changing either.
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::Browse);
    let selected = h.selected_title();

    let mut after = String::new();
    for _ in 0..200 {
        after = h.mouse(MouseEventKind::ScrollDown, 75, 10);
        if after.contains("End of wheel task") {
            break;
        }
    }
    assert!(after.contains("End of wheel task"), "{after}");
    assert_eq!(h.app.mode, Mode::Browse, "wheel must not change focus");
    assert_eq!(
        h.selected_title(),
        selected,
        "wheel must not change selection"
    );

    // Bounded: scrolling far past the end, then far past the start, lands
    // exactly on the actual top and bottom rather than wrapping or panicking.
    for _ in 0..300 {
        h.mouse(MouseEventKind::ScrollDown, 75, 10);
    }
    let bottom = h.render();
    assert!(bottom.contains("custom fields: Not set"), "{bottom}");

    let mut top = String::new();
    for _ in 0..500 {
        top = h.mouse(MouseEventKind::ScrollUp, 75, 10);
    }
    assert!(top.contains("Detail line 001"), "{top}");
}

#[test]
fn clicking_a_pane_row_moves_the_cursor_there_and_focuses_the_pane() {
    let mut h = Harness::new();
    select_task_titled(&mut h, "Add dark theme");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::Browse);
    let selected = h.selected_title();
    // Status is the pane's second row; see the fixed row order in the
    // module-level `STATUS` constant.
    let y = h.app.detail_hit.detail_inner().y + h.app.detail_hit.row_starts[STATUS];
    let screen = h.mouse(MouseEventKind::Down(MouseButton::Left), 55, y);
    assert_eq!(h.app.mode, Mode::DetailFocus);
    assert_eq!(h.app.detail_cursor, STATUS);
    assert!(screen.contains("status:"), "{screen}");
    assert_eq!(
        h.selected_title(),
        selected,
        "clicking a pane row must not change list selection"
    );

    // A different row: due date, two rows further down.
    let y = h.app.detail_hit.detail_inner().y + h.app.detail_hit.row_starts[DUE_DATE];
    let screen = h.mouse(MouseEventKind::Down(MouseButton::Left), 55, y);
    assert_eq!(h.app.detail_cursor, DUE_DATE);
    assert!(screen.contains("due date:"), "{screen}");
}

#[test]
fn clicking_the_list_selects_that_row_and_returns_to_browse() {
    let mut h = Harness::new();
    let rows = visible_titles(&h);
    h.press(KeyCode::Enter);
    h.press(KeyCode::Enter);
    assert_eq!(h.app.mode, Mode::DetailFocus);
    let selected = h.selected_title();
    let target_row = rows
        .iter()
        .position(|title| *title != selected)
        .expect("fixture has more than one task");

    let screen = h.mouse(
        MouseEventKind::Down(MouseButton::Left),
        10,
        3 + target_row as u16,
    );
    assert_eq!(
        h.app.mode,
        Mode::Browse,
        "click outside the pane returns to list"
    );
    assert_eq!(h.selected_title(), rows[target_row]);
    assert!(screen.contains(&rows[target_row]), "{screen}");
}
