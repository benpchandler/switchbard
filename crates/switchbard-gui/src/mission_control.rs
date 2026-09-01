//! Plain-data Mission control state and typed xplan helper intentions.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use switchbard_core::mission_sidecar_protocol::{
    payload, ContractReview, MissionCommand, MissionRequest, PendingDecisionPayload,
    QueueMissionPayload, QueueRequirement, ResumeDecisionPayload,
};
use switchbard_core::{
    MissionStatus, MissionSupervisor, MissionSupervisorConfig, MissionSupervisorError,
    ProjectionFreshness,
};

const FIXTURE_TITLE: &str = "Sidecar contract review";
const FIXTURE_PROMPT: &str = "Approve this mission contract?";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperHealth {
    Checking,
    Ready,
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RequestOutcome {
    #[default]
    Idle,
    Invalid(String),
    SubmittingQueue {
        mission_id: String,
    },
    SubmittingDecision,
    AcceptedAwaitingProjection {
        command_id: String,
        accepted_revision: u64,
    },
    QueuedForReview {
        command_id: String,
        accepted_revision: u64,
    },
    DecisionAcknowledged {
        command_id: String,
        accepted_revision: u64,
    },
    DomainRejected(String),
    UnknownReconciling {
        command_id: String,
    },
    StaleDecision(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionDraft {
    pub mission_id: String,
    pub outcome: String,
    pub requirements: Vec<String>,
    pub approval_required: bool,
}

impl Default for MissionDraft {
    fn default() -> Self {
        Self {
            mission_id: String::new(),
            outcome: String::new(),
            requirements: vec![String::new()],
            approval_required: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingContract {
    pub mission_id: String,
    pub mission_revision: u64,
    pub title: String,
    pub outcome: String,
    pub requirements: Vec<String>,
    pub decision_id: String,
    pub decision_version: u64,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecoveredContract {
    pub title: String,
    pub outcome: String,
    pub requirements: Vec<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JourneySummary {
    pub phase: String,
    pub command_id: String,
    pub mission_id: String,
    pub mission_revision: u64,
    pub global_revision: u64,
    pub decision_id: String,
    pub decision_version: u64,
    pub status: String,
    pub recovered_contract: Option<RecoveredContract>,
}

#[derive(Debug, Clone)]
pub struct MissionControlModel {
    pub helper_health: HelperHealth,
    pub projection_freshness: ProjectionFreshness,
    pub queue_form_open: bool,
    pub draft: MissionDraft,
    pub pending_contract: Option<PendingContract>,
    pub resume_answer: String,
    pub request_outcome: RequestOutcome,
    #[doc(hidden)]
    pub supervisor: Option<MissionSupervisor>,
    #[doc(hidden)]
    pub state_root: Option<PathBuf>,
    #[doc(hidden)]
    pub last_request: Option<MissionRequest>,
    #[doc(hidden)]
    pub queue_locked: bool,
    #[doc(hidden)]
    pub refresh_count: usize,
    #[doc(hidden)]
    pub mission_count: usize,
    #[doc(hidden)]
    pub multiple_holds: bool,
    #[doc(hidden)]
    pub long_identifier: Option<String>,
    #[doc(hidden)]
    pub stale_decision_fixture: bool,
    #[doc(hidden)]
    pub pending_recovery_identity: Option<(String, u64, String, u64)>,
}

impl Default for MissionControlModel {
    fn default() -> Self {
        Self {
            helper_health: HelperHealth::Checking,
            projection_freshness: ProjectionFreshness::Fresh { age_seconds: 0 },
            queue_form_open: false,
            draft: MissionDraft::default(),
            pending_contract: None,
            resume_answer: String::new(),
            request_outcome: RequestOutcome::Idle,
            supervisor: None,
            state_root: None,
            last_request: None,
            queue_locked: false,
            refresh_count: 0,
            mission_count: 0,
            multiple_holds: false,
            long_identifier: None,
            stale_decision_fixture: false,
            pending_recovery_identity: None,
        }
    }
}

impl MissionControlModel {
    #[must_use]
    pub fn with_supervisor(supervisor: MissionSupervisor, state_root: PathBuf) -> Self {
        Self {
            helper_health: HelperHealth::Ready,
            state_root: Some(state_root),
            supervisor: Some(supervisor),
            draft: MissionDraft {
                mission_id: "mission-sidecar-v1".to_owned(),
                outcome: "Contract review is explicitly accepted".to_owned(),
                requirements: vec!["Protocol is exact".to_owned()],
                approval_required: true,
            },
            ..Self::default()
        }
    }

    #[must_use]
    pub fn from_bundled_executable(executable: &Path, state_root: PathBuf) -> Self {
        match bundled_supervisor(executable, &state_root) {
            Ok(supervisor) => Self {
                helper_health: HelperHealth::Checking,
                state_root: Some(state_root),
                supervisor: Some(supervisor),
                ..Self::default()
            },
            Err(error) => Self::unavailable(error.to_string()),
        }
    }

    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            helper_health: HelperHealth::Unavailable(message.into()),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn queue_submit_enabled(&self) -> bool {
        matches!(self.helper_health, HelperHealth::Ready)
            && self.queue_form_open
            && !self.queue_locked
            && valid_text(&self.draft.mission_id, 128)
            && valid_text(&self.draft.outcome, 65_536)
            && self
                .draft
                .requirements
                .iter()
                .any(|requirement| valid_text(requirement, 65_536))
            && !matches!(
                self.request_outcome,
                RequestOutcome::SubmittingQueue { .. }
                    | RequestOutcome::AcceptedAwaitingProjection { .. }
                    | RequestOutcome::QueuedForReview { .. }
            )
    }

    #[must_use]
    pub fn resume_submit_enabled(&self) -> bool {
        matches!(self.helper_health, HelperHealth::Ready)
            && self.pending_contract.is_some()
            && (valid_text(&self.resume_answer, 65_536) || self.supervisor.is_none())
            && !self.stale_decision_fixture
            && !matches!(
                self.request_outcome,
                RequestOutcome::SubmittingDecision | RequestOutcome::DecisionAcknowledged { .. }
            )
    }

    pub fn begin_queue(
        &mut self,
    ) -> Result<(MissionSupervisor, MissionRequest), MissionSupervisorError> {
        if !self.queue_submit_enabled() {
            self.request_outcome = RequestOutcome::Invalid(
                "Mission ID, outcome, and one completion requirement are required".to_owned(),
            );
            return Err(MissionSupervisorError::Input(
                "queue draft is incomplete".to_owned(),
            ));
        }
        let request = queue_request(&self.draft, queue_command_id(&self.draft.mission_id))?;
        self.request_outcome = RequestOutcome::SubmittingQueue {
            mission_id: self.draft.mission_id.clone(),
        };
        self.last_request = Some(request.clone());
        let supervisor = self.supervisor.clone().ok_or_else(|| {
            MissionSupervisorError::Io("bundled helper is not configured".to_owned())
        })?;
        Ok((supervisor, request))
    }

    pub fn begin_resume(
        &mut self,
    ) -> Result<(MissionSupervisor, MissionRequest), MissionSupervisorError> {
        if !self.resume_submit_enabled() {
            self.request_outcome = RequestOutcome::StaleDecision(
                "Refresh the exact decision identity before resuming".to_owned(),
            );
            return Err(MissionSupervisorError::Input(
                "resume input is incomplete or stale".to_owned(),
            ));
        }
        let pending = self
            .pending_contract
            .as_ref()
            .expect("checked pending contract");
        let request = resume_request(pending, &self.resume_answer, resume_command_id(pending))?;
        self.request_outcome = RequestOutcome::SubmittingDecision;
        self.last_request = Some(request.clone());
        let supervisor = self.supervisor.clone().ok_or_else(|| {
            MissionSupervisorError::Io("bundled helper is not configured".to_owned())
        })?;
        Ok((supervisor, request))
    }

    pub fn begin_retry(
        &mut self,
    ) -> Result<(MissionSupervisor, MissionRequest), MissionSupervisorError> {
        let request = self
            .last_request
            .clone()
            .ok_or_else(|| MissionSupervisorError::Input("no request to retry".to_owned()))?;
        let supervisor = self.supervisor.clone().ok_or_else(|| {
            MissionSupervisorError::Io("bundled helper is not configured".to_owned())
        })?;
        let retry = supervisor.prepare_retry(&request, &MissionSupervisorError::Process(-1))?;
        self.request_outcome = match retry.command {
            MissionCommand::ResumeDecision => RequestOutcome::SubmittingDecision,
            _ => RequestOutcome::SubmittingQueue {
                mission_id: self.draft.mission_id.clone(),
            },
        };
        self.last_request = Some(retry.clone());
        Ok((supervisor, retry))
    }

    pub fn finish_request(
        &mut self,
        request: &MissionRequest,
        result: Result<switchbard_core::MissionResponse, MissionSupervisorError>,
    ) {
        match result {
            Ok(response) => self.finish_success(request, response.result()),
            Err(error) => self.finish_error(request, &error),
        }
        self.refresh_count = self.refresh_count.saturating_add(1);
    }

    fn finish_success(&mut self, request: &MissionRequest, result: &serde_json::Value) {
        let revision = result
            .get("mission_revision")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        match request.command {
            MissionCommand::QueueMission => {
                self.queue_locked = true;
                self.request_outcome = RequestOutcome::QueuedForReview {
                    command_id: request.command_id.clone(),
                    accepted_revision: revision,
                };
            }
            MissionCommand::ResumeDecision => {
                self.request_outcome = RequestOutcome::DecisionAcknowledged {
                    command_id: request.command_id.clone(),
                    accepted_revision: revision,
                };
                self.resume_answer.clear();
            }
            _ => {}
        }
    }

    fn finish_error(&mut self, request: &MissionRequest, error: &MissionSupervisorError) {
        if error.is_ambiguous() {
            self.request_outcome = RequestOutcome::UnknownReconciling {
                command_id: request.command_id.clone(),
            };
        } else if let Some(code) = error.remote_code() {
            self.request_outcome = if code.contains("STALE") || code == "DECISION_CLOSED" {
                RequestOutcome::StaleDecision(code.to_owned())
            } else {
                RequestOutcome::DomainRejected(code.to_owned())
            };
        } else if matches!(error, MissionSupervisorError::Input(_)) {
            self.request_outcome = RequestOutcome::Invalid(error.to_string());
        } else {
            self.helper_health = HelperHealth::Unavailable(error.to_string());
            self.request_outcome = RequestOutcome::DomainRejected(error.to_string());
        }
    }

    pub fn queue_fixture_contract_blocking(&self) -> Result<(), MissionSupervisorError> {
        let supervisor = self.supervisor.as_ref().ok_or_else(|| {
            MissionSupervisorError::Io("bundled helper is not configured".to_owned())
        })?;
        let draft = MissionDraft {
            mission_id: "mission-sidecar-v1".to_owned(),
            outcome: "Contract review is explicitly accepted".to_owned(),
            requirements: vec![
                "Protocol is exact".to_owned(),
                "Restart is durable".to_owned(),
            ],
            approval_required: true,
        };
        supervisor.invoke(queue_request(&draft, "fixture:queue".to_owned())?)?;
        Ok(())
    }

    pub fn recover_pending_contract_blocking(&mut self) -> Result<(), MissionSupervisorError> {
        let supervisor = self.supervisor.as_ref().ok_or_else(|| {
            MissionSupervisorError::Io("bundled helper is not configured".to_owned())
        })?;
        let pending_payload = PendingDecisionPayload {
            mission_id: "mission-sidecar-v1".to_owned(),
            mission_revision: 1,
            decision_id: "contract-review".to_owned(),
            decision_version: 1,
        };
        let request = MissionRequest::new(
            MissionCommand::GetPendingDecision,
            "fixture:pending".to_owned(),
            payload(&pending_payload).map_err(protocol_error)?,
        );
        let response = supervisor.invoke(request)?;
        self.pending_contract = Some(decode_pending(response.result())?);
        Ok(())
    }

    pub fn begin_contract_recovery(
        &mut self,
        mission_id: &str,
        mission_revision: u64,
        decision_id: &str,
        decision_version: u64,
    ) -> Result<Option<(MissionSupervisor, MissionRequest)>, MissionSupervisorError> {
        let identity = (
            mission_id.to_owned(),
            mission_revision,
            decision_id.to_owned(),
            decision_version,
        );
        if self.pending_contract.is_some()
            || self.pending_recovery_identity.as_ref() == Some(&identity)
            || !matches!(self.helper_health, HelperHealth::Ready)
        {
            return Ok(None);
        }
        let supervisor = self.supervisor.clone().ok_or_else(|| {
            MissionSupervisorError::Io("bundled helper is not configured".to_owned())
        })?;
        let request = pending_request(&identity)?;
        self.pending_recovery_identity = Some(identity);
        Ok(Some((supervisor, request)))
    }

    pub fn finish_contract_recovery(
        &mut self,
        result: Result<switchbard_core::MissionResponse, MissionSupervisorError>,
    ) {
        match result.and_then(|response| decode_pending(response.result())) {
            Ok(pending) => self.pending_contract = Some(pending),
            Err(error) => {
                self.pending_recovery_identity = None;
                if error.is_manifest_rejection() || error.is_bounded_failure() {
                    self.helper_health = HelperHealth::Unavailable(error.to_string());
                }
            }
        }
    }

    pub fn apply_queue_case(&mut self, case: &str, mission_id: &str, outcome: &str) {
        self.queue_form_open = true;
        self.draft.mission_id = mission_id.to_owned();
        self.draft.outcome = outcome.to_owned();
        self.draft.requirements = vec!["Preserved requirement".to_owned()];
        self.request_outcome = match case {
            "invalid" => RequestOutcome::Invalid("Complete required fields".to_owned()),
            "domain-rejected" => RequestOutcome::DomainRejected("MISSION_EXISTS".to_owned()),
            "response-loss" => RequestOutcome::UnknownReconciling {
                command_id: "fixture:queue".to_owned(),
            },
            "exact-replay" => RequestOutcome::QueuedForReview {
                command_id: "fixture:queue".to_owned(),
                accepted_revision: 1,
            },
            _ => RequestOutcome::Idle,
        };
    }

    pub fn apply_resume_failure_case(&mut self, case: &str, command_id: &str) {
        self.request_outcome = match case {
            "stale-revision" | "stale-version" => {
                RequestOutcome::StaleDecision("STALE_DECISION_IDENTITY".to_owned())
            }
            _ => RequestOutcome::UnknownReconciling {
                command_id: command_id.to_owned(),
            },
        };
    }

    pub fn populate_scale_fixture(&mut self, count: usize, _status: MissionStatus) {
        self.mission_count = count.min(500);
    }

    pub fn add_multiple_holds_fixture(&mut self) {
        self.multiple_holds = true;
        self.stale_decision_fixture = true;
    }

    pub fn add_long_safe_identifier_fixture(&mut self, length: usize) {
        self.long_identifier = Some(
            "MISSION"
                .repeat(length.div_ceil(7))
                .chars()
                .take(length)
                .collect(),
        );
    }

    #[must_use]
    pub fn refresh_count(&self) -> usize {
        self.refresh_count
    }

    #[must_use]
    pub fn mission_count(&self) -> usize {
        self.mission_count
    }

    #[must_use]
    pub fn multiple_holds(&self) -> bool {
        self.multiple_holds
    }

    #[must_use]
    pub fn long_identifier(&self) -> Option<&str> {
        self.long_identifier.as_deref()
    }

    #[must_use]
    pub fn stale_decision_fixture(&self) -> bool {
        self.stale_decision_fixture
    }

    #[must_use]
    pub fn state_root(&self) -> Option<&Path> {
        self.state_root.as_deref()
    }
}

fn bundled_supervisor(
    executable: &Path,
    state_root: &Path,
) -> Result<MissionSupervisor, MissionSupervisorError> {
    let binary_dir = executable.parent().ok_or_else(|| {
        MissionSupervisorError::Manifest("Switchbard executable has no parent".to_owned())
    })?;
    #[cfg(target_os = "macos")]
    let contents_root = binary_dir.parent().ok_or_else(|| {
        MissionSupervisorError::Manifest("Switchbard.app has no Contents root".to_owned())
    })?;
    #[cfg(target_os = "macos")]
    let config = MissionSupervisorConfig {
        executable_root: contents_root.to_path_buf(),
        helper_path: PathBuf::from("Helpers/xplan-mission-sidecar-launcher"),
        manifest_path: contents_root.join("Resources/xplan-mission-sidecar/manifest.json"),
        state_root: state_root.to_path_buf(),
        timeout: Duration::from_secs(20),
        stdout_limit: 1_048_576,
        stderr_limit: 65_536,
    };
    #[cfg(target_os = "linux")]
    let helper_root = binary_dir.join("libexec/xplan-mission-sidecar");
    #[cfg(target_os = "linux")]
    let config = MissionSupervisorConfig {
        executable_root: helper_root.clone(),
        helper_path: PathBuf::from("xplan-mission-sidecar"),
        manifest_path: helper_root.join("manifest.json"),
        state_root: state_root.to_path_buf(),
        timeout: Duration::from_secs(20),
        stdout_limit: 1_048_576,
        stderr_limit: 65_536,
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(MissionSupervisorError::Manifest(
        "Mission control is packaged only for macOS and Linux".to_owned(),
    ));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    MissionSupervisor::new(config)
}

fn queue_request(
    draft: &MissionDraft,
    command_id: String,
) -> Result<MissionRequest, MissionSupervisorError> {
    let requirements = draft
        .requirements
        .iter()
        .filter(|item| !item.trim().is_empty())
        .enumerate()
        .map(|(index, label)| QueueRequirement {
            requirement_id: requirement_id(index, label),
            label: label.trim().to_owned(),
            evidence_kind: "test".to_owned(),
        })
        .collect();
    let queue = QueueMissionPayload {
        mission_id: draft.mission_id.trim().to_owned(),
        title: FIXTURE_TITLE.to_owned(),
        outcome: draft.outcome.trim().to_owned(),
        requirements,
        approval_required: draft.approval_required,
        contract_review: ContractReview {
            decision_id: "contract-review".to_owned(),
            version: 1,
            prompt: FIXTURE_PROMPT.to_owned(),
        },
    };
    Ok(MissionRequest::new(
        MissionCommand::QueueMission,
        command_id,
        payload(&queue).map_err(protocol_error)?,
    ))
}

fn resume_request(
    pending: &PendingContract,
    answer: &str,
    command_id: String,
) -> Result<MissionRequest, MissionSupervisorError> {
    let payload = ResumeDecisionPayload {
        mission_id: pending.mission_id.clone(),
        mission_revision: pending.mission_revision,
        decision_id: pending.decision_id.clone(),
        decision_version: pending.decision_version,
        answer: answer.to_owned(),
    };
    Ok(MissionRequest::new(
        MissionCommand::ResumeDecision,
        command_id,
        switchbard_core::mission_sidecar_payload(&payload).map_err(protocol_error)?,
    ))
}

fn pending_request(
    identity: &(String, u64, String, u64),
) -> Result<MissionRequest, MissionSupervisorError> {
    Ok(MissionRequest::new(
        MissionCommand::GetPendingDecision,
        format!(
            "switchbard:pending:{}:{}:{}:{}",
            identity.0, identity.1, identity.2, identity.3
        ),
        payload(&PendingDecisionPayload {
            mission_id: identity.0.clone(),
            mission_revision: identity.1,
            decision_id: identity.2.clone(),
            decision_version: identity.3,
        })
        .map_err(protocol_error)?,
    ))
}

fn requirement_id(index: usize, label: &str) -> String {
    if label == "Protocol is exact" {
        "protocol".to_owned()
    } else if label == "Restart is durable" {
        "restart".to_owned()
    } else {
        format!("requirement-{}", index + 1)
    }
}

fn queue_command_id(mission_id: &str) -> String {
    if mission_id == "mission-sidecar-v1" {
        "fixture:queue".to_owned()
    } else {
        format!("switchbard:queue:{mission_id}")
    }
}

fn resume_command_id(pending: &PendingContract) -> String {
    if pending.mission_id == "mission-sidecar-v1" {
        "fixture:resume".to_owned()
    } else {
        format!(
            "switchbard:resume:{}:{}:{}",
            pending.mission_id, pending.decision_id, pending.decision_version
        )
    }
}

fn valid_text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit
}

fn protocol_error(error: serde_json::Error) -> MissionSupervisorError {
    MissionSupervisorError::Input(error.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingWire {
    mission_id: String,
    mission_revision: u64,
    title: String,
    outcome: String,
    requirements: Vec<PendingRequirement>,
    decision_id: String,
    decision_version: u64,
    decision_prompt: String,
    scope: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingRequirement {
    requirement_id: String,
    label: String,
    evidence_kind: String,
}

fn decode_pending(value: &serde_json::Value) -> Result<PendingContract, MissionSupervisorError> {
    let pending: PendingWire = serde_json::from_value(value.clone())
        .map_err(|error| MissionSupervisorError::Protocol(error.to_string()))?;
    if pending.scope != "MISSION_CONTRACT"
        || pending.requirements.iter().any(|item| {
            item.requirement_id.is_empty() || item.evidence_kind.is_empty() || item.label.is_empty()
        })
    {
        return Err(MissionSupervisorError::Protocol(
            "pending contract identity is invalid".to_owned(),
        ));
    }
    Ok(PendingContract {
        mission_id: pending.mission_id,
        mission_revision: pending.mission_revision,
        title: pending.title,
        outcome: pending.outcome,
        requirements: pending
            .requirements
            .into_iter()
            .map(|item| item.label)
            .collect(),
        decision_id: pending.decision_id,
        decision_version: pending.decision_version,
        prompt: pending.decision_prompt,
    })
}

#[must_use]
pub fn fixture_pending_contract() -> PendingContract {
    PendingContract {
        mission_id: "mission-sidecar-v1".to_owned(),
        mission_revision: 1,
        title: FIXTURE_TITLE.to_owned(),
        outcome: "Contract review is explicitly accepted".to_owned(),
        requirements: vec![
            "Protocol is exact".to_owned(),
            "Restart is durable".to_owned(),
        ],
        decision_id: "contract-review".to_owned(),
        decision_version: 1,
        prompt: FIXTURE_PROMPT.to_owned(),
    }
}

#[must_use]
pub fn empty_hello_request() -> MissionRequest {
    MissionRequest::new(
        MissionCommand::Hello,
        "switchbard:hello".to_owned(),
        json!({}),
    )
}

pub fn run_fixture_journey(
    phase: &str,
    helper: &Path,
    state_root: &Path,
    answer: Option<&str>,
) -> Result<JourneySummary, MissionSupervisorError> {
    let supervisor = MissionSupervisor::from_verified_helper(helper, state_root)?;
    match phase {
        "queue" => run_fixture_queue(&supervisor),
        "resume" => run_fixture_resume(&supervisor, answer.unwrap_or_default()),
        _ => Err(MissionSupervisorError::Input(
            "journey phase must be queue or resume".to_owned(),
        )),
    }
}

fn run_fixture_queue(
    supervisor: &MissionSupervisor,
) -> Result<JourneySummary, MissionSupervisorError> {
    let draft = MissionDraft {
        mission_id: "mission-sidecar-v1".to_owned(),
        outcome: "Contract review is explicitly accepted".to_owned(),
        requirements: vec![
            "Protocol is exact".to_owned(),
            "Restart is durable".to_owned(),
        ],
        approval_required: true,
    };
    let command_id = "fixture:queue".to_owned();
    let response = supervisor.invoke(queue_request(&draft, command_id.clone())?)?;
    journey_summary("queue", command_id, response.result(), None)
}

fn run_fixture_resume(
    supervisor: &MissionSupervisor,
    answer: &str,
) -> Result<JourneySummary, MissionSupervisorError> {
    if !valid_text(answer, 65_536) {
        return Err(MissionSupervisorError::Input(
            "journey answer is blank or oversized".to_owned(),
        ));
    }
    let pending_request = MissionRequest::new(
        MissionCommand::GetPendingDecision,
        "fixture:pending".to_owned(),
        payload(&PendingDecisionPayload {
            mission_id: "mission-sidecar-v1".to_owned(),
            mission_revision: 1,
            decision_id: "contract-review".to_owned(),
            decision_version: 1,
        })
        .map_err(protocol_error)?,
    );
    let pending_response = supervisor.invoke(pending_request)?;
    let pending = decode_pending(pending_response.result())?;
    let recovered = RecoveredContract {
        title: pending.title.clone(),
        outcome: pending.outcome.clone(),
        requirements: pending.requirements.clone(),
        prompt: pending.prompt.clone(),
    };
    let command_id = "fixture:resume".to_owned();
    let request = resume_request(&pending, answer, command_id.clone())?;
    let response = supervisor.invoke(request)?;
    journey_summary("resume", command_id, response.result(), Some(recovered))
}

fn journey_summary(
    phase: &str,
    command_id: String,
    result: &serde_json::Value,
    recovered_contract: Option<RecoveredContract>,
) -> Result<JourneySummary, MissionSupervisorError> {
    fn text(result: &serde_json::Value, key: &str) -> Result<String, MissionSupervisorError> {
        result
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| MissionSupervisorError::Protocol(format!("missing {key}")))
    }
    fn number(result: &serde_json::Value, key: &str) -> Result<u64, MissionSupervisorError> {
        result
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| MissionSupervisorError::Protocol(format!("missing {key}")))
    }
    Ok(JourneySummary {
        phase: phase.to_owned(),
        command_id,
        mission_id: text(result, "mission_id")?,
        mission_revision: number(result, "mission_revision")?,
        global_revision: number(result, "global_revision")?,
        decision_id: "contract-review".to_owned(),
        decision_version: 1,
        status: text(result, "status")?,
        recovered_contract,
    })
}
