//! Selected-agent termination uses real OS processes and the real terminal.
mod harness;
use crossterm::event::KeyCode;
use harness::Harness;
use ratatui::{backend::TestBackend, Terminal};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use switchbard_core::{probe_agent_identity, AgentActivity, AgentSession};
use switchbard_tui::{agents::Observation, app::Mode};

struct Lease {
    child: Child,
    _exe_dir: tempfile::TempDir,
}
impl Lease {
    fn new(h: &Harness) -> Self {
        let dir = tempfile::tempdir_in(&h.root).unwrap();
        let exe = dir.path().join("claude");
        compile_fixture(&exe);
        Self {
            _exe_dir: dir,
            child: Command::new(exe).current_dir(&h.root).spawn().unwrap(),
        }
    }
    fn row(&self, h: &Harness) -> AgentSession {
        let identity = (0..100)
            .find_map(|_| {
                let result = probe_agent_identity(self.child.id()).ok();
                if result.is_none() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                result
            })
            .unwrap();
        AgentSession {
            pid: self.child.id(),
            kind: identity.kind,
            cwd: Some(identity.cwd.clone()),
            process_identity: Some(identity),
            repo_name: Some("fixture".into()),
            worktree_path: Some(h.root.clone()),
            worktree_branch: Some("main".into()),
            started_unix: None,
            pgid: None,
            session_id: None,
            name: Some("日本語 👩‍💻 agent".into()),
            title: Some("日本語 👩‍💻 agent".into()),
            activity: AgentActivity::Idle,
        }
    }
}
fn compile_fixture(exe: &std::path::Path) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../switchbard-core/tests/fixtures/agent-process.c");
    assert!(Command::new("cc")
        .arg(source)
        .arg("-o")
        .arg(exe)
        .status()
        .unwrap()
        .success());
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        self.child.wait().unwrap();
    }
}
fn inject(h: &mut Harness, rows: Vec<AgentSession>) {
    h.app.agents.live = false;
    h.app.agents.accept(
        Observation {
            sessions: rows,
            listing_error: None,
            statuses: Default::default(),
            observed_unix: 1_800_000_000,
        },
        Instant::now(),
    );
}
fn settle(h: &mut Harness, predicate: impl Fn(&Harness) -> bool) -> String {
    for _ in 0..200 {
        h.app.tick();
        let screen = h.render();
        if predicate(h) {
            return screen;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("async state did not settle: {}", h.render());
}
fn agents(h: &mut Harness) {
    h.press(KeyCode::Tab);
    h.press(KeyCode::Tab);
}
fn confirm(h: &mut Harness) -> String {
    h.press(KeyCode::Char('x'));
    settle(h, |h| h.app.mode == Mode::PickValue)
}

#[test]
fn agents_offer_a_discoverable_kill_action_without_rebinding_navigation() {
    let mut h = Harness::new();
    h.press(KeyCode::Tab);
    h.press(KeyCode::Tab);
    let screen = h.press(KeyCode::Char('x'));
    assert!(screen.contains("No agent selected"), "{screen}");
    let screen = h.press(KeyCode::Char('?'));
    assert!(screen.contains("kill_agent"), "{screen}");
}

#[test]
fn cancellation_and_letter_digit_shortcuts_never_signal_an_agent() {
    let mut h = Harness::new();
    let mut lease = Lease::new(&h);
    let rows = vec![lease.row(&h)];
    inject(&mut h, rows);
    agents(&mut h);
    let screen = confirm(&mut h);
    assert!(
        screen.contains(&format!("PID {}", lease.child.id())),
        "{screen}"
    );
    assert!(screen.contains("Unsaved work may be lost"), "{screen}");
    for key in ['c', '2', 'x'] {
        h.press(KeyCode::Char(key));
        assert!(lease.child.try_wait().unwrap().is_none());
    }
    assert!(h.press(KeyCode::Enter).contains("Agent signal canceled"));
    assert!(lease.child.try_wait().unwrap().is_none());
    confirm(&mut h);
    h.press(KeyCode::Down);
    h.press(KeyCode::Esc);
    assert!(lease.child.try_wait().unwrap().is_none());
}

#[test]
fn enter_sends_to_selected_pid_off_thread_and_neighbor_and_records_survive() {
    let mut h = Harness::new();
    let mut target = Lease::new(&h);
    let mut neighbor = Lease::new(&h);
    let task_id = h.app.selected_task().unwrap().id.clone();
    switchbard_core::claim_work(
        &h.root.join("work"),
        &switchbard_core::WorkIdentity {
            session_id: "kill-owned-fixture".into(),
            pid: target.child.id(),
            agent: "claude".into(),
        },
        &h.root,
        &task_id,
    )
    .unwrap();
    let rows = vec![target.row(&h), neighbor.row(&h)];
    inject(&mut h, rows);
    agents(&mut h);
    let task_records: Vec<_> = std::fs::read_dir(h.root.join("backlog/tasks"))
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    h.press(KeyCode::Enter);
    let screen = confirm(&mut h);
    assert!(screen.contains("Children may remain"));
    h.press(KeyCode::Down);
    let start = Instant::now();
    h.press(KeyCode::Enter);
    assert!(start.elapsed() < Duration::from_millis(200));
    assert!(h.app.agent_kill.is_submitting());
    h.press(KeyCode::Char('x'));
    assert!(h.app.status.contains("pending"));
    h.press(KeyCode::Char('q'));
    assert!(!h.app.should_quit);
    h.press(KeyCode::Tab);
    let screen = settle(&mut h, |h| !h.app.agent_kill.is_submitting());
    assert!(screen.contains("SIGTERM sent"), "{screen}");
    let exit = (0..100)
        .find_map(|_| {
            let exit = target.child.try_wait().unwrap();
            if exit.is_none() {
                std::thread::sleep(Duration::from_millis(5));
            }
            exit
        })
        .expect("selected child exits within bounded wait");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(exit.signal(), Some(15));
    h.app.tick();
    let history = switchbard_core::read_work_history(&h.root.join("work")).unwrap();
    assert!(history
        .iter()
        .any(|event| event.session_id == "kill-owned-fixture"
            && event.event == switchbard_core::WorkEventKind::Pruned));
    assert!(!history
        .iter()
        .any(|event| event.session_id == "kill-owned-fixture"
            && matches!(
                event.event,
                switchbard_core::WorkEventKind::Released | switchbard_core::WorkEventKind::Passed
            )));
    assert!(neighbor.child.try_wait().unwrap().is_none());
    for (path, bytes) in task_records {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn narrow_confirmation_cannot_fire_until_full_identity_fits() {
    for (width, height) in [(40, 8), (0, 0), (4, 4), (100, 24), (160, 40)] {
        let mut h = Harness::new();
        let mut lease = Lease::new(&h);
        let rows = vec![lease.row(&h)];
        inject(&mut h, rows);
        agents(&mut h);
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        confirm(&mut h);
        h.press(KeyCode::Down);
        if width <= 40 {
            h.press(KeyCode::Enter);
            assert!(!h.app.agent_kill.is_submitting());
            assert!(lease.child.try_wait().unwrap().is_none());
            h.press(KeyCode::Esc);
        } else {
            assert!(h.app.agent_kill.confirmation_visible);
            h.press(KeyCode::Esc);
        }
    }
}

#[test]
fn stale_process_and_selection_change_are_visible_refusals() {
    let mut h = Harness::new();
    let mut target = Lease::new(&h);
    let mut neighbor = Lease::new(&h);
    let rows = vec![target.row(&h), neighbor.row(&h)];
    inject(&mut h, rows);
    agents(&mut h);
    confirm(&mut h);
    target.child.kill().unwrap();
    target.child.wait().unwrap();
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    let screen = settle(&mut h, |h| !h.app.agent_kill.is_submitting());
    assert!(screen.contains("already gone"), "{screen}");
    assert!(neighbor.child.try_wait().unwrap().is_none());
    let rows = vec![neighbor.row(&h)];
    inject(&mut h, rows);
    confirm(&mut h);
    h.app.agents.rows.clear();
    h.press(KeyCode::Down);
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("selection changed"), "{screen}");
    assert!(neighbor.child.try_wait().unwrap().is_none());
}

#[test]
fn configured_action_and_help_agree_and_a_remapped_default_still_navigates() {
    let mut h = Harness::new();
    let a = Lease::new(&h);
    let b = Lease::new(&h);
    std::fs::write(
        &h.config_path,
        "return { keys = { X = 'kill_agent', x = 'up' } }",
    )
    .unwrap();
    h.app = harness::open_app(&h.root, &h.config_path);
    let rows = vec![a.row(&h), b.row(&h)];
    inject(&mut h, rows);
    agents(&mut h);
    assert!(h.render().contains("X kill"));
    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.agents.selected, 1);
    h.press(KeyCode::Char('x'));
    assert_eq!(h.app.agents.selected, 0);
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('k'));
    assert_eq!(h.app.agents.selected, 0);
    assert!(h.press(KeyCode::Char('?')).contains("kill_agent"));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('X'));
    settle(&mut h, |h| h.app.mode == Mode::PickValue);
    h.press(KeyCode::Esc);
}

