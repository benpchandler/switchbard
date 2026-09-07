//! Shared controls on the PR page: real keys, real task files, real GitHub reads.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant};

fn settle(h: &mut Harness) {
    h.app.tick();
    let deadline = Instant::now() + Duration::from_secs(100);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
    }
    assert!(!h.app.pull_requests.loading(), "PR read exceeded its bound");
    h.render();
}

#[test]
fn pr_column_menu_and_saved_layout_are_isolated_from_tasks() {
    let mut h = Harness::new();
    let tasks = h.app.state.clone();
    h.press(KeyCode::Tab);
    let menu = h.press(KeyCode::Char('1'));
    assert!(
        menu.contains("hide it"),
        "PR header must open shared column menu: {menu}"
    );
    assert!(
        !menu.contains("group by it"),
        "PR grouping is not available: {menu}"
    );
    h.press(KeyCode::Char('x'));
    h.type_text("vsd");
    let saved = h.render();
    assert!(saved.contains("saved v1"), "{saved}");
    let resume = h.app.resume_state();
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, tasks, "PR columns must not alter Tasks");
    h.app = open_app(&h.root, &h.config_path);
    assert_eq!(h.app.state, tasks, "PR save must not replace Tasks slot 1");
    h.press(KeyCode::Tab);
    let restored = h.press(KeyCode::Char('1'));
    assert!(
        !restored.contains("┌ id ─"),
        "hidden PR id must stay hidden: {restored}"
    );
    h.press(KeyCode::Esc);
    h.app.resume_from(Some(&resume));
    h.press(KeyCode::Tab);
    assert_eq!(
        h.app.state, tasks,
        "self restart must preserve both page states"
    );
    settle(&mut h);
}

#[test]
fn empty_pr_controls_use_shared_pickers_and_preserve_task_settings() {
    let mut h = Harness::new();
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    let tasks = h.app.state.clone();
    h.press(KeyCode::Tab);
    for (key, heading) in [('f', "filter"), ('s', "sort"), ('p', "paint")] {
        let screen = h.press(KeyCode::Char(key));
        assert!(screen.contains(heading), "{screen}");
        assert!(h.app.picker.is_some(), "{key} must open a picker: {screen}");
        assert!(
            !screen.contains("priority"),
            "no task-only columns: {screen}"
        );
        h.press(KeyCode::Esc);
    }
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, tasks);
    settle(&mut h);
}

fn pick(h: &mut Harness, label: &str) {
    let picker = h.app.picker.as_ref().expect("a shared picker is open");
    let options = picker.matching();
    let index = options
        .iter()
        .position(|o| o.label == label || o.label.starts_with(&format!("{label} ·")))
        .unwrap_or_else(|| {
            panic!(
                "missing {label}: {:?}",
                options.iter().map(|o| &o.label).collect::<Vec<_>>()
            )
        });
    let selected = picker.selected;
    for _ in 0..selected {
        h.press(KeyCode::Up);
    }
    for _ in 0..index {
        h.press(KeyCode::Down);
    }
    h.press(KeyCode::Enter);
}

fn live() -> Harness {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let source = switchbard_core::fetch_pull_requests(std::path::Path::new(&root)).unwrap();
    let mut h = Harness::new();
    for args in [
        vec!["init", "-q"],
        vec!["remote", "add", "origin", &source.repository_url],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&h.root)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    h.press(KeyCode::Tab);
    settle(&mut h);
    assert!(
        h.app.pull_requests.error.is_none(),
        "{:?}",
        h.app.pull_requests.error
    );
    assert!(
        h.app.pull_requests.visible.len() > 10,
        "live repo needs a useful PR history"
    );
    h
}

fn numbers(h: &Harness) -> Vec<u64> {
    let snapshot = h.app.pull_requests.snapshot.as_ref().unwrap();
    h.app
        .pull_requests
        .visible
        .iter()
        .map(|&i| snapshot.rows[i].number)
        .collect()
}

