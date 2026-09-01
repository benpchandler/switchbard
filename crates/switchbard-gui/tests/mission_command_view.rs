//! Mission Command state/stress coverage through the real Switchbard window.

mod common;

use common::{harness, seeded_app};
use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::SnapshotOptions;
use std::path::PathBuf;
use std::time::Instant;
use switchbard_core::config::ThemeChoice;
use switchbard_core::{
    ApprovalStatus, DecisionStatus, FeedbackStatus, MissionApproval, MissionDecision,
    MissionEvidence, MissionFeedback, MissionPortfolio, MissionProjection, MissionProjectionLoad,
    MissionReconciliation, MissionRequirement, MissionStatus, MissionUnit, PortfolioStatus,
    ProjectedMission, ProjectionFreshness, ReconciliationStatus, RequirementStatus, UnitStatus,
};
use switchbard_gui::runtime::Place;

const MISSION_STATE_LABELS: &[&str] = &[
    "Queue -> Work -> Evidence -> Reconcile -> Approval -> Done",
    "Developer QA fixture",
    "Active",
    "Decision needed",
    "Support needed",
    "External block",
    "Outcome proven",
    "Approval pending",
    "Mission done",
    "DEC-1 v3 is open",
    "APR-1 requested",
    "FDBK-HELD v1",
    "reconciliation passed",
    "Reconcile the mission contract and evidence",
    "Grant or reject explicit completion approval",
    "No further mission action",
];

fn mission(id: &str, status: MissionStatus) -> ProjectedMission {
    let outcome_proven = matches!(
        status,
        MissionStatus::OutcomeProven | MissionStatus::ApprovalPending | MissionStatus::MissionDone
    );
    let evidence_id = format!("EVD-{id}");
    let (next_step, next_owner) = mission_next_action(status);
    ProjectedMission {
        id: id.to_string(),
        mission_revision: None,
        status,
        contract_version: 2,
        attempt_id: format!("ATT-{id}"),
        source_revision: outcome_proven.then(|| "b".repeat(40)),
        outcome_proven,
        next_step: next_step.to_string(),
        next_owner: next_owner.to_string(),
        updated_at: "2026-09-01T12:00:00Z".to_string(),
        requirements: vec![mission_requirement(id, &evidence_id, outcome_proven)],
        units: vec![mission_unit(id, status, outcome_proven)],
        decision: None,
        approval: None,
        feedback: vec![mission_feedback(id)],
        evidence: mission_evidence(id, &evidence_id, outcome_proven),
        reconciliation: mission_reconciliation(id, evidence_id, status),
    }
}

fn mission_next_action(status: MissionStatus) -> (&'static str, &'static str) {
    let (next_step, next_owner) = match status {
        MissionStatus::Queued => ("Assign the next outcome-shaped unit", "orchestrator"),
        MissionStatus::Running => ("Monitor active unit evidence", "agent-alpha"),
        MissionStatus::NeedsDecision => ("Resolve the versioned decision", "user"),
        MissionStatus::OutcomeProven => (
            "Reconcile the mission contract and evidence",
            "orchestrator",
        ),
        MissionStatus::ApprovalPending => ("Grant or reject explicit completion approval", "user"),
        MissionStatus::MissionDone => ("No further mission action", ""),
        _ => ("Inspect mission state", "orchestrator"),
    };
    (next_step, next_owner)
}

fn mission_requirement(id: &str, evidence_id: &str, proven: bool) -> MissionRequirement {
    MissionRequirement {
        id: format!("REQ-{id}"),
        evidence_kind: "browser".to_string(),
        status: if proven {
            RequirementStatus::Proven
        } else {
            RequirementStatus::Open
        },
        evidence_ids: proven
            .then(|| evidence_id.to_string())
            .into_iter()
            .collect(),
    }
}

fn mission_unit(id: &str, status: MissionStatus, proven: bool) -> MissionUnit {
    let status = if proven {
        UnitStatus::Released
    } else if status == MissionStatus::NeedsDecision {
        UnitStatus::Held
    } else {
        UnitStatus::Active
    };
    MissionUnit {
        id: format!("UNIT-{id}"),
        owner: "agent-alpha".to_string(),
        status,
        lease_count: u64::from(!proven),
    }
}

