//! Native Mission Command supervision over xplan's versioned projection.
//!
//! Every disk read happens on `workers::spawn_mission_projection`; rendering
//! holds the already-validated cache and performs only bounded arithmetic over
//! at most `MAX_PROJECTED_MISSIONS` rows. Controls emit typed intentions to a
//! background supervisor; xplan remains the only mission-state writer.

use crate::app::HiveApp;
use crate::mission_control::{HelperHealth, MissionControlModel, RequestOutcome};
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use switchbard_core::{
    ApprovalStatus, DecisionStatus, FeedbackStatus, MissionProjection, MissionProjectionLoad,
    MissionStatus, ProjectedMission, ProjectionFreshness, ReconciliationStatus, RequirementStatus,
    UnitStatus,
};

pub fn render(app: &mut HiveApp, ui: &mut egui::Ui) {
    let query = app.filter().trim().to_lowercase();
    let state = std::sync::Arc::clone(
        &app.mission_projection
            .lock()
            .expect("invariant: mission projection cache lock"),
    );
    let mut action = None;
    egui::CentralPanel::default().show(ui, |ui| {
        render_heading(ui);
        action = render_controls(app, ui);
        ui.add_space(10.0);
        match &*state {
            MissionProjectionLoad::Loading { path } => render_loading(ui, path),
            MissionProjectionLoad::Missing { path } => render_missing(ui, path),
            MissionProjectionLoad::Unavailable { path, message } => {
                render_error(ui, "Snapshot unavailable", path, message)
            }
            MissionProjectionLoad::Malformed { path, message } => {
                render_error(ui, "Snapshot malformed", path, message)
            }
            MissionProjectionLoad::Unsupported { path, found } => {
                render_unsupported(ui, path, found)
            }
            MissionProjectionLoad::Ready {
                path,
                projection,
                freshness,
            } => render_projection(ui, path, projection, freshness, &query),
        }
    });
    match action {
        Some(ControlAction::Queue) => app.spawn_queue_mission(ui.ctx().clone()),
        Some(ControlAction::Resume) => app.spawn_resume_decision(ui.ctx().clone()),
        Some(ControlAction::Retry) => app.spawn_retry_mission(ui.ctx().clone()),
        Some(ControlAction::Refresh) => app.mission_projection_kick.notify(),
        None => {}
    }
}

#[derive(Debug, Clone, Copy)]
enum ControlAction {
    Queue,
    Resume,
    Retry,
    Refresh,
}

fn render_controls(app: &HiveApp, ui: &mut egui::Ui) -> Option<ControlAction> {
    let mut model = app.mission_control.lock().unwrap();
    render_health(ui, &model);
    ui.add_space(6.0);
    if model.mission_count() > 0 || model.multiple_holds() {
        render_stress_summary(ui, &model);
    }
    let show_preserved_draft = !matches!(model.request_outcome, RequestOutcome::Idle)
        || !matches!(model.helper_health, HelperHealth::Ready)
        || model.draft.mission_id != "mission-sidecar-v1";
    if model.queue_form_open && show_preserved_draft {
        if !model.draft.mission_id.is_empty() {
            ui.label(&model.draft.mission_id);
        }
        if !model.draft.outcome.is_empty() {
            ui.label(&model.draft.outcome);
        }
    }
    let outcome_action = render_outcome(ui, &model);
    let suppress_primary = matches!(
        model.request_outcome,
        RequestOutcome::SubmittingQueue { .. }
            | RequestOutcome::SubmittingDecision
            | RequestOutcome::AcceptedAwaitingProjection { .. }
            | RequestOutcome::DecisionAcknowledged { .. }
            | RequestOutcome::UnknownReconciling { .. }
    ) || (matches!(
        model.request_outcome,
        RequestOutcome::QueuedForReview { .. }
    ) && model.pending_contract.is_none());
    let action = outcome_action.or_else(|| {
        (!suppress_primary)
            .then(|| render_primary_control(ui, &mut model))
            .flatten()
    });
    if let Some(identifier) = model.long_identifier() {
        ui.label(
            egui::RichText::new(identifier)
                .monospace()
                .color(theme::weak_text()),
        );
    }
    action
}

