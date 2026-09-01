//! Native egui interaction probes for Mission sidecar control states.

mod common;

use common::{harness, seeded_app};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use switchbard_core::mission_supervisor::MissionSupervisor;
use switchbard_core::{MissionStatus, ProjectionFreshness};
use switchbard_gui::mission_control::{
    empty_hello_request, HelperHealth, MissionControlModel, PendingContract, RequestOutcome,
};
use switchbard_gui::runtime::Place;
use tempfile::TempDir;

fn app_with(model: MissionControlModel) -> switchbard_gui::app::HiveApp {
    let mut app = seeded_app();
    app.place = Place::Missions;
    *app.mission_control.lock().expect("mission control lock") = model;
    app
}

fn executable(path: &Path, source: &str) {
    fs::write(path, source).expect("write executable fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("make fixture executable");
}

fn ready_model() -> MissionControlModel {
    MissionControlModel {
        helper_health: HelperHealth::Ready,
        projection_freshness: ProjectionFreshness::Fresh { age_seconds: 1 },
        ..MissionControlModel::default()
    }
}

fn pending_model() -> MissionControlModel {
    let mut model = ready_model();
    model.pending_contract = Some(PendingContract {
        mission_id: "mission-sidecar-v1".to_owned(),
        mission_revision: 1,
        title: "Sidecar contract review".to_owned(),
        outcome: "Contract review is explicitly accepted".to_owned(),
        requirements: vec![
            "Protocol is exact".to_owned(),
            "Restart is durable".to_owned(),
        ],
        decision_id: "contract-review".to_owned(),
        decision_version: 1,
        prompt: "Approve this mission contract?".to_owned(),
    });
    model
}

fn live_model() -> Option<(TempDir, MissionControlModel)> {
    let state = tempfile::tempdir().expect("live sidecar state root");
    let helper = std::env::var("XPLAN_MISSION_HELPER").ok()?;
    let supervisor = MissionSupervisor::from_verified_helper(helper, state.path())
        .expect("real one-shot supervisor");
    let model = MissionControlModel::with_supervisor(supervisor, state.path().to_path_buf());
    Some((state, model))
}

fn ambiguous_helper(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let real = std::env::var("XPLAN_MISSION_HELPER").ok()?;
    let wrapper = root.join("ambiguous-helper");
    let log = root.join("requests.jsonl");
    let marker = root.join("resume-lost-once");
    let script = format!(
        "#!/bin/sh\ninput='{0}/request.$$'\n/bin/cat > \"$input\"\n/bin/cat \"$input\" >> '{1}'\necho >> '{1}'\nif /usr/bin/grep -q '\"command\": \"resume_decision\"' \"$input\" && [ ! -e '{2}' ]; then\n  /usr/bin/touch '{2}'\n  '{3}' \"$@\" < \"$input\" > '{0}/lost-response.json'\n  exit 71\nfi\nexec '{3}' \"$@\" < \"$input\"\n",
        root.display(),
        log.display(),
        marker.display(),
        real
    );
    fs::write(&wrapper, script).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    Some((wrapper, log))
}

fn captured_command_ids(path: &Path, command: &str) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["command"] == command)
        .map(|value| value["command_id"].as_str().unwrap().to_owned())
        .collect()
}

fn assert_authoritative_state(root: &Path, revision: i64) {
    let database = root.join("mission-command.db");
    assert!(database.is_file());
    let snapshot: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("mission-command-snapshot.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        snapshot["portfolio"]["missions"][0]["mission_revision"],
        revision
    );
}

