//! `v`: saved view slots, repo and global.

mod harness;

use std::path::Path;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_tui::app::{App, AppPaths};
use switchbard_tui::telemetry::Telemetry;

#[test]
fn field_filters_and_v_digit_switch_views() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("label:auth");
    assert!(screen.contains("1/3"), "{screen}");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('3'));
    assert!(screen.contains("v3 · status:inprogress · 1/3"), "{screen}");
    assert!(screen.contains("Fix login"), "{screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("v2 · status:todo · 2/3"), "{screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("v1 · 3/3"), "{screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('9'));
    assert!(screen.contains("no view in slot 9"), "{screen}");
}

#[test]
fn vsd_saves_for_this_repo_and_vgd_extends_it_to_every_repo() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("status:!done");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('1'));
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('d'));
    assert!(screen.contains("saved v1 for this repo"), "{screen}");
    assert!(
        screen.contains("v1 · status:!done · ≈pri · 3/3"),
        "{screen}"
    );
    let repo_file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(
        repo_file.contains(
            "[1] = { filter = \"status:!done\", sort = \"priority:semantic\", columns = \"id,status,priority,title\" }"
        ),
        "{repo_file}"
    );
    assert!(
        !h.root.join("views.lua").exists(),
        "a repo save must not touch the global file"
    );

    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.state.filter, "status:!done");
    assert_eq!(fresh.view_label(), "v1");
    assert_eq!(fresh.views.get(0).unwrap().label(), "status:!done ≈pri");

    let other_repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(other_repo.path().join("backlog/tasks")).unwrap();
    let global_file_path = h.root.join("views.lua");
    let other_global = move |root: &Path| {
        App::open(
            root,
            AppPaths {
                global_views: Some(global_file_path.clone()),
                repo_views: Some(root.join("views-repo.lua")),
                ..AppPaths::default()
            },
            Telemetry::in_memory(),
        )
    };
    assert_eq!(
        other_global(other_repo.path())
            .views
            .get(0)
            .unwrap()
            .label(),
        "all",
        "other repos still open the global default"
    );

    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('g'));
    let screen = h.press(KeyCode::Char('d'));
    assert!(screen.contains("slot 1 is now global"), "{screen}");
    let global_file = std::fs::read_to_string(h.root.join("views.lua")).unwrap();
    assert!(
        global_file.contains("filter = \"status:!done\""),
        "{global_file}"
    );
    assert!(
        global_file.contains("filter = \"status:todo\""),
        "starter slots kept: {global_file}"
    );
    let repo_file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(!repo_file.contains("done"), "override dropped: {repo_file}");
    assert_eq!(
        other_global(other_repo.path())
            .views
            .get(0)
            .unwrap()
            .label(),
        "status:!done ≈pri"
    );
    assert_eq!(
        h.app.view_label(),
        "v1",
        "still on the slot after promotion"
    );
}

#[test]
fn vs_with_the_next_free_slot_appends_without_asking_and_escape_abandons() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("label:ui");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('6'));
    assert!(screen.contains("v6 · label:ui · 1/3"), "{screen}");
    h.press(KeyCode::Char('?'));
    let screen = h.render();
    assert!(
        screen.contains("label:ui [repo]"),
        "help marks repo slots: {screen}"
    );
    h.press(KeyCode::Char('?'));
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('9'));
    assert!(screen.contains("slot 9 is out of reach"), "{screen}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Esc);
    assert!(!screen.contains("saved"), "esc abandons: {screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(
        screen.contains("v2 · status:todo"),
        "slot 2 untouched: {screen}"
    );
}

