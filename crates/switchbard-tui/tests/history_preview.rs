//! Real menus preview current data without restoring the selected arrangement.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, Terminal};

fn capture(h: &Harness, name: &str) {
    let Some(directory) = std::env::var_os("SBT_HISTORY_PREVIEW_CAPTURE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).expect("capture directory");
    let buffer = h.terminal.backend().buffer();
    let cells = buffer.content.iter().map(|cell| serde_json::json!({
        "symbol": cell.symbol(), "fg":format!("{:?}",cell.fg), "bg":format!("{:?}",cell.bg), "modifier":format!("{:?}",cell.modifier)
    })).collect::<Vec<_>>();
    let data =
        serde_json::json!({"width":buffer.area.width,"height":buffer.area.height,"cells":cells});
    std::fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec(&data).expect("json"),
    )
    .expect("capture");
}

#[test]
fn colored_grouped_preview_shows_current_tasks_without_changing_live_view() {
    let mut h = Harness::new();
    h.terminal = Terminal::new(TestBackend::new(120, 30)).expect("context terminal");
    h.type_text(":group status");
    h.press(KeyCode::Enter);
    h.type_text("p21");
    h.press(KeyCode::Esc);
    h.app.checkpoint_session().expect("capture");
    h.type_text("v2");
    let live = h.app.resume_state();
    let selected = h.app.selected;
    capture(&h, "tasks-1-normal-list");
    h.type_text("v");
    capture(&h, "tasks-2-views-menu");
    let screen = h.type_text("h");
    capture(&h, "tasks-3-history-preview");
    assert!(screen.contains("Tasks · grouped by Status"), "{screen}");
    assert!(screen.contains("Current data · 3 matches"), "{screen}");
    assert!(screen.contains("Write onboarding guide"), "{screen}");
    assert!(screen.contains("▸ To Do"), "{screen}");
    assert!(!screen.contains("painted by"), "{screen}");
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(ratatui::style::Color::Rgb(0xf4, 0x9f, 0x31))
    );
    assert_eq!(h.app.resume_state(), live);
    assert_eq!(h.app.selected, selected);
    capture(&h, "tasks-grouped");
    h.press(KeyCode::Enter);
    capture(&h, "tasks-4-restored-list");
    h.type_text("v2");
    h.press(KeyCode::Esc);
    h.type_text("/login");
    h.press(KeyCode::Enter);
    h.app.checkpoint_session().expect("second capture");
    let live = h.app.resume_state();
    h.type_text("vh");
    h.press(KeyCode::Down);
    assert_eq!(h.app.resume_state(), live);
    capture(&h, "tasks-choices");
    h.press(KeyCode::Enter);
    assert!(h.render().contains("history restored"));
}

