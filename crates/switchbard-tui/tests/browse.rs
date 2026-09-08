//! Launch, navigation, config, reports, telemetry, restart.

mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn lists_every_task_with_repo_name_and_count() {
    let mut h = Harness::new();
    let screen = h.render();
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("Add dark theme"), "{screen}");
    assert!(screen.contains("v1 · 3/3"), "{screen}");
}

#[test]
fn j_and_k_move_selection_and_enter_opens_detail() {
    let mut h = Harness::new();
    let rows = visible_titles(&h);
    h.press(KeyCode::Char('j'));
    assert_eq!(h.selected_title(), rows[1]);
    h.press(KeyCode::Char('k'));
    assert_eq!(h.selected_title(), rows[0]);
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains(&format!("Description of {}.", rows[0])),
        "{screen}"
    );
    assert!(screen.contains("[ ] It works"), "{screen}");
    let screen = h.press(KeyCode::Esc);
    assert!(!screen.contains("Description of"), "{screen}");
}

#[test]
fn unbound_key_is_reported_and_help_lists_bindings() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('z'));
    assert!(screen.contains("z is not bound"), "{screen}");
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("quit"), "{screen}");
    assert!(screen.contains(":bug"), "{screen}");
    assert!(h
        .app
        .telemetry
        .trail()
        .iter()
        .any(|line| line == "unbound z"));
}

#[test]
fn external_task_edits_show_up_on_tick() {
    let mut h = Harness::new();
    seed(&h.root, "Task added by sb", "To Do", &[]);
    h.app.tick();
    let screen = h.render();
    assert!(screen.contains("Task added by sb"), "{screen}");
    assert!(screen.contains("4/4"), "{screen}");
}

#[test]
fn column_headers_are_numbered() {
    let mut h = Harness::new();
    let screen = h.render();
    assert!(screen.contains("1 id"), "{screen}");
    assert!(screen.contains("2 status"), "{screen}");
    assert!(screen.contains("4 title"), "{screen}");
}

#[test]
fn resume_state_survives_a_self_restart() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('j'));
    let state = h.app.resume_state();
    let mut fresh = open_app(&h.root, &h.config_path);
    fresh.resume_from(Some(&state));
    assert_eq!(fresh.view, 1);
    assert_eq!(fresh.state.filter, "status:todo");
    assert_eq!(fresh.selected, 1);
}

/// TASK-173: a self-restart hands the live view to a *different* build. The
/// record has to survive that build having a field this one does not, and a
/// record that cannot be read has to say so - landing quietly on the saved
/// slot is what the report called "the build updated and my view reset".
#[test]
fn a_view_survives_a_restart_into_a_build_with_a_field_this_one_lacks() {
    let h = Harness::new();
    // A saved slot 1 to fall back to, the way a real repo has one: the reset
    // is invisible unless the live view differs from what slot 1 holds.
    std::fs::write(
        h.root.join("views-repo.lua"),
        "return { [1] = { filter = \"status:todo\", columns = \"id,status,priority,title\" } }",
    )
    .unwrap();
    let mut h = Harness {
        app: open_app(&h.root, &h.config_path),
        ..h
    };
    h.render();
    h.press(KeyCode::Char('/'));
    h.type_text("labels:ui");
    h.press(KeyCode::Enter);
    let live = h.render();
    assert!(live.contains("custom · status:todo labels:ui"), "{live}");

    // The next build writes the same named record plus one field of its own.
    let record = h.app.resume_state();
    let (prefix, json) = record.split_once('=').unwrap();
    let mut value: serde_json::Value = serde_json::from_str(json).unwrap();
    value["a_field_a_later_build_added"] = serde_json::json!(true);
    let from_a_newer_build = format!("{prefix}={value}");

    let mut fresh = open_app(&h.root, &h.config_path);
    fresh.resume_from(Some(&from_a_newer_build));
    assert_eq!(
        fresh.state.filter, "status:todo labels:ui",
        "the live view is kept, not the saved slot it was built from"
    );
    assert_eq!(fresh.view_label(), "custom");
    assert_eq!(fresh.status, "updated to the new build");
}