fn render_health(ui: &mut egui::Ui, model: &MissionControlModel) {
    ui.horizontal_wrapped(|ui| {
        let (kind, helper) = match &model.helper_health {
            HelperHealth::Checking => (StatusKind::Neutral, "Orchestrator checking"),
            HelperHealth::Ready => (StatusKind::Good, "Orchestrator ready"),
            HelperHealth::Unavailable(_) => (StatusKind::Danger, "Orchestrator unavailable"),
        };
        status_pill(ui, kind, helper, None);
        let (kind, projection) = match model.projection_freshness {
            ProjectionFreshness::Fresh { .. } => (StatusKind::Good, "Projection fresh"),
            ProjectionFreshness::Stale { .. } => (StatusKind::Warn, "Projection stale"),
        };
        status_pill(ui, kind, projection, None);
    });
    if let HelperHealth::Unavailable(message) = &model.helper_health {
        ui.colored_label(theme::warn_orange(), message);
    }
}

fn render_stress_summary(ui: &mut egui::Ui, model: &MissionControlModel) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} missions", model.mission_count()));
        if model.multiple_holds() {
            ui.colored_label(
                theme::amber(),
                "Multiple held decisions require exact identities",
            );
        }
        if model.stale_decision_fixture() {
            ui.label("Disabled: stale decision version");
        }
    });
}

fn render_outcome(ui: &mut egui::Ui, model: &MissionControlModel) -> Option<ControlAction> {
    match &model.request_outcome {
        RequestOutcome::Idle => None,
        RequestOutcome::Invalid(message) | RequestOutcome::DomainRejected(message) => {
            ui.colored_label(theme::warn_orange(), message);
            None
        }
        RequestOutcome::SubmittingQueue { mission_id } => {
            ui.label(format!("Submitting {mission_id}"));
            None
        }
        RequestOutcome::SubmittingDecision => {
            ui.label("Submitting decision");
            None
        }
        RequestOutcome::AcceptedAwaitingProjection {
            accepted_revision, ..
        } => {
            ui.label(format!(
                "Accepted - waiting for projection revision {accepted_revision}"
            ));
            None
        }
        RequestOutcome::QueuedForReview { .. } => {
            ui.label("Queued for contract review");
            None
        }
        RequestOutcome::DecisionAcknowledged { .. } => {
            ui.label("Decision acknowledged");
            None
        }
        RequestOutcome::UnknownReconciling { command_id } => {
            ui.label("Outcome unknown - reconciling");
            ui.label(&model.resume_answer);
            if ui.button(format!("Retry {command_id}")).clicked() {
                return Some(ControlAction::Retry);
            }
            if ui.button("Refresh projection").clicked() {
                return Some(ControlAction::Refresh);
            }
            None
        }
        RequestOutcome::StaleDecision(message) => {
            ui.colored_label(theme::amber(), message);
            if !model.resume_answer.is_empty() {
                ui.label(&model.resume_answer);
            }
            ui.button("Refresh projection")
                .clicked()
                .then_some(ControlAction::Refresh)
        }
    }
}

fn render_primary_control(
    ui: &mut egui::Ui,
    model: &mut MissionControlModel,
) -> Option<ControlAction> {
    if model.pending_contract.is_some() {
        render_contract_review(ui, model)
    } else if model.queue_form_open {
        render_queue_form(ui, model)
    } else if ui.button("Queue mission").clicked() {
        model.queue_form_open = true;
        None
    } else {
        None
    }
}