fn run_until_label(
    view: &mut egui_kittest::Harness<'_, switchbard_gui::app::HiveApp>,
    label: &str,
) {
    for _ in 0..100 {
        view.run();
        if view.query_by_label(label).is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let control = view.state().mission_control.lock().unwrap();
    assert!(
        view.query_by_label(label).is_some(),
        "missing label: {label}; outcome: {:?}; open: {}; enabled: {}; draft: {:?}",
        control.request_outcome,
        control.queue_form_open,
        control.queue_submit_enabled(),
        control.draft
    );
}

#[test]
fn mission_control_health_and_projection_freshness_are_independent() {
    let mut model = ready_model();
    model.helper_health = HelperHealth::Unavailable("helper exited".to_owned());
    let mut view = harness(app_with(model));
    view.run();
    assert!(view.query_by_label("Orchestrator unavailable").is_some());
    assert!(view.query_by_label("Projection fresh").is_some());
    assert!(view.query_by_label("Missions").is_some());
    let mut model = ready_model();
    model.projection_freshness = ProjectionFreshness::Stale {
        age_seconds: 90,
        limit_seconds: 60,
    };
    let mut view = harness(app_with(model));
    view.run();
    assert!(view.query_by_label("Orchestrator ready").is_some());
    assert!(view.query_by_label("Projection stale").is_some());
    let mut model = ready_model();
    model.request_outcome = RequestOutcome::AcceptedAwaitingProjection {
        command_id: "fixture:queue".to_owned(),
        accepted_revision: 1,
    };
    model.projection_freshness = ProjectionFreshness::Stale {
        age_seconds: 61,
        limit_seconds: 60,
    };
    let mut view = harness(app_with(model));
    view.run();
    assert!(view
        .query_by_label("Accepted - waiting for projection revision 1")
        .is_some());
    assert!(view.query_by_label("Queue mission").is_none());
    // IA V2: navigation away is never blocked by an in-flight request — the
    // places sidebar (there is no "Backlog" tab anymore) still switches.
    view.get_by_label("Tasks").click();
    view.run();
    assert_eq!(view.state().place, Place::Tasks);
}

#[test]
fn mission_queue_form_preserves_input_and_prevents_duplicate_submit() {
    let Some((state, model)) = live_model() else {
        return;
    };
    let mut view = harness(app_with(model));
    view.run();
    view.get_by_label("Queue mission").click();
    view.run();
    view.get_by_label("Mission ID")
        .type_text("mission-sidecar-v1");
    view.get_by_label("Outcome")
        .type_text("Contract review is explicitly accepted");
    view.get_by_label("Completion requirement 1")
        .type_text("Protocol is exact");
    view.get_by_label("Queue for review").click();
    view.run();
    assert!(view
        .query_by_label("Submitting mission-sidecar-v1")
        .is_some());
    assert!(!view
        .state()
        .mission_control
        .lock()
        .unwrap()
        .queue_submit_enabled());
    assert_eq!(
        view.state()
            .mission_control
            .lock()
            .unwrap()
            .draft
            .mission_id,
        "mission-sidecar-v1"
    );
    run_until_label(&mut view, "Queued for contract review");
    assert_authoritative_state(state.path(), 1);
    assert!(!view
        .state()
        .mission_control
        .lock()
        .unwrap()
        .queue_submit_enabled());
    for case in [
        "invalid",
        "dirty",
        "domain-rejected",
        "response-loss",
        "exact-replay",
    ] {
        let mut model = ready_model();
        model.apply_queue_case(case, "mission-preserved", "Outcome preserved");
        let mut case_view = harness(app_with(model));
        case_view.run();
        assert!(
            case_view.query_by_label("mission-preserved").is_some(),
            "draft lost: {case}"
        );
        assert!(
            case_view.query_by_label("Outcome preserved").is_some(),
            "outcome lost: {case}"
        );
    }
}

#[test]
fn mission_review_recovers_exact_private_contract_after_restart() {
    let Some((state, first)) = live_model() else {
        return;
    };
    first
        .queue_fixture_contract_blocking()
        .expect("queue through real supervisor");
    drop(first);
    let helper = std::env::var("XPLAN_MISSION_HELPER").unwrap();
    let supervisor = MissionSupervisor::from_verified_helper(helper, state.path()).unwrap();
    let mut recovered =
        MissionControlModel::with_supervisor(supervisor, state.path().to_path_buf());
    recovered
        .recover_pending_contract_blocking()
        .expect("private restart read");
    let mut view = harness(app_with(recovered));
    view.run();
    for label in [
        "mission-sidecar-v1",
        "contract-review v1",
        "Contract review is explicitly accepted",
        "Protocol is exact",
        "Restart is durable",
        "Approve this mission contract?",
    ] {
        assert!(
            view.query_by_label(label).is_some(),
            "missing exact private label: {label}"
        );
    }
    assert!(view.query_by_label("Approve and resume").is_some());
    assert_authoritative_state(state.path(), 1);
}

#[test]
fn mission_resume_preserves_answer_and_reconciles_unknown_outcome() {
    let mut model = pending_model();
    model.resume_answer = "Approve with monitored rollout".to_owned();
    model.request_outcome = RequestOutcome::UnknownReconciling {
        command_id: "fixture:resume".to_owned(),
    };
    let mut view = harness(app_with(model));
    view.run();
    assert!(view
        .query_by_label("Outcome unknown - reconciling")
        .is_some());
    assert!(view
        .query_by_label("Approve with monitored rollout")
        .is_some());
    assert!(view.query_by_label("Retry fixture:resume").is_some());
    assert!(view.query_by_label("Mission failed").is_none());
    for case in [
        "timeout",
        "crash",
        "malformed-response",
        "stale-revision",
        "stale-version",
    ] {
        let mut model = pending_model();
        model.resume_answer = "Keep this exact answer".to_owned();
        model.apply_resume_failure_case(case, "fixture:resume");
        let mut case_view = harness(app_with(model));
        case_view.run();
        assert!(case_view.query_by_label("Keep this exact answer").is_some());
        assert!(case_view.query_by_label("Mission failed").is_none());
        assert!(case_view.query_by_label("Refresh projection").is_some());
    }
    let state = tempfile::tempdir().unwrap();
    let Some((helper, log)) = ambiguous_helper(state.path()) else {
        return;
    };
    let supervisor = MissionSupervisor::from_test_fixture(&helper, state.path()).unwrap();
    let mut live = MissionControlModel::with_supervisor(supervisor, state.path().to_path_buf());
    live.queue_fixture_contract_blocking().unwrap();
    live.recover_pending_contract_blocking().unwrap();
    let mut live_view = harness(app_with(live));
    live_view.run();
    live_view.get_by_label("Response").focus();
    live_view
        .get_by_label("Response")
        .type_text("Approve with monitored rollout");
    live_view.get_by_label("Approve and resume").click();
    run_until_label(&mut live_view, "Outcome unknown - reconciling");
    live_view.get_by_label("Retry fixture:resume").click();
    run_until_label(&mut live_view, "Decision acknowledged");
    let ids = captured_command_ids(&log, "resume_decision");
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], ids[1]);
    assert!(
        live_view
            .state()
            .mission_control
            .lock()
            .unwrap()
            .refresh_count()
            >= 1
    );
    assert_authoritative_state(state.path(), 2);
}

