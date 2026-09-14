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
    assert!(!h.root.join("views.lua").exists());
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