fn render_queue_form(ui: &mut egui::Ui, model: &mut MissionControlModel) -> Option<ControlAction> {
    egui::Frame::group(ui.style())
        .fill(theme::card_bg())
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Queue a mission for contract review").strong());
            accessible_text_edit(
                ui.add(
                    egui::TextEdit::singleline(&mut model.draft.mission_id)
                        .hint_text("Mission ID")
                        .desired_width(f32::INFINITY),
                ),
                "Mission ID",
            );
            accessible_text_edit(
                ui.add(
                    egui::TextEdit::multiline(&mut model.draft.outcome)
                        .hint_text("Outcome")
                        .desired_rows(2),
                ),
                "Outcome",
            );
            for (index, requirement) in model.draft.requirements.iter_mut().enumerate() {
                let label = format!("Completion requirement {}", index + 1);
                accessible_text_edit(
                    ui.add(
                        egui::TextEdit::singleline(requirement)
                            .hint_text(&label)
                            .desired_width(f32::INFINITY),
                    ),
                    &label,
                );
            }
            ui.checkbox(&mut model.draft.approval_required, "Approval contract");
            let mut action = None;
            ui.horizontal(|ui| {
                let queue_available = matches!(model.helper_health, HelperHealth::Ready)
                    && !matches!(
                        model.request_outcome,
                        RequestOutcome::SubmittingQueue { .. }
                            | RequestOutcome::AcceptedAwaitingProjection { .. }
                            | RequestOutcome::QueuedForReview { .. }
                    );
                if ui
                    .add_enabled(queue_available, egui::Button::new("Queue for review"))
                    .clicked()
                {
                    action = Some(ControlAction::Queue);
                }
                if ui.button("Cancel").clicked() {
                    model.queue_form_open = false;
                }
            });
            action
        })
        .inner
}

fn render_contract_review(
    ui: &mut egui::Ui,
    model: &mut MissionControlModel,
) -> Option<ControlAction> {
    let resume_available = matches!(model.helper_health, HelperHealth::Ready)
        && !model.stale_decision_fixture()
        && !matches!(
            model.request_outcome,
            RequestOutcome::SubmittingDecision | RequestOutcome::DecisionAcknowledged { .. }
        );
    let model = &mut *model;
    let pending = model
        .pending_contract
        .as_ref()
        .expect("checked pending contract");
    let resume_answer = &mut model.resume_answer;
    egui::Frame::group(ui.style())
        .fill(theme::card_bg())
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&pending.mission_id).strong());
                status_pill(
                    ui,
                    StatusKind::Warn,
                    format!("{} v{}", pending.decision_id, pending.decision_version),
                    None,
                );
            });
            ui.label(&pending.title);
            ui.label(&pending.outcome);
            for requirement in &pending.requirements {
                ui.label(requirement);
            }
            ui.colored_label(theme::amber(), &pending.prompt);
            accessible_text_edit(
                ui.add(
                    egui::TextEdit::multiline(resume_answer)
                        .hint_text("Response")
                        .desired_rows(2),
                ),
                "Response",
            );
            ui.add_enabled(resume_available, egui::Button::new("Approve and resume"))
                .clicked()
                .then_some(ControlAction::Resume)
        })
        .inner
}

fn accessible_text_edit(response: egui::Response, label: &str) {
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, response.enabled(), label)
    });
}

fn render_heading(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("Mission Command");
        status_pill(
            ui,
            StatusKind::Info,
            "Supervised",
            Some("Switchbard submits intentions; xplan owns every write"),
        );
    });
    ui.label(
        egui::RichText::new("Outcome supervision across projects, tasks, and agent units")
            .color(theme::muted_text()),
    );
    render_process_legend(ui);
    ui.add_space(10.0);
}

fn render_process_legend(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Process")
                .small()
                .strong()
                .color(theme::muted_text()),
        );
        ui.label(
            egui::RichText::new("Queue -> Work -> Evidence -> Reconcile -> Approval -> Done")
                .small()
                .color(theme::muted_text()),
        );
    });
}

fn render_loading(ui: &mut egui::Ui, path: &std::path::Path) {
    state_card(
        ui,
        "Loading mission snapshot",
        path,
        "The background reader has not completed its first bounded poll.",
    );
}

fn render_missing(ui: &mut egui::Ui, path: &std::path::Path) {
    state_card(
        ui,
        "No mission snapshot yet",
        path,
        "Switchbard remains fully usable. Start or demo Mission Command in xplan to publish this optional read model.",
    );
}

fn render_error(ui: &mut egui::Ui, title: &str, path: &std::path::Path, message: &str) {
    state_card(ui, title, path, message);
    ui.colored_label(
        theme::warn_orange(),
        "The last read was rejected; no partial mission data is shown.",
    );
}

