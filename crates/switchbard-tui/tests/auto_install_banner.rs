//! sbt's one-line startup banner (TASK-227): what `App::show_startup_banner`
//! renders on screen from a real `scripts/install-switchbard.sh` receipt or
//! hold file, and that it says nothing when there is nothing to say.

mod harness;

use harness::*;

fn write(dir: &std::path::Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).unwrap();
}

#[test]
fn an_ordinary_launch_with_no_receipt_shows_no_banner() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    h.app = open_app_with_auto_install(&h.root, &h.config_path, Some(dir.path().to_path_buf()));
    h.app.restore_session(None, false);
    h.app.show_startup_banner(false, None);
    assert_eq!(h.app.status, "");
    let screen = h.render();
    assert!(!screen.contains("auto-install"), "{screen}");
    assert!(!screen.contains("holding"), "{screen}");
}

#[test]
fn an_active_hold_names_the_branch_and_a_resolving_command() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let until = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    write(
        dir.path(),
        "hold.json",
        &format!(r#"{{"branch": "feat/live-preview", "until": "{until}"}}"#),
    );
    h.app = open_app_with_auto_install(&h.root, &h.config_path, Some(dir.path().to_path_buf()));
    h.app.restore_session(None, false);
    h.app.show_startup_banner(false, None);
    assert!(
        h.app.status.contains("feat/live-preview"),
        "{}",
        h.app.status
    );
    assert!(h.app.status.contains("--force"), "{}", h.app.status);
    let screen = h.render();
    assert!(screen.contains("feat/live-preview"), "{screen}");
}

#[test]
fn a_refused_receipt_names_the_branch_reason_and_a_resolving_command() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "last-install.json",
        r#"{"outcome": "refused", "reason": "would drop 3 commits already running", "from_branch": "feat/queue-drain"}"#,
    );
    h.app = open_app_with_auto_install(&h.root, &h.config_path, Some(dir.path().to_path_buf()));
    h.app.restore_session(None, false);
    h.app.show_startup_banner(false, None);
    assert!(
        h.app.status.contains("feat/queue-drain"),
        "{}",
        h.app.status
    );
    assert!(
        h.app
            .status
            .contains("would drop 3 commits already running"),
        "{}",
        h.app.status
    );
    assert!(
        h.app.status.contains("mise run install --force"),
        "{}",
        h.app.status
    );
    let screen = h.render();
    assert!(screen.contains("feat/queue-drain"), "{screen}");
}

#[test]
fn a_self_restart_shows_the_new_build_and_the_one_it_replaced() {
    let mut h = Harness::new();
    h.app.restore_session(Some(&h.app.resume_state()), false);
    h.app
        .show_startup_banner(true, Some("cafefeedcafefeedcafefeedcafefeedcafefeed"));
    assert!(h.app.status.starts_with("updated to "), "{}", h.app.status);
    assert!(h.app.status.contains("(from cafefeed)"), "{}", h.app.status);
    let screen = h.render();
    assert!(screen.contains("updated to "), "{screen}");
    assert!(screen.contains("(from cafefeed)"), "{screen}");
}

#[test]
fn a_self_restart_with_no_previous_build_known_still_names_the_new_one() {
    let mut h = Harness::new();
    h.app.restore_session(Some(&h.app.resume_state()), false);
    h.app.show_startup_banner(true, None);
    assert!(h.app.status.starts_with("updated to "), "{}", h.app.status);
    assert!(!h.app.status.contains("(from"), "{}", h.app.status);
}

#[test]
fn a_restart_takes_priority_over_a_pending_hold() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir().unwrap();
    let until = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    write(
        dir.path(),
        "hold.json",
        &format!(r#"{{"branch": "feat/other", "until": "{until}"}}"#),
    );
    h.app = open_app_with_auto_install(&h.root, &h.config_path, Some(dir.path().to_path_buf()));
    let restart = h.app.resume_state();
    h.app.restore_session(Some(&restart), false);
    h.app.show_startup_banner(true, None);
    assert!(h.app.status.starts_with("updated to "), "{}", h.app.status);
    assert!(!h.app.status.contains("feat/other"), "{}", h.app.status);
}
