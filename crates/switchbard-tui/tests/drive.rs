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
    assert!(screen.contains("v1 · 3/3"), "all shown again: {screen}");
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
fn f_opens_the_column_list_with_shown_columns_numbered_as_in_the_header() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('f'));
    assert!(
        screen.contains("filter by column · number or name"),
        "{screen}"
    );
    for entry in [
        "1 ✓id",
        "2 ✓status",
        "3 ✓priority",
        "4 ✓title",
        "5  labels",
        "6  project",
    ] {
        assert!(screen.contains(entry), "{entry} missing: {screen}");
    }
    let screen = h.type_text("f");
    assert!(screen.contains("filter by column: f▏"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("nothing matches 'f'"), "{screen}");
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
    assert_eq!(fresh.filter_text, "status:!done");
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
    let screen = h.press(KeyCode::Char('5'));
    assert!(screen.contains("v5 · label:ui · 1/3"), "{screen}");
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
        screen.contains("slot 9 is out of reach; use 1-6"),
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

fn cell_fg(h: &Harness, needle: &str) -> Option<ratatui::style::Color> {
    let buffer = h.terminal.backend().buffer();
    let width = buffer.area.width as usize;
    for (row, cells) in buffer.content.chunks(width).enumerate() {
        let line: String = cells.iter().map(|cell| cell.symbol()).collect();
        if let Some(col) = line.find(needle) {
            let col = line[..col].chars().count();
            return Some(buffer.content[row * width + col].fg);
        }
    }
    None
}

fn header_line(screen: &str) -> String {
    screen
        .lines()
        .find(|line| line.contains("1 "))
        .unwrap_or_default()
        .to_string()
}

#[test]
fn c_toggles_columns_by_position_and_numbers_follow_what_is_shown() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    let screen = h.render();
    assert!(screen.contains("1 ✓id"), "{screen}");
    assert!(
        screen.contains("5  labels"),
        "hidden columns listed after shown: {screen}"
    );
    let screen = h.press(KeyCode::Char('5'));
    assert!(
        !screen.contains("space keeps open"),
        "digit toggles and closes: {screen}"
    );
    assert!(header_line(&screen).contains("5 labels"), "{screen}");
    assert!(screen.contains("auth,bug"), "{screen}");
    h.press(KeyCode::Char('c'));
    let screen = h.press(KeyCode::Char('3'));
    let header = header_line(&screen);
    assert!(!header.contains("pri"), "{header}");
    assert!(
        header.contains("3 title") && header.contains("4 labels"),
        "renumbered: {header}"
    );
    assert!(screen.contains("cols:id,status,title,labels"), "{screen}");
}

#[test]
fn hidden_columns_are_listed_after_shown_ones_and_stay_filterable_and_sortable() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Char('3'));
    assert!(!header_line(&h.render()).contains("pri"));
    let screen = h.press(KeyCode::Char('f'));
    assert!(
        screen.contains("3 ✓title") && screen.contains("4  priority"),
        "hidden priority listed after the shown columns: {screen}"
    );
    let screen = h.press(KeyCode::Char('4'));
    assert!(screen.contains("pri · type/number picks one"), "{screen}");
    let screen = h.type_text("h");
    assert!(
        screen.contains("pri:high · cols:id,status,title · 1/3"),
        "{screen}"
    );
    h.press(KeyCode::Char('s'));
    let screen = h.type_text("p");
    assert!(
        screen.contains("1  priority") && screen.contains("2  project"),
        "an ambiguous name narrows to both: {screen}"
    );
    h.type_text("ri");
    let screen = h.type_text("d");
    assert!(screen.contains("↓pri"), "{screen}");
}
#[test]
fn shift_k_moves_a_column_up_and_the_order_saves_with_the_view() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('c'));
    h.press(KeyCode::Char('j'));
    h.app
        .handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT));
    let screen = h.press(KeyCode::Esc);
    let header = header_line(&screen);
    assert!(
        header.starts_with("│1 status") || header.contains("1 status    2 id"),
        "{header}"
    );
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(
        file.contains("columns = \"status,id,priority,title\""),
        "{file}"
    );
    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.columns[0].name(), "status");
    assert_eq!(fresh.view_label(), "v1");
    assert_eq!(
        fresh.views.get(0).unwrap().name(),
        "cols:status,id,priority,title"
    );
}

#[test]
fn the_last_column_cannot_be_hidden() {
    let mut h = Harness::new();
    for _ in 0..3 {
        h.press(KeyCode::Char('c'));
        h.press(KeyCode::Char('1'));
    }
    h.press(KeyCode::Char('c'));
    let screen = h.press(KeyCode::Char('1'));
    assert!(screen.contains("at least one column must stay"), "{screen}");
    assert_eq!(h.app.columns.len(), 1);
}

#[test]
fn p_paints_this_row_by_exact_id_and_saves_with_the_view() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    let selected = h.app.selected_task().unwrap().id.clone();
    let screen = h.press(KeyCode::Char('p'));
    assert!(screen.contains(&format!("1  row {selected}")), "{screen}");
    assert!(
        screen.contains("column id") && screen.contains("column title"),
        "{screen}"
    );
    h.press(KeyCode::Char('1'));
    let screen = h.type_text("gre");
    assert!(
        screen.contains(&format!("painted rows:id:{selected}=green")),
        "{screen}"
    );
    assert_eq!(cell_fg(&h, &selected), Some(Color::Green));
    assert!(screen.contains("paint:1 · 3/3"), "{screen}");
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('d'));
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(
        file.contains(&format!("paint = \"rows:id:{selected}=green\"")),
        "{file}"
    );
    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.paint.len(), 1);
    assert_eq!(fresh.view_label(), "v1");
}

#[test]
fn p_paints_rows_matching_the_filter_and_columns_and_none_clears() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('p'));
    let screen = h.render();
    assert!(
        !screen.contains("rows status"),
        "cleared filter offers no rows target: {screen}"
    );
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('p'));
    let screen = h.render();
    assert!(screen.contains("2  rows status:todo"), "{screen}");
    h.press(KeyCode::Char('2'));
    h.type_text("yel");
    h.press(KeyCode::Esc);
    let screen = h.render();
    assert!(
        screen.contains("3/3"),
        "filter cleared, paint stays: {screen}"
    );
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Yellow));
    assert_eq!(cell_fg(&h, "Fix login"), Some(Color::Reset));
    h.press(KeyCode::Char('p'));
    h.type_text("column s");
    h.type_text("cy");
    assert_eq!(
        cell_fg(&h, "In Progress"),
        Some(Color::Cyan),
        "column rule paints every row"
    );
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(Color::Yellow),
        "row rule keeps other cells"
    );
    h.press(KeyCode::Char('p'));
    h.type_text("column s");
    h.type_text("no");
    assert_eq!(
        cell_fg(&h, "In Progress"),
        Some(Color::Reset),
        "none clears the column rule"
    );
    assert!(h.render().contains("paint:1 ·"), "{}", h.render());
}

#[test]
fn p_accepts_a_typed_hex_color() {
    use ratatui::style::Color;
    let mut h = Harness::new();
    h.press(KeyCode::Char('p'));
    h.type_text("column t");
    h.type_text("#ff8800");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("painted column:title=#ff8800"), "{screen}");
    assert_eq!(cell_fg(&h, "Add dark theme"), Some(Color::Rgb(255, 136, 0)));
}
