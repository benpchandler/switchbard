//! End-to-end: real key events into the real app against a real backlog on disk,
//! asserting on the rendered screen. This is the only kind of test this crate has.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use switchbard_core::{create_task_allocating_id, NewBacklogTask};
use switchbard_tui::app::App;
use switchbard_tui::telemetry::Telemetry;
use switchbard_tui::view;

struct Harness {
    _dir: tempfile::TempDir,
    root: PathBuf,
    config_path: PathBuf,
    app: App,
    terminal: Terminal<TestBackend>,
}

impl Harness {
    fn new() -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
        std::fs::write(
            root.join("backlog/config.yml"),
            "project_name: fixture\nstatuses: [\"To Do\", \"In Progress\", \"Done\"]\ntask_prefix: task\n",
        )
        .unwrap();
        seed_with_priority(
            &root,
            "Fix login redirect loop",
            "In Progress",
            &["auth", "bug"],
            "medium",
        );
        seed_with_priority(&root, "Add dark theme", "To Do", &["ui"], "low");
        seed_with_priority(&root, "Write onboarding guide", "To Do", &["docs"], "high");
        let config_path = root.join("tui.lua");
        let app = open_app(&root, &config_path);
        let terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let mut harness = Harness {
            _dir: dir,
            root,
            config_path,
            app,
            terminal,
        };
        harness.render();
        harness
    }

    fn render(&mut self) -> String {
        let app = &mut self.app;
        self.terminal.draw(|frame| view::draw(frame, app)).unwrap();
        view::buffer_text(self.terminal.backend().buffer())
    }

    fn press(&mut self, code: KeyCode) -> String {
        self.app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        self.render()
    }

    fn type_text(&mut self, text: &str) -> String {
        for c in text.chars() {
            self.app
                .handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        self.render()
    }

    fn selected_title(&self) -> String {
        self.app.selected_task().unwrap().title.clone()
    }
}

fn open_app(root: &Path, config_path: &Path) -> App {
    App::open(
        root,
        Some(config_path.to_path_buf()),
        Some(root.join("views.lua")),
        Some(root.join("views-repo.lua")),
        Telemetry::in_memory(),
    )
}

fn seed(root: &Path, title: &str, status: &str, labels: &[&str]) {
    seed_with_priority(root, title, status, labels, "medium");
}

fn seed_with_priority(root: &Path, title: &str, status: &str, labels: &[&str], priority: &str) {
    let task = NewBacklogTask {
        title: title.to_string(),
        description: format!("Description of {title}."),
        status: status.to_string(),
        priority: priority.to_string(),
        acceptance_criteria: vec!["It works".to_string()],
        parent: None,
        labels: labels.iter().map(|s| s.to_string()).collect(),
        assignees: Vec::new(),
        project: None,
        dependencies: Vec::new(),
    };
    create_task_allocating_id(root, &task).unwrap();
}

#[test]
fn lists_every_task_with_repo_name_and_count() {
    let mut h = Harness::new();
    let screen = h.render();
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("Add dark theme"), "{screen}");
    assert!(screen.contains("1 all · 3/3"), "{screen}");
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
fn slash_filters_live_and_esc_clears() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("dark");
    assert!(screen.contains("Add dark theme"), "{screen}");
    assert!(!screen.contains("Fix login"), "{screen}");
    assert!(screen.contains("1/3"), "{screen}");
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Esc);
    assert!(screen.contains("3/3"), "{screen}");
}

