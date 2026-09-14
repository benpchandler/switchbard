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
    h.type_text(":group status");
    h.press(KeyCode::Enter);
    h.type_text("p21");
    h.press(KeyCode::Esc);
    h.app.checkpoint_session().expect("capture");
    h.type_text("v2");
    let live = h.app.resume_state();
    let selected = h.app.selected;
    let screen = h.type_text("vh");
    assert!(screen.contains("Tasks · grouped by Status"), "{screen}");
    assert!(screen.contains("Current data · 3 matches"), "{screen}");
    assert!(screen.contains("Fix login redirect loop"), "{screen}");
    assert!(screen.contains("▸ To Do"), "{screen}");
    assert!(!screen.contains("painted by"), "{screen}");
    assert_eq!(
        cell_fg(&h, "Add dark theme"),
        Some(ratatui::style::Color::Rgb(0xf4, 0x9f, 0x31))
    );
    assert_eq!(h.app.resume_state(), live);
    assert_eq!(h.app.selected, selected);
    capture(&h, "tasks-grouped");
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
    h.app = switchbard_tui::app::App::open(
        std::path::Path::new(&repo),
        switchbard_tui::app::AppPaths {
            config: Some(h.config_path.clone()),
            repo_views: Some(h.root.join("views-repo.lua")),
            ..Default::default()
        },
        switchbard_tui::telemetry::Telemetry::in_memory(),
    );
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
    h.type_text("/status:open");
    h.press(KeyCode::Enter);
    h.app.checkpoint_session().expect("capture open PRs");
    let before = h.app.resume_state();
    let screen = h.type_text("vh");
    assert!(screen.contains("Open pull requests"), "{screen}");
    assert_eq!(h.app.resume_state(), before);
    capture(&h, "pull-requests");
}