#[test]
fn a_hand_written_repo_name_loads_without_a_warning_and_shows_in_the_picker() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
    std::fs::write(
        root.join("backlog/config.yml"),
        "project_name: fixture\nstatuses: [\"To Do\", \"In Progress\", \"Done\"]\ntask_prefix: task\n",
    )
    .unwrap();
    std::fs::write(
        root.join("views-repo.lua"),
        "return {\n  [1] = { name = \"Mine\", filter = \"ball:me\", sort = \"\" },\n}\n",
    )
    .unwrap();
    let config_path = root.join("tui.lua");
    let app = open_app(&root, &config_path);
    assert!(app.status.is_empty(), "no warning: {}", app.status);
    assert_eq!(app.views.get(0).unwrap().name, "Mine");
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 20)).unwrap();
    let mut app = app;
    terminal
        .draw(|frame| switchbard_tui::view::draw(frame, &mut app))
        .unwrap();
    app.handle_key(crossterm::event::KeyEvent::new(
        KeyCode::Char('v'),
        crossterm::event::KeyModifiers::NONE,
    ));
    terminal
        .draw(|frame| switchbard_tui::view::draw(frame, &mut app))
        .unwrap();
    let screen = switchbard_tui::view::buffer_text(terminal.backend().buffer());
    assert!(screen.contains("Mine"), "{screen}");
}

#[test]
fn v_n_names_a_slot_and_it_round_trips_through_the_file_and_reload() {
    let mut h = Harness::new();
    // Save the current filter into slot 1 first, so it is a repo override
    // (naming a global-only slot writes the global file instead - see below).
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('n'));
    h.press(KeyCode::Char('1'));
    h.type_text("My todo view");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("slot 1 named \"My todo view\""), "{screen}");
    let repo_file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(repo_file.contains("name = \"My todo view\""), "{repo_file}");
    assert_eq!(h.app.view_label(), "My todo view");

    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.views.get(0).unwrap().name, "My todo view");
}

#[test]
fn naming_a_slot_with_no_repo_override_writes_the_global_file() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('n'));
    h.press(KeyCode::Char('1'));
    h.type_text("Default view");
    h.press(KeyCode::Enter);
    assert!(
        !h.root.join("views-repo.lua").exists(),
        "no repo override existed for slot 1; nothing should be written there"
    );
    let global_file = std::fs::read_to_string(h.root.join("views.lua")).unwrap();
    assert!(
        global_file.contains("name = \"Default view\""),
        "{global_file}"
    );
}

#[test]
fn v_n_esc_cancels_without_writing() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('n'));
    h.press(KeyCode::Char('1'));
    h.type_text("abandoned");
    let screen = h.press(KeyCode::Esc);
    assert!(screen.contains("view naming cancelled"), "{screen}");
    assert!(
        !h.root.join("views-repo.lua").exists(),
        "esc must not write a file"
    );
    assert_eq!(h.app.views.get(0).unwrap().name, "");
}

#[test]
fn v_x_deletes_a_slot_and_the_file_no_longer_contains_it() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('6'));
    let repo_file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(repo_file.contains("[6]"), "{repo_file}");

    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('x'));
    let screen = h.press(KeyCode::Char('6'));
    assert!(screen.contains("deleted slot 6"), "{screen}");
    let repo_file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(!repo_file.contains("[6]"), "{repo_file}");
    assert!(h.app.views.get(5).is_none());
}

#[test]
fn a_named_view_shows_custom_once_its_live_state_diverges() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('n'));
    h.press(KeyCode::Char('1'));
    h.type_text("Everything");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.view_label(), "Everything");
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.view_label(), "custom");
}

#[test]
fn saving_into_another_slot_does_not_carry_the_origin_slots_name() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('n'));
    h.press(KeyCode::Char('1'));
    h.type_text("Mine");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.views.get(0).unwrap().name, "Mine");
    h.type_text("/status:done");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('2'));
    assert_eq!(h.app.views.get(0).unwrap().name, "Mine");
    assert_eq!(h.app.views.get(1).unwrap().name, "");
    assert_eq!(h.app.view_label(), "v2");
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert_eq!(file.matches("Mine").count(), 1, "{file}");
}
