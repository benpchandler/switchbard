//! Real application launches sharing only durable per-repository session files.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_tui::views::ViewState;

#[test]
fn cold_launch_restores_both_pages_and_every_view_field() {
    let mut h = Harness::new();
    h.app.state = ViewState::try_from_lua(
        "{filter='status:todo',sort='priority:semantic',columns='id,status,title',glyphs='status',abbreviated='',group='status',pin=false,name='日本語 view',paint='by:status=todo:p1,inprogress:p2',title_lines=3,row_spacing=1}",
        h.app.registry(),
    ).expect("all-field fixture");
    h.type_text(":group status");
    h.press(KeyCode::Enter);
    let task = h.app.state.clone();
    h.next_list_page();
    h.type_text(":group status");
    h.press(KeyCode::Enter);
    let pr = h.app.state.clone();
    let before = h.app.resume_state();
    h.app.checkpoint_session().expect("checkpoint");
    h.app = open_app(&h.root, &h.config_path);
    h.app.restore_session(None, false);
    assert_eq!(h.app.resume_state(), before);
    assert_eq!(h.app.state, pr);
    assert!(h.render().contains("[Pull Requests]"));
    h.next_list_page();
    assert_eq!(h.app.state, task);
    assert!(!h.root.join("views-repo.lua").exists());
    assert!(std::fs::read_to_string(h.root.join("views.lua")).unwrap() == harness::LEGACY_VIEWS);
}

#[test]
fn first_launch_and_fresh_keep_deliberately_saved_default() {
    let mut h = Harness::new();
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    h.type_text("vsd");
    let views = std::fs::read(h.root.join("views-repo.lua")).expect("saved default");
    let mut first = open_app(&h.root, &h.config_path);
    first.restore_session(None, false);
    assert_eq!(first.state.filter, "theme");
    first.state.filter = "login".into();
    first.checkpoint_session().expect("checkpoint");
    let mut resumed = open_app(&h.root, &h.config_path);
    resumed.restore_session(None, false);
    assert_eq!(resumed.state.filter, "login");
    let mut fresh = open_app(&h.root, &h.config_path);
    fresh.restore_session(None, true);
    assert_eq!(fresh.state.filter, "theme");
    assert_eq!(
        std::fs::read(h.root.join("views-repo.lua")).expect("default"),
        views
    );
}

#[test]
fn self_restart_takes_precedence_even_when_launched_fresh() {
    let h = Harness::new();
    let mut app = open_app(&h.root, &h.config_path);
    app.state.filter = "disk".into();
    app.checkpoint_session().expect("checkpoint");
    app.state.filter = "restart".into();
    let restart = app.resume_state();
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(Some(&restart), true);
    assert_eq!(next.state.filter, "restart");
}

#[test]
fn malformed_cold_record_is_reported_and_not_overwritten() {
    let h = Harness::new();
    let path = h.root.join("views-repo.resume");
    let source = "sbt-resume-1={\"task_view\":\"not valid Lua!!!\"}";
    std::fs::write(&path, source).expect("invalid source");
    let mut app = open_app(&h.root, &h.config_path);
    app.restore_session(None, false);
    assert!(!app.status.is_empty());
    assert!(app.checkpoint_session().is_err());
    assert_eq!(
        std::fs::read_to_string(path).expect("preserved record"),
        source
    );
}

#[test]
fn fresh_launch_does_not_authorize_overwriting_unreadable_resume_views() {
    let h = Harness::new();
    let path = h.root.join("views-repo.resume");
    let source = "sbt-resume-1={\"task_view\":\"not valid Lua!!!\"}";
    std::fs::write(&path, source).expect("invalid source");
    let mut app = open_app(&h.root, &h.config_path);
    app.restore_session(None, true);
    assert!(app.checkpoint_session().is_err());
    assert_eq!(
        std::fs::read_to_string(path).expect("preserved record"),
        source
    );
}

#[test]
fn a_new_session_restores_a_committed_view_while_the_previous_stays_open() {
    let mut h = Harness::new();
    h.type_text("/login");
    h.press(KeyCode::Enter);
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    h.app = next;
    let screen = h.render();
    assert!(screen.contains("login"), "{screen}");
    assert!(!screen.contains("Add dark theme"), "{screen}");
    assert_eq!(h.app.state.filter, "login");
}

#[test]
fn drafts_and_cancelled_filters_never_replace_the_committed_arrangement() {
    let mut h = Harness::new();
    h.type_text("/login");
    h.press(KeyCode::Enter);
    let committed = std::fs::read(h.root.join("views-repo.resume")).expect("committed view");
    h.type_text("/unfinished");
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    assert_eq!(next.state.filter, "login");
    h.app
        .checkpoint_session()
        .expect("timer captures committed view");
    next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    assert_eq!(next.state.filter, "login");
    h.press(KeyCode::Esc);
    assert!(h.render().contains("login"));
    assert_eq!(
        std::fs::read(h.root.join("views-repo.resume")).expect("unchanged"),
        committed
    );
}

