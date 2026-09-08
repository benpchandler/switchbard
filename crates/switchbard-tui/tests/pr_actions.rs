mod harness;
use crossterm::event::KeyCode;
use harness::*;

#[test]
fn browser_action_requires_a_selected_pr_and_leaves_tasks_unchanged() {
    let mut h = Harness::new();
    let selected = h.selected_title();
    assert!(h
        .press(KeyCode::Char('O'))
        .contains("Switch to Pull Requests"));
    h.press(KeyCode::Tab);
    assert!(h.press(KeyCode::Char('O')).contains("No PR selected"));
    h.type_text(":open");
    assert!(h.press(KeyCode::Enter).contains("No PR selected"));
    h.press(KeyCode::Tab);
    assert_eq!(h.selected_title(), selected);
}

#[test]
#[ignore = "opens the actual selected GitHub PR in the default browser; requires SBT_PR_REPO"]
fn selected_pr_opens_in_the_real_browser() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let mut h = Harness::new();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.press(KeyCode::Tab);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(100);
    while h.app.pull_requests.loading() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.app.tick();
    }
    let url = h.app.pull_requests.row().expect("actual PR").url.clone();
    h.press(KeyCode::Char('O'));
    assert_eq!(h.app.status, format!("Opened {url}"));
    assert!(h.render().contains("Opened https://github.com/"));
}
