//! Process-boundary tests for the bundled one-shot Mission sidecar.

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use switchbard_core::mission_sidecar_protocol::{MissionCommand, MissionRequest};
use switchbard_core::mission_supervisor::{MissionSupervisor, MissionSupervisorConfig};
use tempfile::TempDir;

fn executable(path: &Path, source: &str) {
    fs::write(path, source).expect("write fake helper");
    let mut permissions = fs::metadata(path).expect("helper metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).expect("make helper executable");
}

fn request(command: MissionCommand, command_id: &str, payload: Value) -> MissionRequest {
    MissionRequest {
        protocol_version: "xplan-mission-sidecar-v1".to_owned(),
        request_id: format!("request-{command_id}"),
        command_id: command_id.to_owned(),
        command,
        payload,
    }
}

fn fixture(helper: &str) -> (TempDir, MissionSupervisor) {
    let root = tempfile::tempdir().expect("temporary supervisor fixture");
    let contents = root.path().join("Contents");
    let helper_path = contents.join("Helpers/xplan-mission-sidecar-launcher");
    let payload_path = contents.join("Resources/xplan-mission-sidecar/xplan-mission-sidecar");
    fs::create_dir_all(helper_path.parent().expect("helper parent")).expect("helper directory");
    fs::create_dir_all(payload_path.parent().expect("payload parent")).expect("payload directory");
    executable(&helper_path, helper);
    executable(&payload_path, "#!/bin/sh\nexit 0\n");
    let manifest_path = root
        .path()
        .join("Contents/Resources/xplan-mission-sidecar/manifest.json");
    let manifest = MissionSupervisor::build_test_manifest(&payload_path).expect("test manifest");
    fs::write(&manifest_path, manifest).expect("write test manifest");
    let config = MissionSupervisorConfig {
        executable_root: contents,
        helper_path: PathBuf::from("Helpers/xplan-mission-sidecar-launcher"),
        manifest_path,
        state_root: root.path().join("state"),
        timeout: Duration::from_secs(2),
        stdout_limit: 1_048_576,
        stderr_limit: 65_536,
    };
    let supervisor = MissionSupervisor::new(config).expect("verified supervisor");
    (root, supervisor)
}

#[test]
fn mission_supervisor_uses_verified_path_private_stdin_and_strict_identity() {
    let script = r#"#!/bin/sh
record="${0%/*}/record"
/usr/bin/env > "$record.env"
printf '%s\n' "$@" > "$record.argv"
read request
printf '%s\n' "$request" > "$record.stdin"
printf '%s\n' '{"protocol_version":"xplan-mission-sidecar-v1","request_id":"request-fixture:hello","command_id":"fixture:hello","result":{"ok":true}}'
"#;
    std::env::set_var("SUPER_SECRET", "must-not-cross-boundary");
    let (root, supervisor) = fixture(script);
    let response = supervisor
        .invoke(request(MissionCommand::Hello, "fixture:hello", json!({})))
        .expect("strict helper invocation");
    assert_eq!(response.request_id(), "request-fixture:hello");
    assert_eq!(response.command_id(), "fixture:hello");
    assert_eq!(response.protocol_version(), "xplan-mission-sidecar-v1");
    assert_eq!(response.result()["ok"], true);
    let record = root.path().join("Contents/Helpers/record");
    let environment = fs::read_to_string(record.with_extension("env")).unwrap();
    let arguments = fs::read_to_string(record.with_extension("argv")).unwrap();
    let stdin = fs::read_to_string(record.with_extension("stdin")).unwrap();
    assert!(!environment.contains("SUPER_SECRET"));
    assert!(!arguments.contains("fixture:hello") && !arguments.contains("request-fixture"));
    assert!(arguments.contains(root.path().join("state").to_str().unwrap()));
    assert!(stdin.contains("fixture:hello") && !stdin.contains(root.path().to_str().unwrap()));
    fs::write(
        root.path()
            .join("Contents/Helpers/xplan-mission-sidecar-launcher"),
        "#!/bin/sh\nexit 0\n",
    )
    .unwrap();
    assert!(supervisor
        .invoke(request(MissionCommand::Hello, "fixture:tamper", json!({})))
        .expect_err("tampered helper must reject")
        .is_manifest_rejection());
    let (payload_root, payload_supervisor) = fixture(script);
    fs::write(
        payload_root
            .path()
            .join("Contents/Resources/xplan-mission-sidecar/xplan-mission-sidecar"),
        "#!/bin/sh\nexit 0\n# tampered\n",
    )
    .unwrap();
    assert!(payload_supervisor
        .invoke(request(
            MissionCommand::Hello,
            "fixture:payload-tamper",
            json!({}),
        ))
        .expect_err("tampered payload must reject")
        .is_manifest_rejection());
    for (name, response) in strict_identity_failures() {
        let (_case_root, case_supervisor) = fixture(response);
        let error = case_supervisor
            .invoke(request(MissionCommand::Hello, "fixture:hello", json!({})))
            .expect_err(name);
        assert!(
            error.is_protocol_rejection(),
            "identity case accepted: {name}"
        );
    }
}

fn strict_identity_failures() -> [(&'static str, &'static str); 4] {
    [
        ("wrong protocol", "#!/bin/sh\nread x\necho '{\"protocol_version\":\"wrong\",\"request_id\":\"request-fixture:hello\",\"command_id\":\"fixture:hello\",\"result\":{}}'"),
        ("wrong request", "#!/bin/sh\nread x\necho '{\"protocol_version\":\"xplan-mission-sidecar-v1\",\"request_id\":\"wrong\",\"command_id\":\"fixture:hello\",\"result\":{}}'"),
        ("wrong command", "#!/bin/sh\nread x\necho '{\"protocol_version\":\"xplan-mission-sidecar-v1\",\"request_id\":\"request-fixture:hello\",\"command_id\":\"wrong\",\"result\":{}}'"),
        ("extra field", "#!/bin/sh\nread x\necho '{\"protocol_version\":\"xplan-mission-sidecar-v1\",\"request_id\":\"request-fixture:hello\",\"command_id\":\"fixture:hello\",\"result\":{},\"extra\":true}'"),
    ]
}

#[test]
fn mission_supervisor_bounds_kills_reaps_and_reuses_command_id() {
    let command = request(MissionCommand::QueueMission, "fixture:ambiguous", json!({}));
    for (name, script) in supervisor_failure_cases() {
        let (_root, supervisor) = fixture(script);
        let error = supervisor.invoke(command.clone()).expect_err(name);
        assert!(supervisor.last_process_group_reaped(), "child leak: {name}");
        assert!(error.is_bounded_failure(), "unclassified failure: {name}");
        if error.is_ambiguous() {
            let retry = supervisor.prepare_retry(&command, &error).unwrap();
            assert_eq!(retry.command_id, "fixture:ambiguous");
            assert_ne!(retry.request_id, command.request_id);
        }
    }
}

#[test]
fn mission_supervisor_bounds_reader_joins_when_a_descendant_escapes_the_group() {
    let (_root, supervisor) = fixture(concat!(
        "#!/bin/sh\n",
        "read x\n",
        "marker=\"${0%/*}/escaped\"\n",
        "python3 -c \"import os,time; os.setsid(); open('$marker','w').close(); time.sleep(30)\" &\n",
        "while [ ! -e \"$marker\" ]; do sleep 0.05; done\n",
        "echo '{\"protocol_version\":\"xplan-mission-sidecar-v1\",\"request_id\":\"request-fixture:escapee\",\"command_id\":\"fixture:escapee\",\"result\":{}}'\n",
    ));
    let started = std::time::Instant::now();
    let error = supervisor
        .invoke(request(MissionCommand::Hello, "fixture:escapee", json!({})))
        .expect_err("escaped descendant holding the pipe must not yield a trusted response");
    assert!(error.is_ambiguous(), "misclassified escapee: {error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "reader join was not bounded"
    );
}

fn supervisor_failure_cases() -> [(&'static str, &'static str); 6] {
    [
        (
            "timeout",
            "#!/bin/sh\ntrap 'exit 0' TERM\nsleep 30 & wait $!",
        ),
        (
            "oversized stdout",
            "#!/bin/sh\nread x\n/usr/bin/yes x | /usr/bin/head -c 1100000",
        ),
        (
            "oversized stderr",
            "#!/bin/sh\nread x\n/usr/bin/yes x | /usr/bin/head -c 70000 >&2\nexit 7",
        ),
        (
            "multiple responses",
            "#!/bin/sh\nread x\necho '{}'\necho '{}'",
        ),
        ("malformed response", "#!/bin/sh\nread x\necho 'not-json'"),
        ("abnormal exit", "#!/bin/sh\nread x\nexit 23"),
    ]
}

#[test]
fn mission_fresh_supervisor_recovers_pending_contract() {
    let (Ok(helper), Ok(state)) = (
        std::env::var("XPLAN_MISSION_HELPER"),
        std::env::var("XPLAN_MISSION_STATE"),
    ) else {
        return;
    };
    let supervisor =
        MissionSupervisor::from_verified_helper(helper, state).expect("fresh verified supervisor");
    let payload = json!({
        "mission_id": "mission-sidecar-v1",
        "mission_revision": 1,
        "decision_id": "contract-review",
        "decision_version": 1
    });
    let response = supervisor
        .invoke(request(
            MissionCommand::GetPendingDecision,
            "fixture:pending",
            payload,
        ))
        .expect("recover exact pending contract");
    let result = response.result();
    assert_eq!(result["mission_id"], "mission-sidecar-v1");
    assert_eq!(result["mission_revision"], 1);
    assert_eq!(result["decision_id"], "contract-review");
    assert_eq!(result["decision_version"], 1);
    assert_eq!(result["requirements"].as_array().map(Vec::len), Some(2));
}

#[test]
fn mission_projection_v1_is_read_only() {
    let root = tempfile::tempdir().expect("temporary legacy snapshot root");
    let path = root.path().join("mission-command-snapshot.json");
    let snapshot = json!({
        "schema_version": "xplan-mission-projection-v1",
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "revision": 7,
        "stale_after_seconds": 60,
        "portfolio": {"id": "PORT-1", "status": "OPEN", "missions": [{
            "id": "mission-sidecar-v1", "status": "RUNNING", "contract_version": 1,
            "attempt_id": "ATT-1", "source_revision": "b".repeat(40),
            "outcome_proven": false,
            "next_step": "Monitor active unit evidence", "next_owner": "agent",
            "updated_at": "2026-08-31T12:00:00Z",
            "requirements": [
                {"id": "REQ-1", "evidence_kind": "browser",
                 "status": "PROVEN", "evidence_ids": ["EVD-1"]},
                {"id": "REQ-2", "evidence_kind": "review",
                 "status": "OPEN", "evidence_ids": []}
            ],
            "units": [{"id": "UNIT-1", "owner": "agent", "status": "ACTIVE", "lease_count": 1}],
            "decision": {"id": "DEC-1", "version": 1, "status": "RESOLVED"},
            "approval": {"id": "APR-1", "status": "GRANTED"},
            "feedback": [{"id": "FDBK-1", "version": 1, "status": "ACKNOWLEDGED"}],
            "evidence": [{
                "id": "EVD-1", "requirement_id": "REQ-1", "kind": "browser",
                "artifact_digest": "a".repeat(64), "source_revision": "b".repeat(40),
                "recorded_at": "2026-08-31T12:00:00Z"
            }],
            "reconciliation": {"id": "REC-1", "status": "FAIL", "mission_revision": 6,
                               "evidence_ids": ["EVD-1"]}
        }]}
    });
    fs::write(&path, snapshot.to_string()).expect("write legacy v1 snapshot");
    let loaded = switchbard_core::load_mission_projection(&path);
    assert!(loaded.is_legacy_v1());
    assert!(!loaded.controls_enabled());
    assert_eq!(
        loaded
            .mission("mission-sidecar-v1")
            .unwrap()
            .mission_revision(),
        None
    );
}

#[test]
fn mission_projection_v2_enables_controls() {
    let Ok(value) = std::env::var("XPLAN_MISSION_STATE") else {
        return;
    };
    let state = PathBuf::from(value);
    let loaded =
        switchbard_core::load_mission_projection(&state.join("mission-command-snapshot.json"));
    assert!(loaded.is_v2());
    assert!(loaded.controls_enabled());
    assert_eq!(
        loaded
            .mission("mission-sidecar-v1")
            .unwrap()
            .mission_revision(),
        Some(1)
    );
}
