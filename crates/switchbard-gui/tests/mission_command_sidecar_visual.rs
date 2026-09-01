//! Fresh real-egui evidence renders for the Mission sidecar control slice.

mod common;

use chrono::Utc;
use common::{harness, seeded_app};
use egui_kittest::{try_image_snapshot_options, SnapshotOptions};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use switchbard_core::config::ThemeChoice;
use switchbard_core::ProjectionFreshness;
use switchbard_gui::mission_control::{HelperHealth, MissionControlModel, PendingContract};
use switchbard_gui::runtime::Place;

fn canonical_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/qa/screenshots")
}

fn actual_dir() -> PathBuf {
    std::env::var("MISSION_SIDECAR_VISUAL_ACTUAL_DIR")
        .map(PathBuf::from)
        .expect("verifier must bind fresh visual actuals")
}

fn evidence_path() -> PathBuf {
    std::env::var("MISSION_SIDECAR_VISUAL_EVIDENCE_OUT")
        .map(PathBuf::from)
        .expect("verifier must bind the visual evidence output")
}

fn ready() -> MissionControlModel {
    MissionControlModel {
        helper_health: HelperHealth::Ready,
        projection_freshness: ProjectionFreshness::Fresh { age_seconds: 1 },
        ..MissionControlModel::default()
    }
}

fn queue() -> MissionControlModel {
    let mut model = ready();
    model.queue_form_open = true;
    model.draft.mission_id = "mission-sidecar-v1".to_owned();
    model.draft.outcome = "Contract review is explicitly accepted".to_owned();
    model.draft.requirements = vec!["Protocol is exact".to_owned()];
    model
}

fn decision() -> MissionControlModel {
    let mut model = ready();
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

fn helper_failure() -> MissionControlModel {
    let mut model = queue();
    model.helper_health = HelperHealth::Unavailable("helper exited before response".to_owned());
    model
}

fn render(state: &str, filename: &str, model: MissionControlModel, theme: ThemeChoice, width: f32) {
    if std::env::var_os("MISSION_SIDECAR_VISUAL_ACTUAL_DIR").is_none() {
        return;
    }
    assert!(std::env::var_os("UPDATE_SNAPSHOTS").is_none());
    let mut app = seeded_app();
    app.place = Place::Missions;
    app.config.ui.theme = theme;
    *app.mission_control.lock().expect("mission control lock") = model;
    let mut view = harness(app);
    view.set_size(eframe::egui::vec2(width, 760.0));
    view.run();
    view.run();
    fs::create_dir_all(actual_dir()).expect("actual screenshot directory");
    let image = view.render().expect("render real egui pixels");
    let actual = actual_dir().join(filename);
    image.save(&actual).expect("write fresh actual PNG");
    let name = filename
        .strip_suffix(".png")
        .expect("png evidence filename");
    let options = SnapshotOptions::new().output_path(canonical_dir());
    try_image_snapshot_options(&image, name, &options).expect("immutable canonical comparison");
    record_evidence(state, &actual, &canonical_dir().join(filename));
}

fn png_dimensions(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).expect("read rendered PNG");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (width, height)
}

fn digest(path: &Path) -> String {
    for (program, args) in [("shasum", ["-a", "256"]), ("sha256sum", ["", ""])] {
        let mut command = Command::new(program);
        if program == "shasum" {
            command.args(args);
        }
        if let Ok(output) = command.arg(path).output() {
            if output.status.success() {
                return String::from_utf8(output.stdout)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .to_owned();
            }
        }
    }
    panic!("no SHA-256 tool available for visual evidence")
}

fn record_evidence(state: &str, actual: &Path, canonical: &Path) {
    let (width, height) = png_dimensions(actual);
    assert_eq!((width, height), png_dimensions(canonical));
    let target = evidence_path();
    let mut data = if target.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&target).unwrap()).unwrap()
    } else {
        json!({"schema_version": 1, "images": []})
    };
    let images = data["images"].as_array_mut().expect("images array");
    images.retain(|item| item["state_id"] != state);
    images.push(json!({
        "state_id": state,
        "actual_path": actual.canonicalize().unwrap(),
        "actual_sha256": digest(actual),
        "canonical_path": canonical.canonicalize().unwrap(),
        "canonical_sha256": digest(canonical),
        "width": width,
        "height": height,
    }));
    data["generated_at"] = json!(Utc::now().to_rfc3339());
    data["xplan_commit"] = json!(std::env::var("XPLAN_COMMIT").unwrap());
    data["switchbard_commit"] = json!(std::env::var("SWITCHBARD_COMMIT").unwrap());
    fs::write(target, serde_json::to_vec_pretty(&data).unwrap()).expect("write visual evidence");
}

#[test]
fn mission_sidecar_visual_ready() {
    render(
        "ready_light",
        "mission_sidecar_ready_light.png",
        ready(),
        ThemeChoice::Light,
        1280.0,
    );
}

#[test]
fn mission_sidecar_visual_queue() {
    render(
        "queue_light",
        "mission_sidecar_queue_light.png",
        queue(),
        ThemeChoice::Light,
        1280.0,
    );
}

#[test]
fn mission_sidecar_visual_decision() {
    render(
        "decision_light",
        "mission_sidecar_decision_light.png",
        decision(),
        ThemeChoice::Light,
        1280.0,
    );
}

#[test]
fn mission_sidecar_visual_helper_failure() {
    render(
        "helper_failure_light",
        "mission_sidecar_helper_failure_light.png",
        helper_failure(),
        ThemeChoice::Light,
        1280.0,
    );
}

#[test]
fn mission_sidecar_visual_narrow() {
    render(
        "narrow_light",
        "mission_sidecar_narrow_light.png",
        decision(),
        ThemeChoice::Light,
        520.0,
    );
}

#[test]
fn mission_sidecar_visual_light() {
    for (state, filename, model) in [
        ("ready_light", "mission_sidecar_ready_light.png", ready()),
        ("queue_light", "mission_sidecar_queue_light.png", queue()),
        (
            "decision_light",
            "mission_sidecar_decision_light.png",
            decision(),
        ),
        (
            "helper_failure_light",
            "mission_sidecar_helper_failure_light.png",
            helper_failure(),
        ),
    ] {
        render(state, filename, model, ThemeChoice::Light, 1280.0);
    }
}

#[test]
fn mission_sidecar_visual_dark() {
    for (state, filename, model) in [
        ("ready_dark", "mission_sidecar_ready_dark.png", ready()),
        ("queue_dark", "mission_sidecar_queue_dark.png", queue()),
        (
            "decision_dark",
            "mission_sidecar_decision_dark.png",
            decision(),
        ),
        (
            "helper_failure_dark",
            "mission_sidecar_helper_failure_dark.png",
            helper_failure(),
        ),
    ] {
        render(state, filename, model, ThemeChoice::Dark, 1280.0);
    }
}
