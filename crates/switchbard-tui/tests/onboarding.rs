//! Onboarding contract exercised through the real binary and isolated databases.
use std::path::Path;
use std::process::{Command, Output};
use switchbard_core::storage::Store;

fn run(root: &Path, database: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sbt"))
        .arg("--repo")
        .arg(root)
        .args(args)
        .env("SWITCHBARD_DATABASE", database)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run sbt")
}

#[test]
fn explicit_setup_activates_all_kinds_without_files_and_is_idempotent() {
    let root = tempfile::tempdir().expect("repo");
    let state = tempfile::tempdir().expect("state");
    let database = state.path().join("central.sqlite3");
    let output = run(root.path(), &database, &["init", "--yes"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let store = Store::open_existing(&database)
        .expect("store")
        .expect("created");
    let identity = store
        .repository(root.path())
        .expect("identity")
        .expect("registered");
    for kind in [
        "initiative",
        "project",
        "config",
        "ranking",
        "goals",
        "task",
    ] {
        assert!(store.authority(&identity, kind).expect("authority"));
    }
    let sequence = store.change_sequence().expect("sequence");
    assert!(!root.path().join("backlog").exists());
    assert!(run(root.path(), &database, &["init", "--yes"])
        .status
        .success());
    assert_eq!(
        store.repository(root.path()).expect("identity"),
        Some(identity)
    );
    assert_eq!(store.change_sequence().expect("sequence"), sequence);
}

#[test]
fn unattended_missing_workspace_has_actionable_guidance_and_no_writes() {
    let root = tempfile::tempdir().expect("repo");
    let database = root.path().join("central.sqlite3");
    let output = run(root.path(), &database, &[]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("No SBT workspace"));
    assert!(error.contains("init --yes"));
    assert!(!error.contains("has no backlog"));
    assert!(!database.exists());
}

#[test]
fn legacy_workspace_setup_refuses_instead_of_hiding_records() {
    let root = tempfile::tempdir().expect("repo");
    std::fs::create_dir_all(root.path().join("backlog/tasks")).expect("legacy");
    std::fs::write(
        root.path().join("backlog/config.yml"),
        "project_name: Existing\n",
    )
    .expect("config");
    let database = root.path().join("central.sqlite3");
    let output = run(root.path(), &database, &["init", "--yes"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reviewed migration"));
    assert!(!database.exists());
}

#[test]
fn corruption_is_not_an_invitation_to_reinitialize() {
    let root = tempfile::tempdir().expect("repo");
    let database = root.path().join("central.sqlite3");
    std::fs::write(&database, b"corrupt established data").expect("corrupt fixture");
    let output = run(root.path(), &database, &["init", "--yes"]);
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read(&database).expect("preserved"),
        b"corrupt established data"
    );
    assert!(!root.path().join("backlog").exists());
}

#[test]
fn yes_without_setup_is_rejected_before_mutation() {
    let root = tempfile::tempdir().expect("repo");
    let database = root.path().join("central.sqlite3");
    assert!(!run(root.path(), &database, &["--yes"]).status.success());
    assert!(!database.exists());
}

#[test]
fn interactive_confirmation_default_decline_eof_and_accept() {
    let root = tempfile::tempdir().expect("repo");
    let script = r#"
import os, pty, select, subprocess, sys, time
binary, root = sys.argv[1:]
for index, answer in enumerate([b'\n', b'n\n', b'\x04', b'yes\n']):
    database = os.path.join(root, 'state-' + str(index) + '.sqlite3')
    master, slave = pty.openpty()
    process = subprocess.Popen([binary, '--repo', root, 'init'], stdin=slave, stdout=slave, stderr=slave,
                               env=dict(os.environ, SWITCHBARD_DATABASE=database))
    os.close(slave)
    output = b''
    deadline = time.monotonic() + 10
    while b'Set it up now? [y/N]' not in output:
        assert time.monotonic() < deadline, output
        if select.select([master], [], [], 0.1)[0]: output += os.read(master, 65536)
    os.write(master, answer)
    assert process.wait(timeout=10) == 0, output
    os.close(master)
    assert os.path.exists(database) == (answer == b'yes\n'), (answer, output)
assert not os.path.exists(os.path.join(root, 'backlog'))
"#;
    let output = Command::new("python3")
        .args(["-c", script, env!("CARGO_BIN_EXE_sbt")])
        .arg(root.path())
        .output()
        .expect("PTY journey");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fresh_workspace_real_app_creates_and_changes_status_centrally() {
    if std::env::var_os("SBT_ONBOARDING_APP_CHILD").is_none() {
        let state = tempfile::tempdir().expect("state");
        let output = Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "fresh_workspace_real_app_creates_and_changes_status_centrally",
                "--nocapture",
            ])
            .env("SBT_ONBOARDING_APP_CHILD", "1")
            .env("SWITCHBARD_DATABASE", state.path().join("central.sqlite3"))
            .output()
            .expect("isolated app journey");
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use switchbard_tui::app::{App, AppPaths};
    let root = tempfile::tempdir().expect("repo");
    switchbard_core::setup_repository(root.path()).expect("setup");
    let mut app = App::open(
        root.path(),
        AppPaths {
            config: None,
            global_views: None,
            repo_views: None,
            global_settings: None,
            repo_settings: None,
            work_dir: None,
            auto_install_dir: None,
        },
        switchbard_tui::telemetry::Telemetry::in_memory(),
    );
    for ch in "tnCapture a central task".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.selected_task().expect("created").id, "TASK-1");
    assert_eq!(app.selected_task().expect("created").status, "To Do");
    for ch in "tsDone".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.selected_task().expect("changed").status, "Done");
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 20)).expect("terminal");
    terminal
        .draw(|frame| switchbard_tui::view::draw(frame, &mut app))
        .expect("render");
    let screen = switchbard_tui::view::buffer_text(terminal.backend().buffer());
    assert!(screen.contains("Capture a central task"), "{screen}");
    assert!(screen.contains("Done"), "{screen}");
    let loaded = switchbard_core::load_backlog_repo(root.path()).expect("reload");
    assert_eq!(loaded.tasks.len(), 1);
    assert_eq!(loaded.tasks[0].status, "Done");
    assert_eq!(
        switchbard_core::assignable_statuses(&loaded),
        ["To Do", "In Progress", "Done"]
    );
    assert!(!root.path().join("backlog").exists());
}

#[test]
fn missing_established_database_refuses_reinitialization() {
    let root = tempfile::tempdir().expect("repo");
    let database = root.path().join("central.sqlite3");
    assert!(run(root.path(), &database, &["init", "--yes"])
        .status
        .success());
    std::fs::remove_file(&database).expect("lost database fixture");
    let output = run(root.path(), &database, &["init", "--yes"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("restore its backup"));
    assert!(!database.exists());
}

#[test]
fn registered_legacy_scope_is_not_silently_activated_by_init() {
    let root = tempfile::tempdir().expect("repo");
    let database = root.path().join("central.sqlite3");
    let mut store = Store::open(&database).expect("store");
    let repo = store
        .bind_repository(root.path())
        .expect("bind legacy scope");
    let sequence = store.change_sequence().expect("sequence");
    let output = run(root.path(), &database, &["init", "--yes"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reviewed migration"));
    assert!(!store.authority(&repo, "task").expect("authority"));
    assert_eq!(store.change_sequence().expect("sequence"), sequence);
}