#[test]
fn interrupted_preparation_never_reopens_or_signals_after_returning() {
    for interruption in [KeyCode::Esc, KeyCode::Tab, KeyCode::Char(':')] {
        let mut h = Harness::new();
        let mut lease = Lease::new(&h);
        let rows = vec![lease.row(&h)];
        inject(&mut h, rows);
        agents(&mut h);
        h.press(KeyCode::Char('x'));
        h.press(interruption);
        if interruption == KeyCode::Tab {
            h.press(KeyCode::Tab);
            h.press(KeyCode::Tab);
            h.press(KeyCode::Tab);
        }
        if interruption == KeyCode::Char(':') {
            h.press(KeyCode::Esc);
        }
        for _ in 0..20 {
            h.app.tick();
            h.render();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(h.app.mode, Mode::Browse);
        assert!(h.app.picker.is_none());
        assert!(lease.child.try_wait().unwrap().is_none());
    }
}

#[test]
fn a_signal_ignoring_agent_is_not_reported_as_exited() {
    let mut h = Harness::new();
    let dir = tempfile::tempdir_in(&h.root).unwrap();
    let exe = dir.path().join("claude");
    compile_fixture(&exe);
    let child = Command::new(exe)
        .arg(h.root.join("ready"))
        .current_dir(&h.root)
        .spawn()
        .unwrap();
    let mut lease = Lease {
        child,
        _exe_dir: dir,
    };
    // A freshly compiled fixture's first exec can take well over half a second
    // on a machine busy running the rest of this suite, so wait on a deadline
    // rather than a fixed tick count (the old 100x5ms flaked roughly one run
    // in three).
    let ready_by = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < ready_by {
        if h.root.join("ready").exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        h.root.join("ready").exists(),
        "shell status {:?}",
        lease.child.try_wait().unwrap()
    );
    let rows = vec![lease.row(&h)];
    inject(&mut h, rows);
    agents(&mut h);
    confirm(&mut h);
    h.press(KeyCode::Down);
    h.press(KeyCode::Enter);
    let screen = settle(&mut h, |h| !h.app.agent_kill.is_submitting());
    assert!(screen.contains("exit not yet confirmed"), "{screen}");
    assert!(screen.contains(&lease.child.id().to_string()));
    assert!(lease.child.try_wait().unwrap().is_none());
}

#[test]
fn long_duplicate_unicode_labels_cannot_replace_native_confirmation_identity() {
    let mut h = Harness::new();
    let mut lease = Lease::new(&h);
    let mut row = lease.row(&h);
    row.name = Some(format!("日本語 👩‍💻\n{}", "x".repeat(500)));
    row.title = row.name.clone();
    let rows = (0..512).map(|_| row.clone()).collect();
    inject(&mut h, rows);
    agents(&mut h);
    h.terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
    let screen = confirm(&mut h);
    assert!(
        screen.contains(&format!("PID {}", lease.child.id())),
        "{screen}"
    );
    assert!(screen.contains("OS birth token:"));
    assert!(screen.contains("Session labels may be stale"));
    assert!(
        !screen.contains(&"x".repeat(100)),
        "untrusted label not used as target authority"
    );
    h.press(KeyCode::Esc);
    assert!(lease.child.try_wait().unwrap().is_none());
}

#[test]
fn unavailable_native_identity_is_an_explicit_refusal_not_an_endless_check() {
    let mut h = Harness::new();
    let mut lease = Lease::new(&h);
    let mut row = lease.row(&h);
    row.process_identity = None;
    inject(&mut h, vec![row]);
    agents(&mut h);
    let screen = h.press(KeyCode::Char('x'));
    assert!(screen.contains("identity unavailable"), "{screen}");
    assert!(!screen.contains("Checking agent identity"));
    assert!(!h.app.agent_kill.is_pending());
    assert!(lease.child.try_wait().unwrap().is_none());
    assert!(h.app.picker.is_none());
}