fn set_filter(h: &mut Harness, query: &str) {
    h.press(KeyCode::Char('/'));
    // Replace the current search through ordinary editing keys.
    for _ in 0..h.app.input.chars().count() {
        h.press(KeyCode::Backspace);
    }
    h.type_text(query);
    h.press(KeyCode::Enter);
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; reads only"]
fn live_pr_sort_filter_and_refresh_preserve_identity_and_task_state() {
    let mut h = live();
    let identity = h.app.pull_requests.row().unwrap().id.clone();
    h.type_text("s1");
    pick(&mut h, "ascending");
    assert!(
        numbers(&h).windows(2).all(|p| p[0] < p[1]),
        "numeric ascending PR IDs"
    );
    assert_eq!(h.app.pull_requests.row().unwrap().id, identity);
    h.type_text("s1");
    pick(&mut h, "descending");
    assert!(numbers(&h).windows(2).all(|p| p[0] > p[1]));
    let row = h.app.pull_requests.row().unwrap();
    let number = row.number;
    let lifecycle = row.lifecycle.label().to_string();
    h.type_text("f2");
    pick(&mut h, &lifecycle);
    assert!(numbers(&h).contains(&number));
    assert_eq!(h.app.pull_requests.row().unwrap().id, identity);
    h.press(KeyCode::Char('r'));
    settle(&mut h);
    assert_eq!(h.app.pull_requests.row().unwrap().id, identity);
    assert!(numbers(&h).windows(2).all(|p| p[0] > p[1]));
    set_filter(&mut h, "id:999999999999");
    assert!(h.app.pull_requests.visible.is_empty());
    let empty = h.render();
    assert!(
        empty.contains("No PRs match") || empty.contains("No matches"),
        "{empty}"
    );
    set_filter(&mut h, "");
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state.filter, "");
    assert!(h.app.state.sort.is_none());
}

#[test]
fn task_reload_while_pr_page_is_active_keeps_task_filter_sort_and_selection() {
    let mut h = Harness::new();
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    h.type_text("s41");
    let task_state = h.app.state.clone();
    let selected = h.selected_title();
    h.press(KeyCode::Tab);
    set_filter(&mut h, "status:merged");
    seed(&h.root, "Another theme task", "To Do", &["ui"]);
    h.app.tick();
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, task_state);
    assert_eq!(h.selected_title(), selected);
    assert!(h.render().contains("Another theme task"));
    h.press(KeyCode::Tab);
    assert_eq!(h.app.pull_requests.filter, "status:merged");
    settle(&mut h);
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; reads only"]
fn live_pr_paint_links_and_saved_view_survive_page_switch_and_restart() {
    let mut h = live();
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(180, 50)).unwrap();
    let row = h.app.pull_requests.row().unwrap();
    let number = row.number;
    let url = row.url.clone();
    let task_id = h.app.selected_task().unwrap().id.clone();
    switchbard_core::edit_backlog_task(
        &h.root,
        &task_id,
        &switchbard_core::BacklogTaskPatch {
            references: Some(vec![url]),
            ..Default::default()
        },
    )
    .unwrap();
    h.app.tick();
    set_filter(&mut h, &format!("tasks:{task_id}"));
    assert_eq!(
        numbers(&h),
        [number],
        "linked task filter uses real disk references"
    );
    h.type_text("pr");
    pick(&mut h, "red");
    if h.app.picker.is_some() {
        h.press(KeyCode::Esc);
    }
    assert_eq!(
        cell_fg(&h, &format!("#{number}")),
        Some(ratatui::style::Color::Red),
        "{}",
        h.render()
    );
    h.type_text("pc");
    pick(&mut h, "title");
    pick(&mut h, "blue");
    if h.app.picker.is_some() {
        h.press(KeyCode::Esc);
    }
    h.type_text("s1");
    pick(&mut h, "ascending");
    let pr_state = h.app.state.clone();
    assert!(
        pr_state.filter.contains(&task_id),
        "paint must retain the PR filter"
    );
    assert_eq!(pr_state.paint.len(), 2);
    h.type_text("vsd");
    h.press(KeyCode::Tab);
    assert!(h.app.state.paint.is_empty());
    assert_eq!(h.app.state.filter, "");
    h.app = open_app(&h.root, &h.config_path);
    h.press(KeyCode::Tab);
    settle(&mut h);
    assert_eq!(
        h.app.state, pr_state,
        "saved PR filter/columns/paint restore independently"
    );
    assert_eq!(numbers(&h), [number]);
    assert_eq!(
        cell_fg(&h, &format!("#{number}")),
        Some(ratatui::style::Color::Red)
    );
    h.type_text("pd");
    h.press(KeyCode::Esc);
    assert!(h.app.state.paint.is_empty());
    assert_ne!(
        cell_fg(&h, &format!("#{number}")),
        Some(ratatui::style::Color::Red)
    );
}