#[test]
fn a_resume_record_this_build_cannot_read_is_reported_not_swallowed() {
    let h = Harness::new();
    let mut fresh = open_app(&h.root, &h.config_path);
    fresh.resume_from(Some("pages=something this build cannot parse"));
    assert_eq!(
        fresh.status,
        "the new build could not read the previous view; opened your saved view"
    );
    let mut cold = open_app(&h.root, &h.config_path);
    cold.resume_from(None);
    assert_eq!(cold.status, "", "an ordinary cold start says nothing");
}

#[test]
fn zero_size_terminal_does_not_crash() {
    let mut h = Harness::new();
    h.terminal = Terminal::new(TestBackend::new(0, 0)).unwrap();
    let screen = h.press(KeyCode::Char('?'));
    assert_eq!(screen, "");
}

#[test]
fn colon_bug_files_a_task_carrying_screen_and_trail() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('j'));
    let selected_id = h.app.selected_task().unwrap().id.clone();
    h.press(KeyCode::Char(':'));
    h.type_text("bug wanted to sort by priority");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("filed TASK-4"), "{screen}");
    assert_eq!(h.selected_title(), "sbt bug: wanted to sort by priority");
    assert!(
        screen.contains("sbt bug: wanted to sort by priority"),
        "{screen}"
    );
    let filed = std::fs::read_dir(h.root.join("backlog/tasks"))
        .unwrap()
        .flatten()
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .find(|text| text.contains("sbt bug"))
        .unwrap();
    assert!(filed.contains("- tui\n"), "{filed}");
    assert!(filed.contains("- bug\n"), "{filed}");
    assert!(
        filed.contains(&format!("selected={selected_id}")),
        "{filed}"
    );
    assert!(
        filed.contains("Add dark theme"),
        "screen dump missing: {filed}"
    );
    assert!(filed.contains("action down"), "trail missing: {filed}");
}

#[test]
fn colon_bug_without_intent_is_refused() {
    let mut h = Harness::new();
    h.press(KeyCode::Char(':'));
    h.type_text("bug");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("say what you were trying to do"),
        "{screen}"
    );
    assert!(screen.contains("3/3"), "{screen}");
}

#[test]
fn colon_shows_completions_and_tab_accepts() {
    let mut h = Harness::new();
    h.press(KeyCode::Char(':'));
    let screen = h.type_text("b");
    assert!(screen.contains(":b▏   bug"), "{screen}");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains(":bug▏"), "{screen}");
    h.type_text(" tab test");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("filed TASK-4"), "{screen}");
}

#[test]
fn lua_config_rebinds_keys_and_hot_reloads() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('x'));
    assert!(!h.app.should_quit);
    std::fs::write(&h.config_path, "return { keys = { x = \"quit\" } }").unwrap();
    h.app.tick();
    let screen = h.render();
    assert!(screen.contains("config reloaded"), "{screen}");
    h.press(KeyCode::Char('x'));
    assert!(h.app.should_quit);
}

#[test]
fn broken_lua_config_keeps_defaults_and_says_so() {
    let mut h = Harness::new();
    std::fs::write(&h.config_path, "return { keys = { j = \"fly\" } }").unwrap();
    h.app.tick();
    let screen = h.render();
    assert!(screen.contains("unknown action 'fly'"), "{screen}");
    h.press(KeyCode::Char('k'));
    h.press(KeyCode::Char('q'));
    assert!(h.app.should_quit);
}

#[test]
fn reports_file_into_the_configured_repo_not_the_one_being_browsed() {
    let mut h = Harness::new();
    let tool_repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tool_repo.path().join("backlog/tasks")).unwrap();
    std::fs::write(
        tool_repo.path().join("backlog/config.yml"),
        "project_name: sbt\nstatuses: [\"To Do\", \"Done\"]\ntask_prefix: task\n",
    )
    .unwrap();
    std::fs::write(
        &h.config_path,
        format!(
            "return {{ report_repo = \"{}\" }}",
            tool_repo.path().display()
        ),
    )
    .unwrap();
    h.app.tick();
    h.press(KeyCode::Char(':'));
    h.type_text("bug the footer overlaps");
    h.press(KeyCode::Enter);
    assert!(h.app.status.starts_with("filed 1 in "), "{}", h.app.status);
    assert_eq!(
        h.app.total_tasks(),
        3,
        "nothing filed into the browsed repo"
    );
    let filed = std::fs::read_dir(tool_repo.path().join("backlog/tasks"))
        .unwrap()
        .count();
    assert_eq!(filed, 1);
}