fn render_unsupported(ui: &mut egui::Ui, path: &std::path::Path, found: &str) {
    state_card(
        ui,
        "Snapshot version unsupported",
        path,
        &format!("Found {found}; this build reads xplan-mission-projection-v1."),
    );
}

fn state_card(ui: &mut egui::Ui, title: &str, path: &std::path::Path, detail: &str) {
    egui::Frame::group(ui.style())
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).strong());
            ui.label(egui::RichText::new(detail).color(theme::muted_text()));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(path.display().to_string())
                    .monospace()
                    .color(theme::weak_text()),
            );
        });
}

fn render_projection(
    ui: &mut egui::Ui,
    path: &std::path::Path,
    projection: &MissionProjection,
    freshness: &ProjectionFreshness,
    query: &str,
) {
    render_projection_meta(ui, path, projection, freshness);
    if projection.portfolio.missions.is_empty() {
        render_empty(ui, &projection.portfolio.id);
        return;
    }
    render_mission_list(ui, projection, query);
}

fn render_mission_list(ui: &mut egui::Ui, projection: &MissionProjection, query: &str) {
    let visible = projection
        .portfolio
        .missions
        .iter()
        .filter(|mission| mission_matches(mission, query));
    let mut count = 0usize;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for mission in visible {
                count += 1;
                render_mission(ui, mission);
                ui.add_space(8.0);
            }
            if count == 0 {
                ui.label("No missions match the current search.");
            }
        });
}

fn render_projection_meta(
    ui: &mut egui::Ui,
    path: &std::path::Path,
    projection: &MissionProjection,
    freshness: &ProjectionFreshness,
) {
    let summary = mission_summary(projection);
    render_projection_identity(ui, projection, &summary);
    render_freshness(ui, freshness);
    ui.label(
        egui::RichText::new(path.display().to_string())
            .small()
            .monospace()
            .color(theme::weak_text()),
    );
    ui.add_space(8.0);
}

fn render_projection_identity(
    ui: &mut egui::Ui,
    projection: &MissionProjection,
    summary: &MissionSummary,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(&projection.portfolio.id).strong());
        if projection.portfolio.id.starts_with("DEVELOPER-QA-") {
            status_pill(
                ui,
                StatusKind::Neutral,
                "Developer QA fixture",
                Some("Synthetic states, not live user projects"),
            );
        }
        ui.label(format!("revision {}", projection.revision));
        ui.separator();
        ui.label(format!("{} active", summary.active));
        ui.label(format!("{} need attention", summary.attention));
        ui.label(format!("{} done", summary.done));
    });
}

fn render_freshness(ui: &mut egui::Ui, freshness: &ProjectionFreshness) {
    match freshness {
        ProjectionFreshness::Fresh { age_seconds } => {
            ui.label(
                egui::RichText::new(format!("Updated {} ago", age_label(*age_seconds)))
                    .color(theme::muted_text()),
            );
        }
        ProjectionFreshness::Stale {
            age_seconds,
            limit_seconds,
        } => {
            ui.colored_label(
                theme::amber(),
                format!(
                    "Stale snapshot: updated {} ago (expected within {})",
                    age_label(*age_seconds),
                    age_label(*limit_seconds)
                ),
            );
        }
    }
}

fn render_empty(ui: &mut egui::Ui, portfolio_id: &str) {
    egui::Frame::group(ui.style())
        .fill(theme::card_bg())
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Queue is empty").strong());
            ui.label(format!("{portfolio_id} has no missions to supervise."));
        });
}

fn render_mission(ui: &mut egui::Ui, mission: &ProjectedMission) {
    egui::Frame::group(ui.style())
        .fill(theme::card_bg())
        .stroke(theme::surface_stroke())
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(12))
        .shadow(theme::card_shadow())
        .show(ui, |ui| {
            render_mission_header(ui, mission);
            render_progress(ui, mission);
            render_outcome_gap(ui, mission);
            render_attention(ui, mission);
            render_next_step(ui, mission);
            render_operational_summary(ui, mission);
        });
}