#[test]
fn field_filters_and_v_digit_switch_views() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("label:auth");
    assert!(screen.contains("1/3"), "{screen}");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('3'));
    assert!(
        screen.contains("3 active · status:inprogress · 1/3"),
        "{screen}"
    );
    assert!(screen.contains("Fix login"), "{screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("2 todo · status:todo · 2/3"), "{screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("1 all · 3/3"), "{screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('9'));
    assert!(screen.contains("no view in slot 9"), "{screen}");
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
fn f_then_column_number_picks_a_value_from_the_data() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("1 ✓To Do"), "{screen}");
    assert!(screen.contains("2 ✓In Progress"), "{screen}");
    let screen = h.press(KeyCode::Char('j'));
    let screen_after_enter = h.press(KeyCode::Enter);
    assert!(
        !screen_after_enter.contains("1 To Do"),
        "picker still open: {screen_after_enter}"
    );
    assert!(
        screen_after_enter.contains("status:inprogress · 1/3"),
        "{screen} {screen_after_enter}"
    );
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(
        screen.contains("status:todo · 2/3"),
        "replacing the status term: {screen}"
    );
}

#[test]
fn f_on_a_free_text_column_drops_into_search() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('4'));
    let screen = h.type_text("guide");
    assert!(screen.contains("/guide"), "{screen}");
    assert!(screen.contains("1/3"), "{screen}");
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
fn resume_state_survives_a_self_restart() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('j'));
    let state = h.app.resume_state();
    let mut fresh = open_app(&h.root, &h.config_path);
    fresh.resume_from(Some(&state));
    assert_eq!(fresh.view, 1);
    assert_eq!(fresh.filter_text, "status:todo");
    assert_eq!(fresh.selected, 1);
}

#[test]
fn zero_size_terminal_does_not_crash() {
    let mut h = Harness::new();
    h.terminal = Terminal::new(TestBackend::new(0, 0)).unwrap();
    let screen = h.press(KeyCode::Char('?'));
    assert_eq!(screen, "");
}

#[test]
fn space_in_picker_toggles_values_and_writes_the_shortest_filter() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    let screen = h.render();
    assert!(
        screen.contains("✓To Do") && screen.contains("✓In Progress"),
        "{screen}"
    );
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains(" To Do"), "unchecked: {screen}");
    assert!(screen.contains("status:!todo · 1/3"), "{screen}");
    h.press(KeyCode::Char('j'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(
        screen.contains("status:!todo status:!inprogress · 0/3"),
        "{screen}"
    );
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("status:!todo · 1/3"), "re-shown: {screen}");
    h.press(KeyCode::Char('k'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("1 all · 3/3"), "all shown again: {screen}");
    let screen = h.press(KeyCode::Esc);
    assert!(!screen.contains("space toggles"), "{screen}");
}

#[test]
fn space_widens_a_single_value_filter_instead_of_fighting_it() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('j'));
    let screen = h.press(KeyCode::Char(' '));
    assert!(screen.contains("· 3/3"), "{screen}");
    assert!(screen.contains("✓In Progress"), "{screen}");
}

#[test]
fn editing_the_filter_relabels_the_view_as_custom() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('3'));
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("pri:medium");
    assert!(
        screen.contains("custom · status:inprogress pri:medium · 1/3"),
        "{screen}"
    );
}

#[test]
fn typing_in_the_picker_narrows_and_a_unique_match_applies_at_once() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('2'));
    let screen = h.type_text("o");
    assert!(screen.contains("status: o▏"), "{screen}");
    assert!(
        screen.contains("To Do") && screen.contains("In Progress"),
        "{screen}"
    );
    let screen = h.type_text("d");
    assert!(screen.contains("status:todo · 2/3"), "{screen}");
    h.press(KeyCode::Char('f'));
    h.press(KeyCode::Char('3'));
    let screen = h.type_text("h");
    assert!(
        screen.contains("status:todo pri:high · 1/3"),
        "stacked: {screen}"
    );
}

#[test]
fn f_then_a_letter_explains_the_column_numbers() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('f'));
    let screen = h.press(KeyCode::Char('f'));
    assert!(
        screen.contains("'f' is not a column; press 1-4"),
        "{screen}"
    );
}