#[test]
fn mission_controls_cover_state_scale_layout_and_keyboard_matrix() {
    for count in [0, 1, 50, 500] {
        let mut model = pending_model();
        model.populate_scale_fixture(count, MissionStatus::NeedsDecision);
        model.add_multiple_holds_fixture();
        model.add_long_safe_identifier_fixture(240);
        let mut view = harness(app_with(model));
        view.set_size(egui::vec2(520.0, 620.0));
        view.ctx.set_pixels_per_point(1.75);
        view.run();
        assert!(view.query_by_label(&format!("{count} missions")).is_some());
        // IA V2: below `ui::nav::NARROW_WIDTH_THRESHOLD` the places sidebar
        // collapses to its icon rail (no text labels), so the surviving
        // proof at this width is the place body's own heading.
        assert!(view.query_by_label("Mission Command").is_some());
        assert!(view
            .query_by_label("Disabled: stale decision version")
            .is_some());
    }
    let mut view = harness(app_with(pending_model()));
    view.set_size(egui::vec2(520.0, 420.0));
    view.run();
    view.get_by_label("Response").focus();
    view.key_press(egui::Key::Tab);
    view.key_press(egui::Key::Enter);
    view.run();
    assert!(view.query_by_label("Submitting decision").is_some());
    // IA V2: narrow width collapses the sidebar to the icon rail — the
    // place body's heading is the chrome that must survive here.
    assert!(view.query_by_label("Mission Command").is_some());
    let Some((state, model)) = live_model() else {
        return;
    };
    let mut queue_view = harness(app_with(model));
    queue_view.run();
    queue_view.get_by_label("Queue mission").focus();
    queue_view.key_press(egui::Key::Enter);
    queue_view.run();
    for (label, value) in [
        ("Mission ID", "mission-sidecar-v1"),
        ("Outcome", "Contract review is explicitly accepted"),
        ("Completion requirement 1", "Protocol is exact"),
    ] {
        queue_view.get_by_label(label).focus();
        queue_view.get_by_label(label).type_text(value);
    }
    queue_view.run();
    queue_view.get_by_label("Queue for review").focus();
    queue_view.key_press(egui::Key::Enter);
    run_until_label(&mut queue_view, "Queued for contract review");
    assert_authoritative_state(state.path(), 1);
}