fn mission_feedback(id: &str) -> MissionFeedback {
    MissionFeedback {
        id: format!("FDBK-{id}"),
        version: 1,
        status: FeedbackStatus::Acknowledged,
    }
}

fn mission_evidence(id: &str, evidence_id: &str, proven: bool) -> Vec<MissionEvidence> {
    proven
        .then(|| MissionEvidence {
            id: evidence_id.to_string(),
            requirement_id: format!("REQ-{id}"),
            kind: "browser".to_string(),
            artifact_digest: "a".repeat(64),
            source_revision: "b".repeat(40),
            recorded_at: "2026-09-01T12:00:00Z".to_string(),
        })
        .into_iter()
        .collect()
}

fn mission_reconciliation(
    id: &str,
    evidence_id: String,
    status: MissionStatus,
) -> Option<MissionReconciliation> {
    matches!(
        status,
        MissionStatus::ApprovalPending | MissionStatus::MissionDone
    )
    .then(|| MissionReconciliation {
        id: format!("REC-{id}"),
        status: ReconciliationStatus::Pass,
        mission_revision: 19,
        evidence_ids: vec![evidence_id],
    })
}

fn projection(missions: Vec<ProjectedMission>) -> MissionProjection {
    MissionProjection {
        schema_version: "xplan-mission-projection-v1".to_string(),
        generated_at: "2026-09-01T12:00:00Z".to_string(),
        revision: 19,
        stale_after_seconds: 60,
        portfolio: MissionPortfolio {
            id: "DEVELOPER-QA-FIXTURE".to_string(),
            status: PortfolioStatus::Open,
            missions,
        },
    }
}

fn ready(missions: Vec<ProjectedMission>, freshness: ProjectionFreshness) -> MissionProjectionLoad {
    MissionProjectionLoad::Ready {
        path: PathBuf::from("/tmp/mission-command-snapshot.json"),
        projection: projection(missions),
        freshness,
    }
}

fn app_with(load: MissionProjectionLoad) -> switchbard_gui::app::HiveApp {
    let mut app = seeded_app();
    app.place = Place::Missions;
    *app.mission_projection
        .lock()
        .expect("invariant: test projection lock") = std::sync::Arc::new(load);
    app
}

#[test]
fn mission_place_can_be_selected_from_the_existing_sidebar_navigation() {
    let mut view = harness(seeded_app());
    view.run();
    // IA V2 (TASK-96): the places sidebar is the sole navigation surface —
    // in the default Digest place the only "Missions" widget is the nav
    // row, so clicking it is unambiguous (same interaction the sidebar's
    // own `clicking_command_place_switches_view` test exercises).
    view.get_by_label("Missions").click();
    view.run();
    assert_eq!(view.state().place, Place::Missions);
    assert!(view.query_by_label("Mission Command").is_some());
}

fn mixed_missions() -> Vec<ProjectedMission> {
    let mut held = mission("HELD", MissionStatus::NeedsDecision);
    held.decision = Some(MissionDecision {
        id: "DEC-1".to_string(),
        version: 3,
        status: DecisionStatus::Open,
        scope: None,
    });
    held.feedback[0].status = FeedbackStatus::Queued;
    let mut approval = mission("APPROVAL", MissionStatus::ApprovalPending);
    approval.approval = Some(MissionApproval {
        id: "APR-1".to_string(),
        status: ApprovalStatus::Requested,
    });
    let done = mission("DONE", MissionStatus::MissionDone);
    vec![
        mission("ACTIVE", MissionStatus::Running),
        held,
        mission("SUPPORT", MissionStatus::NeedsSupport),
        mission("BLOCKED", MissionStatus::ExternalBlock),
        mission("PROVEN", MissionStatus::OutcomeProven),
        approval,
        done,
    ]
}