#[test]
fn committed_sort_columns_layout_and_page_resume_without_exit_or_timer() {
    let mut h = Harness::new();
    h.type_text("s31c5");
    h.press(KeyCode::Esc);
    h.type_text("vl");
    h.press(KeyCode::Esc);
    let expected = h.app.state.clone();
    h.next_list_page();
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    assert_eq!(next.page, switchbard_tui::page::Page::PullRequests);
    h.app = next;
    h.next_list_page();
    assert_eq!(h.app.state, expected);
    let screen = h.render();
    assert!(screen.contains("labels"), "{screen}");
    assert!(screen.contains("≈pri"), "{screen}");
}

#[test]
fn idle_navigation_does_not_write_and_stale_sessions_preserve_newer_view() {
    let mut h = Harness::new();
    h.type_text("/login");
    h.press(KeyCode::Enter);
    let path = h.root.join("views-repo.resume");
    let before = std::fs::metadata(&path)
        .expect("checkpoint")
        .modified()
        .expect("mtime");
    h.type_text("jk?");
    h.press(KeyCode::Esc);
    assert_eq!(
        std::fs::metadata(&path)
            .expect("checkpoint")
            .modified()
            .expect("mtime"),
        before
    );
    let mut newer = open_app(&h.root, &h.config_path);
    newer.restore_session(None, false);
    let old = std::mem::replace(&mut h.app, newer);
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    let authoritative = std::fs::read(&path).expect("newer checkpoint");
    h.app = old;
    h.type_text("/guide");
    h.press(KeyCode::Enter);
    assert!(h.render().contains("another session"));
    assert_eq!(std::fs::read(&path).expect("preserved"), authoritative);
}

#[test]
fn a_different_repository_keeps_its_default_when_another_view_is_active() {
    let mut first = Harness::new();
    first.type_text("/login");
    first.press(KeyCode::Enter);
    let mut second = Harness::new();
    second.app.restore_session(None, false);
    let screen = second.render();
    assert_eq!(second.app.state.filter, "");
    assert!(screen.contains("Add dark theme"), "{screen}");
    assert!(!second.root.join("views-repo.resume").exists());
}

#[test]
fn timer_during_a_filter_draft_preserves_the_committed_selected_task() {
    let mut h = Harness::new();
    h.type_text("j");
    let selected = h.selected_title();
    assert_ne!(h.app.selected, 0);
    h.app.checkpoint_session().expect("committed selection");
    h.type_text("/login");
    assert_eq!(h.app.selected, 0, "draft narrows the preview");
    h.app.checkpoint_session().expect("draft timer");
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    h.app = next;
    assert_eq!(h.selected_title(), selected);
    assert!(h.render().contains("Add dark theme"));
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO with at least two PRs; reads only"]
fn pr_filter_draft_timer_preserves_committed_selection_identity() {
    let repo = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let mut h = Harness::new();
    let source = switchbard_core::fetch_pull_requests(std::path::Path::new(&repo))
        .expect("read-only public PR source");
    for args in [
        vec!["init", "-q"],
        vec!["remote", "add", "origin", &source.repository_url],
    ] {
        assert!(switchbard_core::git_cmd()
            .arg("-C")
            .arg(&h.root)
            .args(args)
            .status()
            .expect("fixture git command")
            .success());
    }
    h.next_list_page();
    for _ in 0..500 {
        h.app.tick();
        if !h.app.pull_requests.loading() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        h.app.pull_requests.visible.len() >= 2,
        "two real PRs required: {}",
        h.render()
    );
    h.type_text("j");
    let identity = h.app.pull_requests.selection_identity();
    assert_ne!(h.app.pull_requests.selected, 0);
    h.app.checkpoint_session().expect("committed PR cursor");
    h.type_text("/title:__sbt_no_matching_pr_288__");
    assert!(h.app.pull_requests.visible.is_empty());
    h.app.checkpoint_session().expect("draft timer");
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    assert_eq!(next.state.filter, "");
    assert_eq!(next.pull_requests.selection_identity(), identity);
    h.press(KeyCode::Esc);
    assert_eq!(h.app.pull_requests.selection_identity(), identity);
    h.app.checkpoint_session().expect("cancelled draft");
    h.app = next;
    for _ in 0..500 {
        h.app.tick();
        if !h.app.pull_requests.loading() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(h.app.pull_requests.selection_identity(), identity);
    assert!(h.render().contains("[Pull Requests]"));
}

#[test]
fn free_text_column_filter_uses_the_same_committed_baseline() {
    let mut h = Harness::new();
    h.type_text("/guide");
    h.press(KeyCode::Enter);
    h.type_text("f4");
    assert_eq!(h.app.mode, switchbard_tui::app::Mode::Filter);
    h.type_text("draft");
    h.app.checkpoint_session().expect("free text draft timer");
    let mut next = open_app(&h.root, &h.config_path);
    next.restore_session(None, false);
    assert_eq!(next.state.filter, "guide");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.state.filter, "guide");
    assert!(h.render().contains("Write onboarding guide"));
}
