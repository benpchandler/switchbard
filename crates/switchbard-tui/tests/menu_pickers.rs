//! Menu choices are real shared picker rows, exercised through real key events.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};
use switchbard_tui::app::Mode;

fn select_keyed_row(h: &mut Harness, key: char) -> String {
    let position = h
        .app
        .picker
        .as_ref()
        .expect("menu picker")
        .options
        .iter()
        .position(|option| option.key == Some(key))
        .expect("keyed action row");
    for _ in 0..position {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter)
}

#[test]
fn task_menu_choices_render_and_arrow_selection_creates() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('t'));
    assert_eq!(h.app.mode, Mode::PickValue);
    assert!(screen.to_lowercase().contains("new"), "{screen}");
    assert!(!screen.contains("a number ranks it"), "{screen}");
    let picker = h.app.picker.as_ref().unwrap();
    for key in ['n', 'b', 's', 'r', 'p', 'g'] {
        assert!(
            picker.options.iter().any(|o| o.key == Some(key)),
            "missing {key}"
        );
    }
    select_keyed_row(&mut h, 'n');
    h.type_text("Created from picker");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("created TASK-4"), "{screen}");
    assert_eq!(h.selected_title(), "Created from picker");
}

#[test]
fn empty_task_menu_still_creates_and_escape_does_not_write() {
    let mut h = Harness::new();
    for file in std::fs::read_dir(h.root.join("backlog/tasks")).unwrap() {
        std::fs::remove_file(file.unwrap().path()).unwrap();
    }
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Char('t'));
    assert!(h.app.picker.is_some());
    h.press(KeyCode::Esc);
    assert_eq!(h.app.total_tasks(), 0);
    h.type_text("tnFirst task");
    h.press(KeyCode::Enter);
    assert_eq!(h.selected_title(), "First task");
}

#[test]
fn view_open_save_and_global_are_picker_menus() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    assert_eq!(h.app.mode, Mode::PickValue);
    assert!(h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .any(|o| o.key == Some('s')));
    h.press(KeyCode::Char('s'));
    assert_eq!(h.app.mode, Mode::PickValue);
    let screen = h.render();
    assert!(!screen.contains("save view into: d"), "{screen}");
    h.press(KeyCode::Esc);
    assert!(!h.root.join("views-repo.lua").exists());
    h.type_text("vg");
    assert_eq!(h.app.mode, Mode::PickValue);
    assert!(h.app.picker.is_some());
    h.press(KeyCode::Esc);
    assert!(!h.root.join("views.lua").exists());
}

#[test]
fn columns_and_settings_expose_their_actions_as_rows() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    for key in ['m', 'g', 'a'] {
        assert!(
            h.app
                .picker
                .as_ref()
                .unwrap()
                .options
                .iter()
                .any(|o| o.key == Some(key)),
            "missing {key}"
        );
    }
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char(','));
    assert!(h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .any(|o| o.key == Some('g')));
}

#[test]
fn short_picker_keeps_arrow_selected_rows_visible() {
    for (width, height) in [(40, 8), (100, 20), (160, 30)] {
        let mut h = Harness::new();
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        h.press(KeyCode::Char('t'));
        let count = h.app.picker.as_ref().unwrap().options.len();
        for _ in 1..count {
            h.press(KeyCode::Down);
        }
        let label = h.app.picker.as_ref().unwrap().highlighted().unwrap().label;
        let screen = h.render();
        let prefix: String = label.chars().take(12).collect();
        assert!(screen.contains(&prefix), "{width}x{height}: {screen}");
        h.press(KeyCode::Esc);
        assert_eq!(h.app.mode, Mode::Browse);
    }
}

#[test]
fn task_menu_does_not_execute_mutations_from_typeahead() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();
    let status = h.app.selected_task().unwrap().status.clone();
    h.press(KeyCode::Char('t'));
    h.type_text("zzzz");
    h.press(KeyCode::Esc);
    assert_eq!(
        h.app.tasks().iter().find(|t| t.id == id).unwrap().status,
        status
    );
}