fn render_mission_header(ui: &mut egui::Ui, mission: &ProjectedMission) {
    ui.horizontal_wrapped(|ui| {
        if mission.status.is_active() {
            theme::painted_dot_pulse(ui, theme::green(), 1);
        }
        ui.label(
            egui::RichText::new(format!("Mission {}", mission.id))
                .strong()
                .size(16.0),
        );
        status_pill(
            ui,
            status_kind(mission.status),
            mission.status.label(),
            None,
        );
        ui.label(
            egui::RichText::new(format!(
                "{} · contract v{}",
                mission.attempt_id, mission.contract_version
            ))
            .monospace()
            .color(theme::weak_text()),
        );
    });
}

fn render_progress(ui: &mut egui::Ui, mission: &ProjectedMission) {
    let total = mission.requirements.len();
    let proven = mission
        .requirements
        .iter()
        .filter(|item| item.status == RequirementStatus::Proven)
        .count();
    let fraction = if total == 0 {
        0.0
    } else {
        proven as f32 / total as f32
    };
    let text = format!("{proven}/{total} outcome requirements proven");
    ui.add(egui::ProgressBar::new(fraction).text(text));
}

fn render_outcome_gap(ui: &mut egui::Ui, mission: &ProjectedMission) {
    let mut shown = 0usize;
    for requirement in mission
        .requirements
        .iter()
        .filter(|item| item.status == RequirementStatus::Open)
        .take(3)
    {
        let text = format!(
            "Open requirement {} · needs {} evidence",
            requirement.id, requirement.evidence_kind
        );
        ui.label(egui::RichText::new(text).color(theme::muted_text()));
        shown += 1;
    }
    let total_open = mission
        .requirements
        .iter()
        .filter(|item| item.status == RequirementStatus::Open)
        .count();
    if total_open > shown {
        ui.label(format!(
            "+ {} more open outcome requirements",
            total_open - shown
        ));
    }
}

fn render_attention(ui: &mut egui::Ui, mission: &ProjectedMission) {
    render_decision_attention(ui, mission);
    render_approval_attention(ui, mission);
    render_feedback_attention(ui, mission);
}

fn render_decision_attention(ui: &mut egui::Ui, mission: &ProjectedMission) {
    if let Some(decision) = mission
        .decision
        .as_ref()
        .filter(|item| item.status == DecisionStatus::Open)
    {
        attention_line(
            ui,
            "Decision",
            &format!("{} v{} is open", decision.id, decision.version),
        );
    }
}

fn render_approval_attention(ui: &mut egui::Ui, mission: &ProjectedMission) {
    if let Some(approval) = mission
        .approval
        .as_ref()
        .filter(|item| item.status == ApprovalStatus::Requested)
    {
        attention_line(ui, "Approval", &format!("{} requested", approval.id));
    }
}

fn render_feedback_attention(ui: &mut egui::Ui, mission: &ProjectedMission) {
    for feedback in mission
        .feedback
        .iter()
        .filter(|item| item.status == FeedbackStatus::Queued)
        .take(3)
    {
        attention_line(
            ui,
            "Feedback queued",
            &format!("{} v{}", feedback.id, feedback.version),
        );
    }
}

fn attention_line(ui: &mut egui::Ui, label: &str, text: &str) {
    egui::Frame::NONE
        .fill(theme::faint_bg())
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(theme::amber(), egui::RichText::new(label).strong());
                ui.label(text);
            });
        });
}

fn render_next_step(ui: &mut egui::Ui, mission: &ProjectedMission) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Next").strong());
        ui.label(&mission.next_step);
        if !mission.next_owner.is_empty() {
            ui.label(
                egui::RichText::new(format!("Owner: {}", mission.next_owner))
                    .color(theme::muted_text()),
            );
        }
    });
}

fn render_operational_summary(ui: &mut egui::Ui, mission: &ProjectedMission) {
    render_operational_counts(ui, mission);
    render_unit_owners(ui, mission);
}