#[test]
fn mission_controls_50_row_p95_is_within_33ms() {
    let mut model = ready_model();
    model.populate_scale_fixture(50, MissionStatus::Running);
    let mut view = harness(app_with(model));
    let mut samples = Vec::with_capacity(200);
    for _ in 0..200 {
        let started = Instant::now();
        view.run();
        samples.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    samples.sort_by(f64::total_cmp);
    let p95 = samples[189];
    eprintln!("MISSION_SIDECAR_P95_MS={p95:.3}");
    assert!(p95 <= 33.0, "50-row p95 {p95:.3}ms exceeds 33ms");
}

#[test]
fn packaged_launcher_proxies_exact_streams_and_exit_status() {
    let root = tempfile::tempdir().expect("temporary app bundle");
    let contents = root.path().join("Switchbard.app/Contents");
    let launcher = contents.join("Helpers/xplan-mission-sidecar-launcher");
    let payload = contents.join("Resources/xplan-mission-sidecar/xplan-mission-sidecar");
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    fs::create_dir_all(payload.parent().unwrap()).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_switchbard"), &launcher).expect("copy packaged launcher");
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    executable(
        &payload,
        r#"#!/bin/sh
record="${0%/*}/proxy-record"
printf '%s\n' "$@" > "$record.argv"
/bin/cat > "$record.stdin"
printf 'payload-stdout\n'
printf 'payload-stderr\n' >&2
exit 23
"#,
    );
    let state_root = root.path().join("private-state");
    let mut child = Command::new(&launcher)
        .arg("--state-root")
        .arg(&state_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn packaged launcher");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"private-request\n")
        .unwrap();
    let output = child
        .wait_with_output()
        .expect("wait for packaged launcher");
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(output.stdout, b"payload-stdout\n");
    assert_eq!(output.stderr, b"payload-stderr\n");
    let record = payload.parent().unwrap().join("proxy-record");
    assert_eq!(
        fs::read_to_string(record.with_extension("argv")).unwrap(),
        format!("--state-root\n{}\n", state_root.display())
    );
    assert_eq!(
        fs::read_to_string(record.with_extension("stdin")).unwrap(),
        "private-request\n"
    );
    let rejected = Command::new(&launcher)
        .args(["--state-root", state_root.to_str().unwrap(), "extra"])
        .output()
        .expect("run invalid launcher invocation");
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8(rejected.stderr)
        .unwrap()
        .contains("expected exactly --state-root <path>"));
}

#[cfg(target_os = "macos")]
#[test]
fn bundled_supervisor_binds_launcher_and_resources_manifest() {
    let root = tempfile::tempdir().expect("temporary app bundle");
    let contents = root.path().join("Switchbard.app/Contents");
    let executable_path = contents.join("MacOS/Switchbard");
    let launcher = contents.join("Helpers/xplan-mission-sidecar-launcher");
    let payload = contents.join("Resources/xplan-mission-sidecar/xplan-mission-sidecar");
    fs::create_dir_all(executable_path.parent().unwrap()).unwrap();
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    fs::create_dir_all(payload.parent().unwrap()).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_switchbard"), &launcher).expect("copy packaged launcher");
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    executable(
        &payload,
        "#!/bin/sh\nread request\nprintf '%s\\n' '{\"protocol_version\":\"xplan-mission-sidecar-v1\",\"request_id\":\"request-switchbard:hello\",\"command_id\":\"switchbard:hello\",\"result\":{\"ok\":true}}'\n",
    );
    let manifest = MissionSupervisor::build_test_manifest(&payload).unwrap();
    fs::write(payload.parent().unwrap().join("manifest.json"), manifest).unwrap();
    let state_root = root.path().join("state");
    let model = MissionControlModel::from_bundled_executable(&executable_path, state_root);
    assert!(matches!(model.helper_health, HelperHealth::Checking));
    let response = model
        .supervisor
        .as_ref()
        .expect("bundled supervisor")
        .invoke(empty_hello_request())
        .expect("hello through packaged layout");
    assert_eq!(response.result()["ok"], true);
}