#[test]
fn column_action_row_asks_for_explicit_target_then_applies_it() {
    let mut h = Harness::new();
    assert!(h
        .app
        .state
        .abbreviated
        .contains(&switchbard_tui::columns::Column::Id));
    h.press(KeyCode::Char('c'));
    select_keyed_row(&mut h, 'a');
    assert!(h.app.picker.is_some(), "action opens a target chooser");
    let target = h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .position(|o| {
            matches!(
                o.payload,
                switchbard_tui::picker::Payload::Column(switchbard_tui::columns::Column::Id)
            )
        })
        .unwrap();
    for _ in 0..target {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
    assert!(!h
        .app
        .state
        .abbreviated
        .contains(&switchbard_tui::columns::Column::Id));
}

#[test]
#[ignore = "exports rendered fixture evidence when SBT_MENU_EVIDENCE_DIR is set"]
fn menu_render_evidence() {
    let Ok(directory) = std::env::var("SBT_MENU_EVIDENCE_DIR") else {
        return;
    };
    std::fs::create_dir_all(&directory).unwrap();
    for (name, keys, width, height) in [
        ("tasks", "t", 100, 20),
        ("views", "v", 100, 20),
        ("save-view", "vs", 100, 20),
        ("columns", "c", 100, 20),
        ("settings", ",", 100, 20),
        ("status", "ts", 100, 20),
        ("project", "tp", 100, 20),
        ("top-list", "tr", 100, 20),
        ("tasks-narrow", "t", 40, 8),
    ] {
        let mut h = Harness::new();
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        if keys == "tp" {
            seed_project(&h.root, "Delivery", "In Progress", None);
            h.app.tick();
        }
        if keys == "tr" {
            h.type_text("t1");
        }
        h.type_text(keys);
        let buffer = h.terminal.backend().buffer();
        let cells: Vec<_> = buffer.content.iter().map(|cell| serde_json::json!({
            "symbol": cell.symbol(), "fg": format!("{:?}", cell.fg), "bg": format!("{:?}", cell.bg),
        })).collect();
        let evidence = serde_json::json!({"width":width,"height":height,"cells":cells});
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.json")),
            serde_json::to_vec(&evidence).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn task_menu_keeps_its_target_when_external_tasks_reorder_the_list() {
    let mut h = Harness::new();
    h.type_text("s41");
    let target = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Char('t'));
    seed(&h.root, "A new first task", "To Do", &[]);
    h.app.tick();
    h.type_text("sDone");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.id == target)
            .unwrap()
            .status,
        "Done"
    );
    assert_eq!(
        h.app
            .tasks()
            .iter()
            .find(|t| t.title == "A new first task")
            .unwrap()
            .status,
        "To Do"
    );
}

#[test]
fn vanished_task_cancels_its_menu_instead_of_retargeting() {
    let mut h = Harness::new();
    let path = h.app.selected_task().unwrap().path.clone();
    h.press(KeyCode::Char('t'));
    std::fs::remove_file(path).unwrap();
    h.app.tick();
    let screen = h.render();
    assert!(h.app.picker.is_none(), "{screen}");
    assert_eq!(h.app.mode, Mode::Browse);
    assert!(!h.app.status.is_empty(), "{screen}");
    h.press(KeyCode::Char('d'));
    assert!(h.app.tasks().iter().all(|task| task.status != "Done"));
}

#[test]
fn left_and_vim_back_return_to_parent_and_right_opens_a_choice() {
    let mut h = Harness::new();
    h.type_text("ts");
    h.press(KeyCode::Left);
    assert!(h.render().contains("New task"));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('h'));
    assert!(h.render().contains("New task"));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('v'));
    let save = h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .position(|o| o.key == Some('s'))
        .unwrap();
    for _ in 0..save {
        h.press(KeyCode::Char('j'));
    }
    h.press(KeyCode::Right);
    assert!(h.render().contains("save view"));
    h.press(KeyCode::Char('h'));
    assert!(h
        .app
        .picker
        .as_ref()
        .unwrap()
        .options
        .iter()
        .any(|o| o.key == Some('g')));
    h.press(KeyCode::Esc);
    assert_eq!(h.app.mode, Mode::Browse);
}

#[test]
fn top_list_membership_and_view_display_are_separate_menus() {
    let mut h = Harness::new();
    let id = h.app.selected_task().unwrap().id.clone();
    h.type_text("tr");
    assert!(h.render().to_lowercase().contains("top list"));
    let picker = h.app.picker.as_ref().unwrap();
    assert!(picker
        .options
        .iter()
        .any(|o| o.label.to_lowercase().contains("end")));
    h.press(KeyCode::Char('a'));
    assert!(h.app.top.contains(&id));
    h.type_text("vp");
    assert!(!h.app.state.pin_top);
    assert!(
        h.app.top.contains(&id),
        "hiding the section must retain membership"
    );
    h.type_text("trx");
    assert!(!h.app.top.contains(&id));
}