fn render_operational_counts(ui: &mut egui::Ui, mission: &ProjectedMission) {
    let active_units = unit_count(mission, UnitStatus::Active);
    let held_units = unit_count(mission, UnitStatus::Held);
    let queued_feedback = mission
        .feedback
        .iter()
        .filter(|item| item.status == FeedbackStatus::Queued)
        .count();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("{active_units} active units")).color(theme::muted_text()),
        );
        ui.label(egui::RichText::new(format!("{held_units} held")).color(theme::muted_text()));
        ui.label(
            egui::RichText::new(format!("{queued_feedback} feedback queued"))
                .color(theme::muted_text()),
        );
        ui.label(
            egui::RichText::new(format!("{} evidence", mission.evidence.len()))
                .color(theme::muted_text()),
        );
        render_reconciliation(ui, mission);
    });
}

fn unit_count(mission: &ProjectedMission, status: UnitStatus) -> usize {
    mission
        .units
        .iter()
        .filter(|unit| unit.status == status)
        .count()
}

fn render_unit_owners(ui: &mut egui::Ui, mission: &ProjectedMission) {
    if mission.units.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Unit owners")
                .small()
                .color(theme::muted_text()),
        );
        for unit in mission.units.iter().take(4) {
            ui.label(format!("{} · {} leases", unit.owner, unit.lease_count));
        }
        if mission.units.len() > 4 {
            ui.label(format!("+{} more", mission.units.len() - 4));
        }
    });
}

fn render_reconciliation(ui: &mut egui::Ui, mission: &ProjectedMission) {
    let Some(reconciliation) = &mission.reconciliation else {
        return;
    };
    let (kind, text) = match reconciliation.status {
        ReconciliationStatus::Pass => (StatusKind::Good, "reconciliation passed"),
        ReconciliationStatus::Fail => (StatusKind::Danger, "reconciliation failed"),
    };
    status_pill(ui, kind, text, None);
}

fn mission_matches(mission: &ProjectedMission, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let fields = [
        mission.id.as_str(),
        mission.status.label(),
        mission.attempt_id.as_str(),
        mission.next_step.as_str(),
        mission.next_owner.as_str(),
    ];
    fields
        .iter()
        .any(|field| field.to_lowercase().contains(query))
        || mission.units.iter().any(|unit| {
            unit.owner.to_lowercase().contains(query) || unit.id.to_lowercase().contains(query)
        })
        || mission.requirements.iter().any(|requirement| {
            requirement.id.to_lowercase().contains(query)
                || requirement.evidence_kind.to_lowercase().contains(query)
        })
}

#[derive(Default)]
struct MissionSummary {
    active: usize,
    attention: usize,
    done: usize,
}

fn mission_summary(projection: &MissionProjection) -> MissionSummary {
    let mut summary = MissionSummary::default();
    for mission in &projection.portfolio.missions {
        match mission.status {
            MissionStatus::Running => summary.active += 1,
            MissionStatus::NeedsDecision
            | MissionStatus::NeedsSupport
            | MissionStatus::ExternalBlock
            | MissionStatus::ApprovalPending => summary.attention += 1,
            MissionStatus::MissionDone => summary.done += 1,
            _ => {}
        }
    }
    summary
}

fn status_kind(status: MissionStatus) -> StatusKind {
    match status {
        MissionStatus::Running | MissionStatus::OutcomeProven | MissionStatus::MissionDone => {
            StatusKind::Good
        }
        MissionStatus::NeedsDecision | MissionStatus::ApprovalPending => StatusKind::Warn,
        MissionStatus::NeedsSupport | MissionStatus::ExternalBlock => StatusKind::Danger,
        MissionStatus::Queued => StatusKind::Info,
        MissionStatus::Draft | MissionStatus::Paused | MissionStatus::Canceled => {
            StatusKind::Neutral
        }
    }
}

fn age_label(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3_600)
    }
}

#[cfg(test)]
mod tests {
    use super::age_label;

    #[test]
    fn mission_age_label_uses_bounded_human_units() {
        assert_eq!(age_label(59), "59s");
        assert_eq!(age_label(60), "1m");
        assert_eq!(age_label(3_600), "1h");
    }
}