fn visible_titles(h: &Harness) -> Vec<String> {
    (0..h.app.visible.len())
        .map(|index| h.app.task(index).unwrap().title.clone())
        .collect()
}

#[test]
fn s_then_column_offers_semantic_and_plain_orders() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('3'));
    assert!(screen.contains("sort by pri"), "{screen}");
    assert!(
        screen.contains("1  semantic (high, medium, low)"),
        "{screen}"
    );
    assert!(screen.contains("2  ascending"), "{screen}");
    assert!(screen.contains("4  none"), "{screen}");
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("≈pri · 3/3"), "{screen}");
    assert_eq!(
        visible_titles(&h),
        [
            "Write onboarding guide",
            "Fix login redirect loop",
            "Add dark theme"
        ]
    );
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('3'));
    h.type_text("d");
    assert!(h.render().contains("↓pri"), "{}", h.render());
    assert_eq!(visible_titles(&h)[0], "Fix login redirect loop");
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('3'));
    let screen = h.type_text("n");
    assert!(!screen.contains("pri ·"), "sort cleared: {screen}");
    assert_eq!(visible_titles(&h)[0], "Fix login redirect loop");
}

#[test]
fn sort_survives_filtering_and_title_sorts_alphabetically() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('4'));
    h.press(KeyCode::Char('1'));
    assert_eq!(visible_titles(&h)[0], "Add dark theme");
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    let screen = h.render();
    assert!(
        screen.contains("custom · status:todo · ↑title · 2/3"),
        "{screen}"
    );
    assert_eq!(
        visible_titles(&h),
        ["Add dark theme", "Write onboarding guide"]
    );
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
        screen.contains("name: all▏"),
        "prefilled with slot 1's name: {screen}"
    );
    for _ in 0..3 {
        h.press(KeyCode::Backspace);
    }
    h.type_text("open");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("saved slot 1 for this repo · v1 opens it · vg1 makes it global"),
        "{screen}"
    );
    assert!(
        screen.contains("1 open · status:!done · ≈pri · 3/3"),
        "{screen}"
    );
    let repo_file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(
        repo_file.contains(
            "[1] = { name = \"open\", filter = \"status:!done\", sort = \"priority:semantic\" }"
        ),
        "{repo_file}"
    );
    assert!(
        !h.root.join("views.lua").exists(),
        "a repo save must not touch the global file"
    );

    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.filter_text, "status:!done");
    assert_eq!(fresh.view_label(), "1 open");

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
        other_global(other_repo.path()).view_label(),
        "1 all",
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
    assert!(global_file.contains("name = \"open\""), "{global_file}");
    assert!(
        global_file.contains("name = \"todo\""),
        "starter slots kept: {global_file}"
    );
    let repo_file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(!repo_file.contains("open"), "override dropped: {repo_file}");
    assert_eq!(other_global(other_repo.path()).view_label(), "1 open");
    assert_eq!(
        h.app.view_label(),
        "1 open",
        "still on the slot after promotion"
    );
}

#[test]
fn vs_with_the_next_free_slot_appends_and_escape_abandons() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("label:ui");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('5'));
    assert!(
        screen.contains("name: ui▏"),
        "suggested from the filter: {screen}"
    );
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("5 ui · label:ui · 1/3"), "{screen}");
    h.press(KeyCode::Char('?'));
    let screen = h.render();
    assert!(
        screen.contains("ui (label:ui) [repo]"),
        "help marks repo slots: {screen}"
    );
    h.press(KeyCode::Char('?'));
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('9'));
    assert!(
        screen.contains("slot 9 is out of reach; use 1-6"),
        "{screen}"
    );
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('2'));
    let screen = h.press(KeyCode::Esc);
    assert!(screen.contains("view not saved"), "{screen}");
    h.press(KeyCode::Char('v'));
    let screen = h.press(KeyCode::Char('2'));
    assert!(
        screen.contains("2 todo · status:todo"),
        "slot 2 untouched: {screen}"
    );
}
