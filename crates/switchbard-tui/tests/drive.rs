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
        seed(
            &root,
            "Fix login redirect loop",
            "In Progress",
            &["auth", "bug"],
        );
        seed(&root, "Add dark theme", "To Do", &["ui"]);
        seed(&root, "Write onboarding guide", "To Do", &["docs"]);
        let config_path = root.join("tui.lua");
        let app = App::open(&root, Some(config_path.clone()), Telemetry::in_memory());
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

fn seed(root: &Path, title: &str, status: &str, labels: &[&str]) {
    let task = NewBacklogTask {
        title: title.to_string(),
        description: format!("Description of {title}."),
        status: status.to_string(),
        priority: "medium".to_string(),
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
    assert!(screen.contains("all · 3/3"), "{screen}");
}

#[test]
fn j_and_k_move_selection_and_enter_opens_detail() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('j'));
    assert_eq!(h.selected_title(), "Add dark theme");
    h.press(KeyCode::Char('k'));
    assert_eq!(h.selected_title(), "Fix login redirect loop");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("Description of Fix login redirect loop."),
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
fn field_filters_and_number_keys_switch_views() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("label:auth");
    assert!(screen.contains("1/3"), "{screen}");
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Char('3'));
    assert!(
        screen.contains("active · status:inprogress · 1/3"),
        "{screen}"
    );
    assert!(screen.contains("Fix login"), "{screen}");
    let screen = h.press(KeyCode::Char('2'));
    assert!(screen.contains("todo · status:todo · 2/3"), "{screen}");
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("all · 3/3"), "{screen}");
}

#[test]
fn colon_bug_files_a_task_carrying_screen_and_trail() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('j'));
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
    assert!(filed.contains("selected=TASK-2"), "{filed}");
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
    assert!(screen.contains("1 To Do"), "{screen}");
    assert!(screen.contains("2 In Progress"), "{screen}");
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