#[test]
#[ignore = "requires SBT_PR_REPO and SBT_WORK_TASK naming a live claimed task; reads only"]
fn live_claimed_task_is_visible_on_tasks_page() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let id = std::env::var("SBT_WORK_TASK").expect("SBT_WORK_TASK");
    let mut h = Harness::new();
    h.app = switchbard_tui::app::App::open(
        std::path::Path::new(&root),
        switchbard_tui::app::AppPaths {
            config: Some(h.config_path.clone()),
            global_views: Some(h.root.join("views.lua")),
            repo_views: Some(h.root.join("views-repo.lua")),
            global_settings: Some(h.root.join("settings.lua")),
            repo_settings: Some(h.root.join("settings-repo.lua")),
            work_dir: switchbard_core::default_work_dir(),
        },
        switchbard_tui::telemetry::Telemetry::in_memory(),
    );
    set_filter(&mut h, &format!("id:{id}"));
    let task = h.app.selected_task().expect("claimed task is visible");
    assert_eq!(task.id.to_lowercase(), id.to_lowercase());
    assert_eq!(task.status, "In Progress");
    assert!(
        !h.app.working(task).is_empty(),
        "real work store must show the live claim"
    );
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("working ·"), "{screen}");
    println!("{screen}");
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; reads only"]
fn live_pr_selected_identity_survives_self_restart() {
    let mut h = live();
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('j'));
    let identity = h.app.pull_requests.row().unwrap().id.clone();
    let resume = h.app.resume_state();
    h.app = open_app(&h.root, &h.config_path);
    h.app.resume_from(Some(&resume));
    settle(&mut h);
    assert_eq!(h.app.pull_requests.row().unwrap().id, identity);
    assert!(h.render().contains("[Pull Requests]"));
}

#[test]
fn pr_columns_reorder_and_global_save_do_not_change_tasks() {
    let mut h = Harness::new();
    let tasks = h.app.state.clone();
    h.press(KeyCode::Tab);
    h.type_text("cm21");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Esc);
    assert_eq!(h.app.state.columns[0].name(), "lifecycle");
    assert_eq!(h.app.state.columns[1].name(), "id");
    let menu = h.press(KeyCode::Char('1'));
    assert!(
        menu.contains("lifecycle"),
        "number targets displayed field: {menu}"
    );
    h.press(KeyCode::Esc);
    h.type_text("vsdvgd");
    assert!(h.render().contains("global"));
    h.app = open_app(&h.root, &h.config_path);
    assert_eq!(h.app.state, tasks);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state.columns[0].name(), "lifecycle");
    let mut other = Harness::new();
    other.app = switchbard_tui::app::App::open(
        &other.root,
        switchbard_tui::app::AppPaths {
            config: Some(other.config_path.clone()),
            global_views: Some(h.root.join("views.lua")),
            repo_views: Some(other.root.join("views-repo.lua")),
            ..Default::default()
        },
        switchbard_tui::telemetry::Telemetry::in_memory(),
    );
    assert_eq!(other.app.state, tasks);
    other.press(KeyCode::Tab);
    assert_eq!(
        other.app.state.columns[0].name(),
        "lifecycle",
        "global PR slot applies to another repo only on PR page"
    );
    settle(&mut other);
    settle(&mut h);
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; reads only"]
fn live_pr_historical_facets_and_paint_rule_order_use_shared_controls() {
    let mut h = live();
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(180, 50)).unwrap();
    h.press(KeyCode::Char('f'));
    pick(&mut h, "checks");
    pick(&mut h, "Not fetched");
    assert!(!h.app.pull_requests.visible.is_empty());
    let snapshot = h.app.pull_requests.snapshot.as_ref().unwrap();
    assert!(h
        .app
        .pull_requests
        .visible
        .iter()
        .all(|&i| snapshot.rows[i].lifecycle != switchbard_core::PrLifecycle::Open));
    h.type_text("s2");
    let menu = h.render();
    assert!(menu.contains("semantic"));
    h.press(KeyCode::Char('1'));
    let number = h.app.pull_requests.row().unwrap().number;
    h.type_text("p21");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.state.paint.len(), 1);
    let painted = cell_fg(&h, &format!("#{number}")).unwrap();
    h.type_text("p4");
    pick(&mut h, "Not fetched");
    pick(&mut h, "blue");
    h.press(KeyCode::Esc);
    assert_eq!(cell_fg(&h, &format!("#{number}")), Some(painted));
    h.type_text("po");
    h.press(KeyCode::Down);
    h.app.handle_key(crossterm::event::KeyEvent::new(
        KeyCode::Char('K'),
        crossterm::event::KeyModifiers::SHIFT,
    ));
    h.press(KeyCode::Esc);
    assert_eq!(
        cell_fg(&h, &format!("#{number}")),
        Some(ratatui::style::Color::Blue),
        "new top rule paints the row"
    );
    h.type_text("po");
    h.press(KeyCode::Delete);
    h.press(KeyCode::Esc);
    assert_eq!(
        cell_fg(&h, &format!("#{number}")),
        Some(painted),
        "deleting base restores categorical row color"
    );
}
