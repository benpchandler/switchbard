//! `v`: saved view slots, repo and global.

mod harness;

use std::path::Path;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_tui::app::App;
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
    assert!(
        screen.contains("saved v1 for this repo · vg1 makes it global"),
        "{screen}"
    );
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
    assert_eq!(fresh.views.get(0).unwrap().name(), "status:!done ≈pri");

    let other_repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(other_repo.path().join("backlog/tasks")).unwrap();
    let global_file_path = h.root.join("views.lua");
    let other_global = move |root: &Path| {
        App::open(
            root,
            None,
            Some(global_file_path.clone()),
            Some(root.join("views-repo.lua")),
            Telemetry::in_memory(),
        )
    };
    assert_eq!(
        other_global(other_repo.path()).views.get(0).unwrap().name(),
        "all",
        "other repos still open the global default"
    );

    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('g'));
    let screen = h.press(KeyCode::Char('d'));
    assert!(
        screen.contains("slot 1 is now global: every repo opens it with v1"),
        "{screen}"
    );
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
        other_global(other_repo.path()).views.get(0).unwrap().name(),
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
    assert!(
        screen.contains("slot 9 is out of reach; use 1-7"),
        "{screen}"
    );
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