#[test]
fn mission_tab_surfaces_active_held_proven_approval_and_done_states() {
    let app = app_with(ready(
        mixed_missions(),
        ProjectionFreshness::Fresh { age_seconds: 3 },
    ));
    let mut view = harness(app);
    view.run();

    for label in MISSION_STATE_LABELS {
        assert!(
            view.query_all_by_label(label).next().is_some(),
            "missing state label: {label}"
        );
    }
}

#[test]
fn completion_fixtures_assign_the_correct_next_actor() {
    let app = app_with(ready(
        vec![
            mission("PROVEN", MissionStatus::OutcomeProven),
            mission("APPROVAL", MissionStatus::ApprovalPending),
            mission("DONE", MissionStatus::MissionDone),
            mission("ACTIVE", MissionStatus::Running),
        ],
        ProjectionFreshness::Fresh { age_seconds: 3 },
    ));
    let mut view = harness(app);
    view.run();

    let expected = [
        (
            "Reconcile the mission contract and evidence",
            Some("Owner: orchestrator"),
        ),
        (
            "Grant or reject explicit completion approval",
            Some("Owner: user"),
        ),
        ("No further mission action", None),
        ("Monitor active unit evidence", Some("Owner: agent-alpha")),
    ];
    for (next_step, owner) in expected {
        assert!(
            view.query_all_by_label(next_step).next().is_some(),
            "missing next step: {next_step}"
        );
        if let Some(owner) = owner {
            assert!(
                view.query_all_by_label(owner).next().is_some(),
                "missing next actor: {owner}"
            );
        }
    }
    assert!(
        view.query_all_by_label("Owner: ").next().is_none(),
        "a done mission must render no next actor"
    );
}

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/qa/screenshots")
}

#[test]
#[ignore = "wgpu visual evidence: run explicitly with UPDATE_SNAPSHOTS=1"]
fn mission_command_visual_state_matrix() {
    capture_mixed_themes();
    capture_completion_states();
    capture_narrow_stale_state();
}

fn capture_mixed_themes() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let mut app = app_with(ready(
            mixed_missions(),
            ProjectionFreshness::Fresh { age_seconds: 3 },
        ));
        app.config.ui.theme = theme;
        let mut view = harness(app);
        view.run();
        let name = format!("mission_command_mixed_{theme:?}").to_lowercase();
        let options = SnapshotOptions::new().output_path(snapshot_path());
        let _ = view.try_snapshot_options(name, &options);
    }
}

fn capture_completion_states() {
    let completion_states = mixed_missions()
        .into_iter()
        .filter(|mission| {
            matches!(
                mission.status,
                MissionStatus::OutcomeProven
                    | MissionStatus::ApprovalPending
                    | MissionStatus::MissionDone
            )
        })
        .collect();
    let mut app = app_with(ready(
        completion_states,
        ProjectionFreshness::Fresh { age_seconds: 4 },
    ));
    app.config.ui.theme = ThemeChoice::Dark;
    let mut view = harness(app);
    view.set_size(egui::vec2(1280.0, 1020.0));
    view.run();
    view.run();
    let options = SnapshotOptions::new().output_path(snapshot_path());
    let _ = view.try_snapshot_options("mission_command_completion_gates_dark", &options);
}

fn capture_narrow_stale_state() {
    let mut long = mission("NARROW", MissionStatus::Running);
    long.id = "QA-NARROW-MISSION-WITH-A-LONG-STRUCTURAL-IDENTIFIER-0123456789".to_string();
    let mut app = app_with(ready(
        vec![long],
        ProjectionFreshness::Stale {
            age_seconds: 125,
            limit_seconds: 60,
        },
    ));
    app.config.ui.theme = ThemeChoice::Dark;
    let mut view = harness(app);
    view.set_size(egui::vec2(520.0, 620.0));
    view.run();
    view.run();
    let options = SnapshotOptions::new().output_path(snapshot_path());
    let _ = view.try_snapshot_options("mission_command_narrow_stale_dark", &options);
}

