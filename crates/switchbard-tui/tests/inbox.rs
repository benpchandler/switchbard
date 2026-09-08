mod harness;
use crossterm::event::KeyCode;
use harness::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use switchbard_tui::page::Page;

#[test]
fn inbox_cycles_and_resumes_both_list_states_without_mutation() {
    let mut h = Harness::new();
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    let tasks = h.app.state.clone();
    h.press(KeyCode::Tab);
    h.type_text("/title:example");
    h.press(KeyCode::Enter);
    let prs = h.app.state.clone();
    let screen = h.press(KeyCode::Tab);
    assert_eq!(h.app.page, Page::Inbox);
    assert!(
        screen.contains("Your review requests and follow-ups will appear here."),
        "{screen}"
    );
    assert!(!screen.contains("Add dark theme"));
    for key in ['1', 't', 'v', 'b', 'w', 'm', '/', 'f', 's', 'p', ','] {
        h.press(KeyCode::Char(key));
        assert!(h.app.picker.is_none());
        assert_eq!(h.app.state, tasks);
    }
    for command in [
        ":goal nope",
        ":group status",
        ":theme berg",
        ":palette auto",
        ":open",
        ":more",
    ] {
        h.type_text(command);
        let screen = h.press(KeyCode::Enter);
        assert!(
            screen.contains("Switch to Tasks or Pull Requests"),
            "{command}: {screen}"
        );
        assert_eq!(h.app.state, tasks);
    }
    h.press(KeyCode::Esc);
    let resume = h.app.resume_state();
    h.app = open_app(&h.root, &h.config_path);
    h.app.resume_from(Some(&resume));
    assert_eq!(h.app.page, Page::Inbox);
    h.type_text(":reload");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.page, Page::Tasks);
    assert_eq!(h.app.state, tasks);
    assert!(h.render().contains("Add dark theme"));
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, prs);
}

#[test]
fn inbox_reports_help_and_narrow_render_are_real() {
    let mut h = Harness::new();
    h.type_text(":page");
    h.press(KeyCode::Enter);
    h.type_text(":page");
    h.press(KeyCode::Enter);
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("Inbox keys"));
    assert!(screen.contains(":bug <description>"));
    assert!(!screen.contains("column actions"));
    h.press(KeyCode::Esc);
    h.type_text(":idea Inbox should retain visible next actions");
    h.press(KeyCode::Enter);
    assert!(h.app.tasks().iter().any(|task| task
        .title
        .contains("Inbox should retain visible next actions")));
    h.terminal = Terminal::new(TestBackend::new(36, 8)).unwrap();
    assert!(h.render().contains("Inbox"));
    h.press(KeyCode::Tab);
    assert_eq!(h.app.page, Page::Tasks);
}

#[test]
fn legacy_page_and_task_records_still_resume() {
    let mut h = Harness::new();
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    let task_state = h.app.state.clone();
    let old_task = format!("1\t0\t{}", task_state.to_lua());
    h.press(KeyCode::Tab);
    let resume = h.app.resume_state();
    let old_pages = format!("pages={}", resume.split_once('\t').unwrap().1);
    h.press(KeyCode::Tab);
    h.app.resume_from(Some(&old_pages));
    assert_eq!(h.app.page, Page::PullRequests);
    h.app.resume_from(Some(&old_task));
    assert_eq!(h.app.page, Page::Tasks);
    assert_eq!(h.app.state, task_state);
}
