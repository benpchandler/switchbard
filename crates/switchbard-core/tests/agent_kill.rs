use std::process::{Child, Command};
use switchbard_core::{
    prepare_agent_termination, probe_agent_identity, terminate_agent, AgentActivity, AgentSession,
    AgentTerminationOutcome,
};

struct Lease {
    child: Child,
    _dir: tempfile::TempDir,
}
impl Lease {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("claude");
        let source =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent-process.c");
        assert!(Command::new("cc")
            .arg(source)
            .arg("-o")
            .arg(&exe)
            .status()
            .unwrap()
            .success());
        let child = Command::new(exe).current_dir(dir.path()).spawn().unwrap();
        Self { child, _dir: dir }
    }
    fn row(&self) -> AgentSession {
        let identity = (0..100)
            .find_map(|_| {
                let result = probe_agent_identity(self.child.id()).ok();
                if result.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                result
            })
            .expect("fixture becomes observable");
        AgentSession {
            pid: self.child.id(),
            kind: identity.kind,
            cwd: Some(identity.cwd.clone()),
            process_identity: Some(identity),
            repo_name: None,
            worktree_path: None,
            worktree_branch: None,
            started_unix: None,
            pgid: None,
            session_id: None,
            name: None,
            title: None,
            activity: AgentActivity::Unknown,
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        self.child.wait().unwrap();
    }
}

#[test]
fn agent_kill_signals_only_the_authenticated_positive_pid() {
    let mut target = Lease::new();
    let mut neighbor = Lease::new();
    let prepared = prepare_agent_termination(&target.row()).unwrap();
    assert_eq!(
        terminate_agent(prepared).unwrap(),
        AgentTerminationOutcome::SignalSent
    );
    for _ in 0..100 {
        if target.child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(target.child.try_wait().unwrap().is_some());
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(target.child.try_wait().unwrap().unwrap().signal(), Some(15));
    assert!(
        neighbor.child.try_wait().unwrap().is_none(),
        "shared-group neighbor survives"
    );
}

#[test]
fn agent_kill_rejects_stale_forged_self_and_invalid_targets() {
    let mut lease = Lease::new();
    for pid in [0, 1, u32::MAX, std::process::id()] {
        let mut row = lease.row();
        row.pid = pid;
        row.process_identity.as_mut().unwrap().pid = pid;
        assert!(prepare_agent_termination(&row).is_err());
    }
    // SAFETY: getppid has no arguments or pointers and is a read-only OS query.
    let parent = unsafe { libc::getppid() } as u32;
    let mut row = lease.row();
    row.pid = parent;
    row.process_identity.as_mut().unwrap().pid = parent;
    assert!(prepare_agent_termination(&row)
        .unwrap_err()
        .to_string()
        .contains("ancestor"));
    let mut row = lease.row();
    row.process_identity.as_mut().unwrap().start_token.1 += 1;
    assert!(prepare_agent_termination(&row).is_err());
    let mut row = lease.row();
    row.cwd = Some("/wrong-cwd".into());
    assert!(prepare_agent_termination(&row).is_err());
    let mut row = lease.row();
    row.process_identity.as_mut().unwrap().executable = "/wrong-executable".into();
    assert!(prepare_agent_termination(&row).is_err());
    assert!(lease.child.try_wait().unwrap().is_none());
    let prepared = prepare_agent_termination(&lease.row()).unwrap();
    lease.child.kill().unwrap();
    lease.child.wait().unwrap();
    assert_eq!(
        terminate_agent(prepared).unwrap(),
        AgentTerminationOutcome::AlreadyGone
    );
}

#[test]
fn agent_kill_capture_and_registry_merge_keep_native_birth_authoritative() {
    let lease = Lease::new();
    let native = lease.row().process_identity.unwrap();
    let discovery = switchbard_core::AgentProcessRow {
        pid: lease.child.id(),
        kind: native.kind,
        cwd: Some(native.cwd.clone()),
        started_unix: None,
        process_identity: None,
        pgid: Some(12345),
        session_id: None,
        name: None,
        activity: AgentActivity::Unknown,
    };
    let captured = switchbard_core::authenticate_agent_rows(vec![discovery.clone()]);
    assert_eq!(captured[0].cwd, Some(native.cwd.clone()));
    assert_eq!(captured[0].started_unix, native.started_unix);
    assert_eq!(captured[0].process_identity, Some(native.clone()));
    assert_eq!(
        captured[0].pgid, discovery.pgid,
        "fleet dispatch dedup metadata remains independent of positive-PID kill"
    );
    let listed = switchbard_core::AgentProcessRow {
        cwd: None,
        name: Some("observational label".into()),
        activity: AgentActivity::Idle,
        ..discovery.clone()
    };
    let merged = switchbard_core::merge_agent_rows(captured.clone(), vec![listed.clone()]);
    assert_eq!(merged[0].process_identity, Some(native.clone()));
    assert_eq!(merged[0].started_unix, native.started_unix);
    assert_eq!(merged[0].cwd, Some(native.cwd.clone()));
    assert_eq!(merged[0].name, listed.name);
    let contradicted = switchbard_core::merge_agent_rows(
        captured,
        vec![switchbard_core::AgentProcessRow {
            started_unix: Some(1),
            ..listed
        }],
    );
    assert!(contradicted[0].process_identity.is_none());
    assert_eq!(contradicted[0].started_unix, native.started_unix);
    let wrong_cwd =
        switchbard_core::authenticate_agent_rows(vec![switchbard_core::AgentProcessRow {
            cwd: Some("/stale-discovery-cwd".into()),
            ..discovery.clone()
        }]);
    assert!(wrong_cwd[0].process_identity.is_none());
    assert_eq!(wrong_cwd[0].cwd, Some(native.cwd.clone()));
    let wrong_kind =
        switchbard_core::authenticate_agent_rows(vec![switchbard_core::AgentProcessRow {
            kind: switchbard_core::AgentProcessKind::Codex,
            ..discovery.clone()
        }]);
    assert!(wrong_kind[0].process_identity.is_none());
    assert_eq!(wrong_kind[0].kind, native.kind);
    let stale = switchbard_core::authenticate_agent_rows(vec![switchbard_core::AgentProcessRow {
        started_unix: Some(1),
        ..discovery
    }]);
    assert!(stale[0].process_identity.is_none());
    assert_eq!(stale[0].cwd, Some(native.cwd));
}