#[test]
#[ignore = "50-row render perf smoke: run explicitly with --ignored --nocapture"]
fn mission_command_fifty_row_perf_smoke() {
    let missions = (0..50)
        .map(|index| mission(&format!("PERF-{index:02}"), MissionStatus::Running))
        .collect();
    let app = app_with(ready(
        missions,
        ProjectionFreshness::Fresh { age_seconds: 1 },
    ));
    let mut view = harness(app);
    let mut samples = Vec::with_capacity(200);
    for _ in 0..200 {
        let started = Instant::now();
        view.run();
        samples.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    samples.sort_by(f64::total_cmp);
    let p95 = samples[189];
    println!("Mission Command 50-row frame p95: {p95:.3}ms");
    assert!(
        p95 < 33.0,
        "50-row render p95 exceeded 30fps budget: {p95:.3}ms"
    );
}

#[test]
fn mission_tab_keeps_missing_malformed_unsupported_and_stale_explicit() {
    let states = ordinary_load_states()
        .into_iter()
        .chain(rejected_load_states());
    for (state, label) in states {
        let mut view = harness(app_with(state));
        view.run();
        assert!(
            view.query_by_label(label).is_some(),
            "missing load state: {label}"
        );
    }
}

fn ordinary_load_states() -> [(MissionProjectionLoad, &'static str); 3] {
    [
        (
            MissionProjectionLoad::Loading {
                path: PathBuf::from("/tmp/loading.json"),
            },
            "Loading mission snapshot",
        ),
        (
            MissionProjectionLoad::Missing {
                path: PathBuf::from("/tmp/missing.json"),
            },
            "No mission snapshot yet",
        ),
        (
            MissionProjectionLoad::Unavailable {
                path: PathBuf::from("/tmp/unavailable.json"),
                message: "permission denied".to_string(),
            },
            "Snapshot unavailable",
        ),
    ]
}

fn rejected_load_states() -> [(MissionProjectionLoad, &'static str); 2] {
    [
        (
            MissionProjectionLoad::Malformed {
                path: PathBuf::from("/tmp/bad.json"),
                message: "required field absent".to_string(),
            },
            "Snapshot malformed",
        ),
        (
            MissionProjectionLoad::Unsupported {
                path: PathBuf::from("/tmp/v2.json"),
                found: "xplan-mission-projection-v2".to_string(),
            },
            "Snapshot version unsupported",
        ),
    ]
}

#[test]
fn mission_tab_marks_a_stale_ready_projection() {
    let app = app_with(ready(
        vec![mission("STALE", MissionStatus::Running)],
        ProjectionFreshness::Stale {
            age_seconds: 125,
            limit_seconds: 60,
        },
    ));
    let mut view = harness(app);
    view.run();
    assert!(view
        .query_by_label("Stale snapshot: updated 2m ago (expected within 1m)")
        .is_some());
    assert!(view.query_by_label("Mission STALE").is_some());
}

#[test]
fn mission_tab_handles_empty_narrow_long_and_fifty_rows_in_both_themes() {
    let empty = app_with(ready(
        Vec::new(),
        ProjectionFreshness::Fresh { age_seconds: 0 },
    ));
    let mut empty_view = harness(empty);
    empty_view.run();
    assert!(empty_view.query_by_label("Queue is empty").is_some());

    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let mut missions: Vec<ProjectedMission> = (0..50)
            .map(|index| mission(&format!("ROW-{index:02}"), MissionStatus::Running))
            .collect();
        missions[0].id = "QA-ROW-WITH-A-LONG-STRUCTURAL-IDENTIFIER-0123456789".to_string();
        let mut app = app_with(ready(
            missions,
            ProjectionFreshness::Fresh { age_seconds: 1 },
        ));
        app.config.ui.theme = theme;
        let mut view = harness(app);
        view.set_size(egui::vec2(520.0, 620.0));
        view.run();
        view.run();
        assert!(view.query_by_label("50 active").is_some());
        // IA V2: below `ui::nav::NARROW_WIDTH_THRESHOLD` the places sidebar
        // collapses to its icon rail, so the "Missions" text label is gone
        // by design — the surviving proof at narrow width is the place body
        // itself, via its heading.
        assert!(view.query_by_label("Mission Command").is_some());
    }
}