#[test]
fn empty_preview_is_honest_and_short_container_remains_operable() {
    let mut h = Harness::new();
    h.type_text("/does-not-exist");
    h.press(KeyCode::Enter);
    h.app.checkpoint_session().expect("capture");
    h.type_text("vh");
    assert!(h.render().contains("No current tasks match this view"));
    capture(&h, "tasks-empty");
    for (width, height) in [(40, 8), (80, 16), (140, 30)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        assert!(h.render().contains("Current data"));
    }
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; read-only actual PR observations"]
fn live_pr_preview_uses_actual_cached_rows_and_keeps_selection() {
    use std::time::{Duration, Instant};
    let repo = std::env::var_os("SBT_PR_REPO").expect("live repository");
    let mut h = Harness::new();
    h.terminal = Terminal::new(TestBackend::new(120, 30)).expect("context terminal");
    h.app = switchbard_tui::app::App::open(
        std::path::Path::new(&repo),
        switchbard_tui::app::AppPaths {
            config: Some(h.config_path.clone()),
            repo_views: Some(h.root.join("views-repo.lua")),
            ..Default::default()
        },
        switchbard_tui::telemetry::Telemetry::in_memory(),
    );
    h.terminal = Terminal::new(TestBackend::new(120, 30)).expect("cards terminal");
    h.next_list_page();
    let deadline = Instant::now() + Duration::from_secs(90);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        h.app.tick();
    }
    assert!(
        h.app.pull_requests.error.is_none(),
        "{:?}",
        h.app.pull_requests.error
    );
    assert!(
        !h.app.pull_requests.visible.is_empty(),
        "live repository needs PR observations"
    );
    h.type_text("p21");
    h.press(KeyCode::Esc);
    h.app.checkpoint_session().expect("capture PR view");
    let before = h.app.resume_state();
    let visible = h.app.pull_requests.visible.clone();
    let screen = h.type_text("vh");
    assert!(screen.contains("Current data"), "{screen}");
    assert!(screen.contains("cached PRs"), "{screen}");
    if h.app
        .pull_requests
        .snapshot
        .as_ref()
        .expect("snapshot")
        .truncated
    {
        assert!(screen.contains("partial data"), "{screen}");
    }
    assert!(screen.contains('#'), "{screen}");
    assert_eq!(h.app.resume_state(), before);
    assert_eq!(h.app.pull_requests.visible, visible);
    capture(&h, "pull-requests-all");
    h.press(KeyCode::Esc);
    h.type_text("/status:merged");
    h.press(KeyCode::Enter);
    h.app.checkpoint_session().expect("capture merged PRs");
    h.type_text("v1");
    capture(&h, "prs-1-normal-list");
    let before = h.app.resume_state();
    h.type_text("v");
    capture(&h, "prs-2-views-menu");
    let screen = h.type_text("h");
    h.press(KeyCode::Down);
    capture(&h, "prs-3-history-preview");
    assert!(screen.contains("Merged pull requests"), "{screen}");
    assert_eq!(h.app.resume_state(), before);
    assert!(
        screen.matches("Current data").count() >= 2,
        "two cards: {screen}"
    );
    assert!(
        screen.matches("1 id").count() >= 2
            || screen.matches("1 #").count() >= 2
            || screen.matches("1 PR").count() >= 2,
        "independent PR headers: {screen}"
    );
    assert!(
        !screen.contains("No current pull requests match"),
        "both miniatures need rows: {screen}"
    );
    capture(&h, "pull-requests-cards");
    h.press(KeyCode::Enter);
    assert!(h.render().contains("history restored"));
    capture(&h, "prs-4-restored-list");
}

#[test]
fn every_visible_history_entry_has_its_own_table_before_selection() {
    let mut h = Harness::new();
    h.terminal = Terminal::new(TestBackend::new(120, 30)).expect("ordinary terminal");
    for query in ["login", "theme"] {
        h.type_text(&format!("/{query}"));
        h.press(KeyCode::Enter);
        h.app.checkpoint_session().expect("capture separate view");
        h.press(KeyCode::Esc);
    }
    let before = h.app.resume_state();
    let screen = h.type_text("vh");
    assert!(
        screen.matches("Current data").count() >= 2,
        "each card needs a miniature: {screen}"
    );
    assert!(
        screen.contains("Fix login redirect loop"),
        "unselected miniature missing: {screen}"
    );
    assert!(
        screen.contains("Add dark theme"),
        "selected miniature missing: {screen}"
    );
    assert!(
        screen.matches("1 id").count() >= 2,
        "independent table headers: {screen}"
    );
    assert_eq!(h.app.resume_state(), before);
    assert_eq!(
        cell_fg(&h, "Tasks matching “login”"),
        h.app
            .config
            .theme
            .style(switchbard_tui::config::Surface::Text)
            .fg
    );
    assert_eq!(
        cell_fg(&h, "view history"),
        h.app
            .config
            .theme
            .style(switchbard_tui::config::Surface::Text)
            .fg
    );
    assert_eq!(
        cell_fg(&h, "4 columns"),
        h.app
            .config
            .theme
            .style(switchbard_tui::config::Surface::Hint)
            .fg
    );
    capture(&h, "tasks-cards");
}

#[test]
fn nested_group_card_keeps_heading_context_and_actual_task_rows() {
    let mut h = Harness::new();
    h.type_text(":group status,priority,ball,blocked");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.state.group.levels().len(), 4, "four-level fixture");
    h.app.checkpoint_session().expect("capture nested view");
    let before = h.app.resume_state();
    let screen = h.type_text("vh");
    assert!(screen.contains("Current data"), "{screen}");
    assert!(
        screen.contains('›'),
        "nested values form a breadcrumb: {screen}"
    );
    assert!(
        screen.contains("Write onboarding guide")
            || screen.contains("Add dark theme")
            || screen.contains("Fix login redirect loop"),
        "miniature must show data, not only headings: {screen}"
    );
    assert_eq!(h.app.resume_state(), before);
    capture(&h, "tasks-nested-card");
}
