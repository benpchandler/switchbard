//! `,`: standing preferences under every view, per repo with a global fallback.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

#[test]
fn hiding_a_status_applies_under_every_view_and_a_view_that_names_status_wins() {
    let mut h = Harness::new();
    seed(&h.root, "Old finished thing", "Done", &[]);
    h.press(KeyCode::Char('r'));
    assert_eq!(visible_titles(&h).len(), 4);
    let screen = h.press(KeyCode::Char(','));
    assert!(screen.contains("┌ settings ─"), "{screen}");
    assert!(screen.contains(" hide Done"), "{screen}");
    h.type_text("done");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("✓hide Done"),
        "toggled and still open: {screen}"
    );
    assert!(screen.contains("hide:done"), "title says so: {screen}");
    assert_eq!(
        h.app.status,
        "hide:done · this repo · g makes it every repo"
    );
    h.press(KeyCode::Esc);
    assert_eq!(visible_titles(&h).len(), 3);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('2'));
    assert_eq!(
        visible_titles(&h).len(),
        2,
        "status:todo view, Done still hidden"
    );
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.type_text("status:done");
    h.press(KeyCode::Enter);
    assert_eq!(
        visible_titles(&h),
        ["Old finished thing"],
        "a view that asks for Done gets it"
    );
    let file = std::fs::read_to_string(h.root.join("settings-repo.lua")).unwrap();
    assert!(file.contains("hide_statuses = { \"Done\" }"), "{file}");
    let fresh = open_app(&h.root, &h.config_path);
    assert!(fresh.settings.effective().is_hidden("Done"));
}

#[test]
fn g_in_the_panel_promotes_this_repos_settings_to_every_repo() {
    let mut h = Harness::new();
    seed(&h.root, "Old finished thing", "Done", &[]);
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Char(','));
    h.type_text("done");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('g'));
    assert_eq!(h.app.status, "hide:done · every repo");
    assert!(!h.root.join("settings-repo.lua").exists());
    let file = std::fs::read_to_string(h.root.join("settings.lua")).unwrap();
    assert!(file.contains("hide_statuses = { \"Done\" }"), "{file}");
    h.press(KeyCode::Esc);
    assert!(h.render().contains("hide:done"));
}
