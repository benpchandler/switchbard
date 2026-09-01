//! Read-only adapter for xplan's optional Mission Command projection.
//!
//! xplan owns the event log and mission semantics. Switchbard reads one
//! atomically-published JSON snapshot and never shells out to xplan or writes
//! mission state. The adapter validates the versioned boundary once, caps the
//! input and row count, and gives the GUI explicit unavailable states.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const MISSION_PROJECTION_SCHEMA: &str = "xplan-mission-projection-v1";
pub const MISSION_PROJECTION_SCHEMA_V2: &str = "xplan-mission-projection-v2";
pub const MISSION_PROJECTION_ENV: &str = "XPLAN_MISSION_SNAPSHOT";
pub const MAX_PROJECTION_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_PROJECTED_MISSIONS: usize = 500;
const MAX_MISSION_REQUIREMENTS: usize = 10_000;
const MAX_MISSION_UNITS: usize = 10_000;
const MAX_MISSION_FEEDBACK: usize = 10_000;
const MAX_MISSION_EVIDENCE: usize = 10_000;
const MAX_EVIDENCE_IDS: usize = 10_000;
const MAX_IDENTIFIER_BYTES: usize = 128;
const GENERATED_AT_SKEW_TOLERANCE_SECONDS: i64 = 120;
const CREDENTIAL_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "rk_live_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "akia",
    "asia",
    "xoxb-",
    "xoxa-",
    "xoxp-",
    "xoxr-",
    "xoxs-",
    "glpat-",
    "npm_",
    "pypi-",
    "aiza",
    "sq0atp-",
    "sq0csp-",
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionProjection {
    pub schema_version: String,
    pub generated_at: String,
    pub revision: u64,
    pub stale_after_seconds: u64,
    pub portfolio: MissionPortfolio,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionPortfolio {
    pub id: String,
    pub status: PortfolioStatus,
    pub missions: Vec<ProjectedMission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PortfolioStatus {
    Open,
    ClosedToNewWork,
    PortfolioComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedMission {
    pub id: String,
    #[serde(default)]
    pub mission_revision: Option<u64>,
    pub status: MissionStatus,
    pub contract_version: u64,
    pub attempt_id: String,
    pub source_revision: Option<String>,
    pub outcome_proven: bool,
    pub next_step: String,
    pub next_owner: String,
    pub updated_at: String,
    pub requirements: Vec<MissionRequirement>,
    pub units: Vec<MissionUnit>,
    pub decision: Option<MissionDecision>,
    pub approval: Option<MissionApproval>,
    pub feedback: Vec<MissionFeedback>,
    pub evidence: Vec<MissionEvidence>,
    pub reconciliation: Option<MissionReconciliation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MissionStatus {
    Draft,
    Queued,
    Running,
    NeedsDecision,
    NeedsSupport,
    ExternalBlock,
    Paused,
    OutcomeProven,
    ApprovalPending,
    MissionDone,
    Canceled,
}

impl MissionStatus {
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running)
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Queued => "Queued",
            Self::Running => "Active",
            Self::NeedsDecision => "Decision needed",
            Self::NeedsSupport => "Support needed",
            Self::ExternalBlock => "External block",
            Self::Paused => "Paused",
            Self::OutcomeProven => "Outcome proven",
            Self::ApprovalPending => "Approval pending",
            Self::MissionDone => "Mission done",
            Self::Canceled => "Canceled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionRequirement {
    pub id: String,
    pub evidence_kind: String,
    pub status: RequirementStatus,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RequirementStatus {
    Open,
    Proven,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionUnit {
    pub id: String,
    pub owner: String,
    pub status: UnitStatus,
    pub lease_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UnitStatus {
    Active,
    Held,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionDecision {
    pub id: String,
    pub version: u64,
    pub status: DecisionStatus,
    #[serde(default)]
    pub scope: Option<DecisionScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionScope {
    MissionContract,
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionApproval {
    pub id: String,
    pub status: ApprovalStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalStatus {
    Requested,
    Granted,
    Rejected,
    Executed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionFeedback {
    pub id: String,
    pub version: u64,
    pub status: FeedbackStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeedbackStatus {
    Queued,
    Acknowledged,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionEvidence {
    pub id: String,
    pub requirement_id: String,
    pub kind: String,
    pub artifact_digest: String,
    pub source_revision: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionReconciliation {
    pub id: String,
    pub status: ReconciliationStatus,
    pub mission_revision: u64,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReconciliationStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionFreshness {
    Fresh {
        age_seconds: u64,
    },
    Stale {
        age_seconds: u64,
        limit_seconds: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionProjectionLoad {
    Loading {
        path: PathBuf,
    },
    Missing {
        path: PathBuf,
    },
    Unavailable {
        path: PathBuf,
        message: String,
    },
    Malformed {
        path: PathBuf,
        message: String,
    },
    Unsupported {
        path: PathBuf,
        found: String,
    },
    Ready {
        path: PathBuf,
        projection: MissionProjection,
        freshness: ProjectionFreshness,
    },
}

impl MissionProjectionLoad {
    #[must_use]
    pub fn loading(path: PathBuf) -> Self {
        Self::Loading { path }
    }

    #[must_use]
    pub fn is_legacy_v1(&self) -> bool {
        matches!(
            self,
            Self::Ready { projection, .. }
                if projection.schema_version == MISSION_PROJECTION_SCHEMA
        )
    }

    #[must_use]
    pub fn is_v2(&self) -> bool {
        matches!(
            self,
            Self::Ready { projection, .. }
                if projection.schema_version == MISSION_PROJECTION_SCHEMA_V2
        )
    }

    #[must_use]
    pub fn controls_enabled(&self) -> bool {
        self.is_v2()
    }

    #[must_use]
    pub fn mission(&self, mission_id: &str) -> Option<&ProjectedMission> {
        let Self::Ready { projection, .. } = self else {
            return None;
        };
        projection
            .portfolio
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
    }
}

impl ProjectedMission {
    #[must_use]
    pub fn mission_revision(&self) -> Option<u64> {
        self.mission_revision
    }
}

#[derive(Deserialize)]
struct ProjectionEnvelope {
    schema_version: String,
}

#[must_use]
pub fn mission_projection_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(MISSION_PROJECTION_ENV) {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".xplan/mission-command-snapshot.json"))
}

#[must_use]
pub fn load_mission_projection(path: &Path) -> MissionProjectionLoad {
    load_mission_projection_at(path, Utc::now())
}

fn load_mission_projection_at(path: &Path, now: DateTime<Utc>) -> MissionProjectionLoad {
    let bytes = match read_projection_bytes(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return MissionProjectionLoad::Missing {
                path: path.to_path_buf(),
            };
        }
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            return malformed(path, error.to_string());
        }
        Err(error) => return unavailable(path, error.to_string()),
    };
    parse_projection(path, &bytes, now)
}

fn read_projection_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_PROJECTION_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PROJECTION_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "projection exceeds 4 MiB",
        ));
    }
    Ok(bytes)
}

fn parse_projection(path: &Path, bytes: &[u8], now: DateTime<Utc>) -> MissionProjectionLoad {
    let envelope: ProjectionEnvelope = match serde_json::from_slice(bytes) {
        Ok(envelope) => envelope,
        Err(error) => return malformed(path, error.to_string()),
    };
    if !matches!(
        envelope.schema_version.as_str(),
        MISSION_PROJECTION_SCHEMA | MISSION_PROJECTION_SCHEMA_V2
    ) {
        return MissionProjectionLoad::Unsupported {
            path: path.to_path_buf(),
            found: envelope.schema_version,
        };
    }
    let mut projection: MissionProjection = match serde_json::from_slice(bytes) {
        Ok(projection) => projection,
        Err(error) => return malformed(path, error.to_string()),
    };
    if projection.schema_version == MISSION_PROJECTION_SCHEMA {
        for mission in &mut projection.portfolio.missions {
            mission.mission_revision = None;
        }
    }
    finish_projection(path, projection, now)
}

fn finish_projection(
    path: &Path,
    projection: MissionProjection,
    now: DateTime<Utc>,
) -> MissionProjectionLoad {
    if let Err(message) = validate_projection(&projection) {
        return malformed(path, message);
    }
    let generated = match DateTime::parse_from_rfc3339(&projection.generated_at) {
        Ok(value) => value.with_timezone(&Utc),
        Err(error) => return malformed(path, format!("generated_at is not RFC 3339: {error}")),
    };
    let signed_age_seconds = now.signed_duration_since(generated).num_seconds();
    if signed_age_seconds < -GENERATED_AT_SKEW_TOLERANCE_SECONDS {
        return malformed(
            path,
            format!(
                "generated_at is {}s in the future (tolerance {}s)",
                -signed_age_seconds, GENERATED_AT_SKEW_TOLERANCE_SECONDS
            ),
        );
    }
    let age_seconds = signed_age_seconds.max(0) as u64;
    let freshness = freshness(age_seconds, projection.stale_after_seconds);
    MissionProjectionLoad::Ready {
        path: path.to_path_buf(),
        projection,
        freshness,
    }
}

fn validate_projection(projection: &MissionProjection) -> Result<(), String> {
    validate_projection_header(projection)?;
    validate_ids(
        "mission",
        "portfolio",
        projection
            .portfolio
            .missions
            .iter()
            .map(|item| item.id.as_str()),
        MAX_PROJECTED_MISSIONS,
    )?;
    for mission in projection
        .portfolio
        .missions
        .iter()
        .take(MAX_PROJECTED_MISSIONS)
    {
        validate_mission(
            mission,
            projection.schema_version == MISSION_PROJECTION_SCHEMA_V2,
        )?;
    }
    validate_portfolio_status(&projection.portfolio)
}

fn validate_projection_header(projection: &MissionProjection) -> Result<(), String> {
    if projection.stale_after_seconds == 0 {
        return Err("stale_after_seconds must be at least 1".to_string());
    }
    validate_identifier(&projection.portfolio.id, "portfolio.id", false)?;
    bounded_len(
        "projection missions",
        projection.portfolio.missions.len(),
        MAX_PROJECTED_MISSIONS,
    )?;
    let v2 = projection.schema_version == MISSION_PROJECTION_SCHEMA_V2;
    for mission in projection
        .portfolio
        .missions
        .iter()
        .take(MAX_PROJECTED_MISSIONS)
    {
        match (v2, mission.mission_revision) {
            (true, Some(revision)) if revision > 0 => {}
            (true, _) => return Err("projection v2 requires mission_revision".to_owned()),
            (false, None) => {}
            (false, Some(_)) => unreachable!("legacy revision is normalized before validation"),
        }
    }
    Ok(())
}

fn validate_portfolio_status(portfolio: &MissionPortfolio) -> Result<(), String> {
    if portfolio.status != PortfolioStatus::PortfolioComplete {
        return Ok(());
    }
    if portfolio.missions.is_empty() {
        return Err("PORTFOLIO_COMPLETE requires at least one mission".to_string());
    }
    if portfolio
        .missions
        .iter()
        .take(MAX_PROJECTED_MISSIONS)
        .any(|mission| {
            !matches!(
                mission.status,
                MissionStatus::MissionDone | MissionStatus::Canceled
            )
        })
    {
        return Err("PORTFOLIO_COMPLETE requires every mission terminal".to_string());
    }
    Ok(())
}

fn validate_mission(mission: &ProjectedMission, v2: bool) -> Result<(), String> {
    validate_mission_identity(mission)?;
    validate_mission_bounds(mission)?;
    validate_mission_ids(mission)?;
    validate_evidence(mission)?;
    validate_control_identifiers(mission)?;
    if v2
        && mission
            .decision
            .as_ref()
            .is_some_and(|item| item.scope.is_none())
    {
        return Err("projection v2 decision requires scope".to_owned());
    }
    validate_versions(mission)?;
    validate_mission_state(mission)
}

fn validate_mission_identity(mission: &ProjectedMission) -> Result<(), String> {
    validate_identifier(&mission.id, "mission.id", false)?;
    validate_identifier(&mission.attempt_id, "mission.attempt_id", false)?;
    validate_identifier(&mission.next_owner, "mission.next_owner", true)?;
    if mission.contract_version == 0 {
        return Err(format!(
            "mission {} has an invalid contract version",
            mission.id
        ));
    }
    if mission.next_step != expected_next_step(mission.status) {
        return Err(format!(
            "mission {} next_step contradicts its status",
            mission.id
        ));
    }
    validate_timestamp(&mission.updated_at, "mission.updated_at")?;
    if mission.requirements.is_empty() {
        return Err(format!(
            "mission {} requires at least one requirement",
            mission.id
        ));
    }
    Ok(())
}

fn expected_next_step(status: MissionStatus) -> &'static str {
    match status {
        MissionStatus::Queued => "Assign the next outcome-shaped unit",
        MissionStatus::Running => "Monitor active unit evidence",
        MissionStatus::NeedsDecision => "Resolve the versioned decision",
        MissionStatus::OutcomeProven => "Reconcile the mission contract and evidence",
        MissionStatus::ApprovalPending => "Grant or reject explicit completion approval",
        MissionStatus::MissionDone => "No further mission action",
        _ => "Inspect mission state",
    }
}

fn validate_mission_ids(mission: &ProjectedMission) -> Result<(), String> {
    validate_ids(
        "requirement",
        &mission.id,
        mission.requirements.iter().map(|item| item.id.as_str()),
        MAX_MISSION_REQUIREMENTS,
    )?;
    validate_ids(
        "unit",
        &mission.id,
        mission.units.iter().map(|item| item.id.as_str()),
        MAX_MISSION_UNITS,
    )?;
    validate_ids(
        "evidence",
        &mission.id,
        mission.evidence.iter().map(|item| item.id.as_str()),
        MAX_MISSION_EVIDENCE,
    )?;
    validate_ids(
        "feedback",
        &mission.id,
        mission.feedback.iter().map(|item| item.id.as_str()),
        MAX_MISSION_FEEDBACK,
    )?;
    validate_unit_owners(mission)?;
    Ok(())
}

fn validate_unit_owners(mission: &ProjectedMission) -> Result<(), String> {
    for unit in mission.units.iter().take(MAX_MISSION_UNITS) {
        validate_identifier(&unit.owner, "unit.owner", false)?;
    }
    Ok(())
}

fn validate_mission_bounds(mission: &ProjectedMission) -> Result<(), String> {
    for (label, len, limit) in [
        (
            "requirements",
            mission.requirements.len(),
            MAX_MISSION_REQUIREMENTS,
        ),
        ("units", mission.units.len(), MAX_MISSION_UNITS),
        ("feedback", mission.feedback.len(), MAX_MISSION_FEEDBACK),
        ("evidence", mission.evidence.len(), MAX_MISSION_EVIDENCE),
    ] {
        bounded_len(&format!("mission {} {label}", mission.id), len, limit)?;
    }
    Ok(())
}

fn bounded_len(label: &str, len: usize, limit: usize) -> Result<(), String> {
    if len > limit {
        return Err(format!("{label} exceeds {limit} entries"));
    }
    Ok(())
}

fn validate_identifier(value: &str, name: &str, allow_empty: bool) -> Result<(), String> {
    if allow_empty && value.is_empty() {
        return Ok(());
    }
    if !identifier_shape_is_safe(value) {
        return Err(format!("{name} must be a structural identifier"));
    }
    if identifier_resembles_secret(value) {
        return Err(format!("{name} resembles a credential-bearing identifier"));
    }
    Ok(())
}

fn identifier_shape_is_safe(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    value.len() <= MAX_IDENTIFIER_BYTES
        && first.is_ascii_alphanumeric()
        && bytes.all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'@' | b'-')
        })
}

fn identifier_resembles_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if credential_prefix_at_boundary(&lower) {
        return true;
    }
    let parts: Vec<_> = lower.split(['.', '_', ':', '@', '-']).collect();
    parts
        .iter()
        .any(|part| matches!(*part, "password" | "passwd" | "secret" | "token" | "apikey"))
        || parts.windows(2).any(|pair| pair == ["api", "key"])
}

fn credential_prefix_at_boundary(value: &str) -> bool {
    value.char_indices().any(|(index, _)| {
        let boundary = index == 0 || is_identifier_delimiter(value.as_bytes()[index - 1]);
        boundary
            && CREDENTIAL_PREFIXES
                .iter()
                .any(|prefix| value[index..].starts_with(prefix))
    })
}

fn is_identifier_delimiter(byte: u8) -> bool {
    matches!(byte, b'.' | b'_' | b':' | b'@' | b'-')
}

fn validate_timestamp(value: &str, name: &str) -> Result<(), String> {
    DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|error| format!("{name} is not RFC 3339: {error}"))
}

fn validate_ids<'a>(
    kind: &str,
    scope: &str,
    ids: impl Iterator<Item = &'a str>,
    limit: usize,
) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(limit.min(64));
    for id in ids.take(limit) {
        validate_identifier(id, &format!("{kind} id in {scope}"), false)?;
        if !seen.insert(id) {
            return Err(format!("duplicate {kind} id {id} in {scope}"));
        }
    }
    Ok(())
}

fn validate_evidence(mission: &ProjectedMission) -> Result<(), String> {
    let requirements: HashMap<_, _> = mission
        .requirements
        .iter()
        .take(MAX_MISSION_REQUIREMENTS)
        .map(|item| (item.id.as_str(), item))
        .collect();
    let evidence: HashMap<_, _> = mission
        .evidence
        .iter()
        .take(MAX_MISSION_EVIDENCE)
        .map(|item| (item.id.as_str(), item))
        .collect();
    validate_requirement_kinds(mission)?;
    for item in mission.evidence.iter().take(MAX_MISSION_EVIDENCE) {
        validate_evidence_record(item, &requirements, &mission.id)?;
    }
    validate_source_links(mission)?;
    for requirement in mission.requirements.iter().take(MAX_MISSION_REQUIREMENTS) {
        validate_requirement_evidence(requirement, &evidence, mission)?;
    }
    validate_all_evidence_linked(mission)?;
    validate_reconciliation_references(mission, &evidence)
}

fn validate_requirement_kinds(mission: &ProjectedMission) -> Result<(), String> {
    for requirement in mission.requirements.iter().take(MAX_MISSION_REQUIREMENTS) {
        validate_identifier(
            &requirement.evidence_kind,
            "requirement.evidence_kind",
            false,
        )?;
    }
    Ok(())
}

fn validate_source_links(mission: &ProjectedMission) -> Result<(), String> {
    if let Some(revision) = &mission.source_revision {
        if !valid_source_revision(revision) {
            return Err("mission.source_revision is not a full git commit".to_string());
        }
    } else if !mission.evidence.is_empty() {
        return Err("mission evidence lacks an adopted source revision".to_string());
    }
    if mission
        .evidence
        .iter()
        .take(MAX_MISSION_EVIDENCE)
        .any(|item| Some(&item.source_revision) != mission.source_revision.as_ref())
    {
        return Err("mission evidence source revision is inconsistent".to_string());
    }
    Ok(())
}

fn validate_all_evidence_linked(mission: &ProjectedMission) -> Result<(), String> {
    let linked = mission
        .requirements
        .iter()
        .take(MAX_MISSION_REQUIREMENTS)
        .map(|item| item.evidence_ids.len())
        .sum::<usize>();
    if linked != mission.evidence.len() {
        return Err("mission evidence and requirement links disagree".to_string());
    }
    Ok(())
}

fn validate_evidence_record(
    evidence: &MissionEvidence,
    requirements: &HashMap<&str, &MissionRequirement>,
    mission_id: &str,
) -> Result<(), String> {
    validate_identifier(&evidence.requirement_id, "evidence.requirement_id", false)?;
    validate_identifier(&evidence.kind, "evidence.kind", false)?;
    let Some(requirement) = requirements.get(evidence.requirement_id.as_str()) else {
        return Err(format!(
            "evidence {} references an unknown requirement in {mission_id}",
            evidence.id
        ));
    };
    if evidence.kind != requirement.evidence_kind {
        return Err(format!(
            "evidence {} kind does not match requirement",
            evidence.id
        ));
    }
    if !valid_artifact_digest(&evidence.artifact_digest)
        || !valid_source_revision(&evidence.source_revision)
    {
        return Err(format!("evidence {} has invalid provenance", evidence.id));
    }
    validate_timestamp(&evidence.recorded_at, "evidence.recorded_at")?;
    Ok(())
}

fn valid_artifact_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .take(64)
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_source_revision(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_requirement_evidence(
    requirement: &MissionRequirement,
    evidence_by_id: &HashMap<&str, &MissionEvidence>,
    mission: &ProjectedMission,
) -> Result<(), String> {
    validate_identifier(
        &requirement.evidence_kind,
        "requirement.evidence_kind",
        false,
    )?;
    bounded_len(
        &format!("requirement {} evidence_ids", requirement.id),
        requirement.evidence_ids.len(),
        MAX_EVIDENCE_IDS,
    )?;
    validate_ids(
        "requirement evidence",
        &requirement.id,
        requirement.evidence_ids.iter().map(String::as_str),
        MAX_EVIDENCE_IDS,
    )?;
    if requirement.status == RequirementStatus::Open {
        return no_claimed_evidence(requirement);
    }
    validate_proven_requirement(requirement, evidence_by_id, mission)
}

fn no_claimed_evidence(requirement: &MissionRequirement) -> Result<(), String> {
    if !requirement.evidence_ids.is_empty() {
        return Err(format!(
            "OPEN requirement {} must not claim evidence",
            requirement.id
        ));
    }
    Ok(())
}

fn validate_proven_requirement(
    requirement: &MissionRequirement,
    evidence_by_id: &HashMap<&str, &MissionEvidence>,
    mission: &ProjectedMission,
) -> Result<(), String> {
    if requirement.evidence_ids.is_empty() {
        return Err(format!(
            "PROVEN requirement {} has no evidence",
            requirement.id
        ));
    }
    validate_listed_evidence(requirement, evidence_by_id)?;
    let present = mission
        .evidence
        .iter()
        .take(MAX_MISSION_EVIDENCE)
        .filter(|item| item.requirement_id == requirement.id)
        .count();
    if present != requirement.evidence_ids.len() {
        return Err(format!(
            "PROVEN requirement {} does not list its exact evidence set",
            requirement.id
        ));
    }
    Ok(())
}

fn validate_listed_evidence(
    requirement: &MissionRequirement,
    evidence_by_id: &HashMap<&str, &MissionEvidence>,
) -> Result<(), String> {
    for id in requirement.evidence_ids.iter().take(MAX_EVIDENCE_IDS) {
        let Some(evidence) = evidence_by_id.get(id.as_str()) else {
            return Err(format!(
                "PROVEN requirement {} references missing evidence {id}",
                requirement.id
            ));
        };
        if evidence.requirement_id != requirement.id {
            return Err(format!("evidence {id} belongs to a different requirement"));
        }
    }
    Ok(())
}

fn validate_reconciliation_references(
    mission: &ProjectedMission,
    evidence_by_id: &HashMap<&str, &MissionEvidence>,
) -> Result<(), String> {
    let Some(reconciliation) = &mission.reconciliation else {
        return Ok(());
    };
    bounded_len(
        "reconciliation evidence_ids",
        reconciliation.evidence_ids.len(),
        MAX_EVIDENCE_IDS,
    )?;
    validate_ids(
        "reconciliation evidence",
        &mission.id,
        reconciliation.evidence_ids.iter().map(String::as_str),
        MAX_EVIDENCE_IDS,
    )?;
    for id in reconciliation.evidence_ids.iter().take(MAX_EVIDENCE_IDS) {
        if !evidence_by_id.contains_key(id.as_str()) {
            return Err(format!("reconciliation references missing evidence {id}"));
        }
    }
    require_exact_reconciliation_evidence(mission, reconciliation)
}

fn validate_versions(mission: &ProjectedMission) -> Result<(), String> {
    if mission
        .decision
        .as_ref()
        .is_some_and(|item| item.version == 0)
    {
        return Err("decision version must be at least 1".to_string());
    }
    if mission
        .feedback
        .iter()
        .take(MAX_MISSION_FEEDBACK)
        .any(|item| item.version == 0)
    {
        return Err("feedback version must be at least 1".to_string());
    }
    if mission
        .reconciliation
        .as_ref()
        .is_some_and(|item| item.mission_revision == 0)
    {
        return Err("reconciliation mission_revision must be at least 1".to_string());
    }
    Ok(())
}

fn validate_control_identifiers(mission: &ProjectedMission) -> Result<(), String> {
    if let Some(decision) = &mission.decision {
        validate_identifier(&decision.id, "decision.id", false)?;
    }
    if let Some(approval) = &mission.approval {
        validate_identifier(&approval.id, "approval.id", false)?;
    }
    if let Some(reconciliation) = &mission.reconciliation {
        validate_identifier(&reconciliation.id, "reconciliation.id", false)?;
    }
    Ok(())
}

fn validate_mission_state(mission: &ProjectedMission) -> Result<(), String> {
    if mission.outcome_proven
        && mission
            .requirements
            .iter()
            .take(MAX_MISSION_REQUIREMENTS)
            .any(|item| item.status != RequirementStatus::Proven)
    {
        return Err("outcome_proven mission has an open requirement".to_string());
    }
    let state = match mission.status {
        MissionStatus::Running => require_unit(mission, UnitStatus::Active, "RUNNING"),
        MissionStatus::NeedsDecision => validate_needs_decision(mission),
        MissionStatus::OutcomeProven => validate_outcome_proven(mission),
        MissionStatus::ApprovalPending => validate_approval_pending(mission),
        MissionStatus::MissionDone => validate_mission_done(mission),
        _ => Ok(()),
    };
    state.and_then(|()| validate_next_owner(mission))
}

fn require_unit(
    mission: &ProjectedMission,
    status: UnitStatus,
    lifecycle: &str,
) -> Result<(), String> {
    if mission
        .units
        .iter()
        .take(MAX_MISSION_UNITS)
        .any(|unit| unit.status == status)
    {
        return Ok(());
    }
    Err(format!("{lifecycle} requires a {status:?} unit"))
}

fn validate_needs_decision(mission: &ProjectedMission) -> Result<(), String> {
    if !mission
        .decision
        .as_ref()
        .is_some_and(|item| item.status == DecisionStatus::Open)
    {
        return Err("NEEDS_DECISION requires an open decision".to_string());
    }
    let held = mission
        .units
        .iter()
        .take(MAX_MISSION_UNITS)
        .filter(|unit| unit.status == UnitStatus::Held)
        .count();
    if held == 0 && mission.units.is_empty() {
        return Ok(());
    }
    if held != 1 {
        return Err("NEEDS_DECISION requires exactly one Held unit".to_string());
    }
    Ok(())
}

fn validate_outcome_proven(mission: &ProjectedMission) -> Result<(), String> {
    if !mission.outcome_proven {
        return Err(format!(
            "{} requires outcome_proven",
            mission.status.label()
        ));
    }
    if mission
        .requirements
        .iter()
        .take(MAX_MISSION_REQUIREMENTS)
        .any(|item| item.status != RequirementStatus::Proven)
    {
        return Err(format!(
            "{} requires every requirement proven",
            mission.status.label()
        ));
    }
    if mission.source_revision.is_none() {
        return Err(format!(
            "{} requires an adopted source revision",
            mission.status.label()
        ));
    }
    Ok(())
}

fn validate_next_owner(mission: &ProjectedMission) -> Result<(), String> {
    if mission.next_owner == expected_next_owner(mission) {
        return Ok(());
    }
    Err("mission next_owner contradicts its status".to_string())
}

fn expected_next_owner(mission: &ProjectedMission) -> &str {
    match mission.status {
        MissionStatus::MissionDone => "",
        MissionStatus::NeedsDecision | MissionStatus::ApprovalPending => "user",
        MissionStatus::Running => mission
            .units
            .iter()
            .find(|unit| unit.status == UnitStatus::Active)
            .map_or("", |unit| unit.owner.as_str()),
        _ => "orchestrator",
    }
}

fn validate_approval_pending(mission: &ProjectedMission) -> Result<(), String> {
    validate_outcome_proven(mission)?;
    require_all_units_released(mission, "APPROVAL_PENDING")?;
    passing_reconciliation(mission, "APPROVAL_PENDING")?;
    if !mission
        .approval
        .as_ref()
        .is_some_and(|item| item.status == ApprovalStatus::Requested)
    {
        return Err("APPROVAL_PENDING requires requested approval".to_string());
    }
    Ok(())
}

fn validate_mission_done(mission: &ProjectedMission) -> Result<(), String> {
    validate_outcome_proven(mission)?;
    require_all_units_released(mission, "MISSION_DONE")?;
    passing_reconciliation(mission, "MISSION_DONE")?;
    if mission
        .decision
        .as_ref()
        .is_some_and(|item| item.status == DecisionStatus::Open)
    {
        return Err("MISSION_DONE must not retain an open decision".to_string());
    }
    if let Some(approval) = mission.approval.as_ref() {
        if !matches!(
            approval.status,
            ApprovalStatus::Granted | ApprovalStatus::Executed
        ) {
            return Err("MISSION_DONE approval must be granted or executed".to_string());
        }
    }
    Ok(())
}

fn require_all_units_released(mission: &ProjectedMission, lifecycle: &str) -> Result<(), String> {
    if mission
        .units
        .iter()
        .take(MAX_MISSION_UNITS)
        .all(|unit| unit.status == UnitStatus::Released)
    {
        return Ok(());
    }
    Err(format!("{lifecycle} requires all units released"))
}

fn passing_reconciliation<'a>(
    mission: &'a ProjectedMission,
    lifecycle: &str,
) -> Result<&'a MissionReconciliation, String> {
    match mission.reconciliation.as_ref() {
        Some(item) if item.status == ReconciliationStatus::Pass => Ok(item),
        _ => Err(format!("{lifecycle} requires passing reconciliation")),
    }
}

fn require_exact_reconciliation_evidence(
    mission: &ProjectedMission,
    reconciliation: &MissionReconciliation,
) -> Result<(), String> {
    let current: HashSet<_> = mission
        .evidence
        .iter()
        .take(MAX_MISSION_EVIDENCE)
        .map(|item| item.id.as_str())
        .collect();
    let reconciled: HashSet<_> = reconciliation
        .evidence_ids
        .iter()
        .take(MAX_EVIDENCE_IDS)
        .map(String::as_str)
        .collect();
    if current != reconciled {
        return Err("reconciliation must cover exact current evidence".to_string());
    }
    Ok(())
}

fn freshness(age_seconds: u64, limit_seconds: u64) -> ProjectionFreshness {
    if age_seconds > limit_seconds {
        ProjectionFreshness::Stale {
            age_seconds,
            limit_seconds,
        }
    } else {
        ProjectionFreshness::Fresh { age_seconds }
    }
}

fn malformed(path: &Path, message: String) -> MissionProjectionLoad {
    MissionProjectionLoad::Malformed {
        path: path.to_path_buf(),
        message,
    }
}

fn unavailable(path: &Path, message: String) -> MissionProjectionLoad {
    MissionProjectionLoad::Unavailable {
        path: path.to_path_buf(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::{json, Map, Value};
    use std::fs;
    use tempfile::tempdir;

    fn credential_identifiers() -> Vec<String> {
        let alpha = "abcdefghijklmnop";
        let upper = "ABCDEFGHIJKLMNOP";
        let digits = "1234567890";
        [
            format!("mission-sk-{digits}-{alpha}"),
            format!("mission-sk_{digits}{alpha}"),
            format!("mission-rk_live_{digits}{alpha}"),
            format!("safe.ghp_{alpha}"),
            format!("mission-gho_{alpha}"),
            format!("mission-ghu_{alpha}"),
            format!("mission-ghs_{alpha}"),
            format!("mission-ghr_{alpha}"),
            format!("mission-github_pat_{alpha}"),
            format!("prefix-AKIA{upper}"),
            format!("prefix-ASIA{upper}"),
            format!("mission-xoxb-{digits}-{alpha}"),
            format!("mission-xoxa-{digits}-{alpha}"),
            format!("mission-xoxp-{digits}-{alpha}"),
            format!("mission-xoxr-{digits}-{alpha}"),
            format!("mission-xoxs-{digits}-{alpha}"),
            format!("nested-glpat-{alpha}"),
            format!("mission-npm_{alpha}"),
            format!("mission-pypi-{alpha}"),
            format!("prefix-AIza{upper}"),
            format!("mission-sq0atp-{alpha}"),
            format!("mission-sq0csp-{alpha}"),
        ]
        .into()
    }

    fn valid_projection_value() -> Value {
        json!({
            "schema_version": MISSION_PROJECTION_SCHEMA,
            "generated_at": "2026-08-31T12:00:00Z",
            "revision": 7,
            "stale_after_seconds": 60,
            "portfolio": {"id": "PORT-1", "status": "OPEN", "missions": [valid_mission_value()]}
        })
    }

    fn valid_mission_value() -> Value {
        json!({
            "id": "MIS-1", "status": "RUNNING", "contract_version": 1,
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
            "evidence": [first_evidence()],
            "reconciliation": {"id": "REC-1", "status": "FAIL", "mission_revision": 6,
                               "evidence_ids": ["EVD-1"]}
        })
    }

    fn first_evidence() -> Value {
        json!({
            "id": "EVD-1", "requirement_id": "REQ-1", "kind": "browser",
            "artifact_digest": "a".repeat(64), "source_revision": "b".repeat(40),
            "recorded_at": "2026-08-31T12:00:00Z"
        })
    }

    fn valid_projection() -> String {
        valid_projection_value().to_string()
    }

    fn at(minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, minute, second)
            .single()
            .expect("invariant: valid test timestamp")
    }

    #[test]
    fn mission_projection_accepts_strict_v1() {
        let path = Path::new("snapshot.json");
        let loaded = parse_projection(path, valid_projection().as_bytes(), at(0, 30));
        let MissionProjectionLoad::Ready {
            projection,
            freshness,
            ..
        } = loaded
        else {
            panic!("expected ready projection");
        };
        assert_eq!(projection.portfolio.missions.len(), 1);
        assert_eq!(freshness, ProjectionFreshness::Fresh { age_seconds: 30 });
    }

    #[test]
    fn mission_projection_allows_absent_optional_gate_objects() {
        let mut value = valid_projection_value();
        let mission = mission_mut(&mut value);
        mission.remove("decision");
        mission.remove("approval");
        mission.remove("reconciliation");
        assert_ready(value);
    }

    #[test]
    fn mission_projection_rejects_unknown_fields_at_every_object_boundary() {
        let pointers = [
            "",
            "/portfolio",
            "/portfolio/missions/0",
            "/portfolio/missions/0/requirements/0",
            "/portfolio/missions/0/units/0",
            "/portfolio/missions/0/decision",
            "/portfolio/missions/0/approval",
            "/portfolio/missions/0/feedback/0",
            "/portfolio/missions/0/evidence/0",
            "/portfolio/missions/0/reconciliation",
        ];
        for pointer in pointers.into_iter().take(10) {
            let mut value = valid_projection_value();
            value
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .expect("invariant: fixture object")
                .insert("future_field".to_string(), json!(true));
            assert_malformed(value, "unknown field");
        }
    }

    #[test]
    fn mission_projection_marks_old_snapshot_stale() {
        let loaded = parse_projection(
            Path::new("snapshot.json"),
            valid_projection().as_bytes(),
            at(2, 0),
        );
        assert!(matches!(
            loaded,
            MissionProjectionLoad::Ready {
                freshness: ProjectionFreshness::Stale {
                    age_seconds: 120,
                    limit_seconds: 60
                },
                ..
            }
        ));
    }

    #[test]
    fn mission_projection_rejects_generated_at_far_in_the_future() {
        let mut value = valid_projection_value();
        value["generated_at"] = json!("2026-08-31T12:05:00Z");
        let loaded = parse_projection(
            Path::new("snapshot.json"),
            value.to_string().as_bytes(),
            at(0, 0),
        );
        let MissionProjectionLoad::Malformed { message, .. } = loaded else {
            panic!("expected malformed future snapshot, got {loaded:?}");
        };
        assert!(message.contains("in the future"), "message: {message}");

        let mut tolerated = valid_projection_value();
        tolerated["generated_at"] = json!("2026-08-31T12:01:00Z");
        let loaded = parse_projection(
            Path::new("snapshot.json"),
            tolerated.to_string().as_bytes(),
            at(0, 0),
        );
        assert!(matches!(
            loaded,
            MissionProjectionLoad::Ready {
                freshness: ProjectionFreshness::Fresh { age_seconds: 0 },
                ..
            }
        ));
    }

    #[test]
    fn mission_projection_distinguishes_missing_malformed_and_unsupported() {
        let dir = tempdir().expect("invariant: tempdir");
        let missing = load_mission_projection_at(&dir.path().join("missing.json"), at(0, 0));
        assert!(matches!(missing, MissionProjectionLoad::Missing { .. }));

        let malformed_path = dir.path().join("malformed.json");
        fs::write(&malformed_path, b"{").expect("invariant: write fixture");
        let malformed = load_mission_projection_at(&malformed_path, at(0, 0));
        assert!(matches!(malformed, MissionProjectionLoad::Malformed { .. }));

        let unsupported =
            valid_projection().replace(MISSION_PROJECTION_SCHEMA, "xplan-mission-projection-v99");
        let unsupported =
            parse_projection(Path::new("snapshot.json"), unsupported.as_bytes(), at(0, 0));
        assert!(matches!(
            unsupported,
            MissionProjectionLoad::Unsupported { .. }
        ));
    }

    #[test]
    fn mission_projection_rejects_missing_required_fields() {
        let invalid = valid_projection().replace(",\"next_owner\":\"agent\"", "");
        let loaded = parse_projection(Path::new("snapshot.json"), invalid.as_bytes(), at(0, 0));
        assert!(matches!(loaded, MissionProjectionLoad::Malformed { .. }));
    }

    #[test]
    fn mission_projection_rejects_empty_domain_identities() {
        for pointer in [
            "/portfolio/missions/0/id",
            "/portfolio/missions/0/requirements/0/id",
            "/portfolio/missions/0/units/0/id",
            "/portfolio/missions/0/evidence/0/id",
        ] {
            let mut value = valid_projection_value();
            *value.pointer_mut(pointer).expect("invariant: identity") = json!("");
            assert_malformed(value, "structural identifier");
        }
    }

    #[test]
    fn mission_projection_rejects_removed_private_text_fields() {
        for (pointer, field) in [
            ("/portfolio/missions/0", "title"),
            ("/portfolio/missions/0", "outcome"),
            ("/portfolio/missions/0/requirements/0", "label"),
            ("/portfolio/missions/0/units/0", "write_lease"),
            ("/portfolio/missions/0/decision", "prompt"),
            ("/portfolio/missions/0/approval", "effect"),
            ("/portfolio/missions/0/feedback/0", "summary"),
            ("/portfolio/missions/0/evidence/0", "label"),
        ] {
            let mut value = valid_projection_value();
            value
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .expect("invariant: fixture object")
                .insert(field.to_string(), json!("private text"));
            assert_malformed(value, "unknown field");
        }
    }

    #[test]
    fn mission_projection_rejects_nonstructural_or_secret_shaped_identifiers() {
        for (pointer, replacement) in [
            ("/portfolio/missions/0/id", "mission with spaces"),
            (
                "/portfolio/missions/0/requirements/0/evidence_kind",
                "api_key",
            ),
            ("/portfolio/missions/0/next_owner", "owner/with/path"),
        ] {
            assert_field_malformed(pointer, json!(replacement), "identifier");
        }
    }

    #[test]
    fn mission_projection_rejects_credential_prefixes_at_any_delimiter_boundary() {
        for identifier in credential_identifiers() {
            assert_field_malformed(
                "/portfolio/missions/0/id",
                json!(identifier),
                "credential-bearing identifier",
            );
        }
        assert_field_ready("/portfolio/missions/0/id", json!("safe.identifier"));
    }

    #[test]
    fn mission_projection_rejects_status_inconsistent_next_step() {
        assert_field_malformed(
            "/portfolio/missions/0/next_step",
            json!("No further mission action"),
            "contradicts its status",
        );
        assert_field_malformed(
            "/portfolio/missions/0/next_owner",
            json!("orchestrator"),
            "next_owner contradicts",
        );
    }

    #[test]
    fn mission_projection_requires_one_adopted_source_for_all_evidence() {
        let mut missing = valid_projection_value();
        mission_mut(&mut missing).remove("source_revision");
        assert_malformed(missing, "lacks an adopted source revision");

        let mut mismatched = valid_projection_value();
        mismatched["portfolio"]["missions"][0]["source_revision"] = json!("f".repeat(40));
        assert_malformed(mismatched, "source revision is inconsistent");

        assert_field_malformed(
            "/portfolio/missions/0/source_revision",
            json!("abc123"),
            "not a full git commit",
        );

        let mut sha256 = valid_projection_value();
        sha256["portfolio"]["missions"][0]["source_revision"] = json!("c".repeat(64));
        sha256["portfolio"]["missions"][0]["evidence"][0]["source_revision"] =
            json!("c".repeat(64));
        assert_ready(sha256);
    }

    #[test]
    fn mission_projection_allows_no_adopted_source_before_evidence() {
        let mut value = valid_projection_value();
        mission_mut(&mut value).remove("source_revision");
        value["portfolio"]["missions"][0]["requirements"][0]["status"] = json!("OPEN");
        value["portfolio"]["missions"][0]["requirements"][0]["evidence_ids"] = json!([]);
        value["portfolio"]["missions"][0]["evidence"] = json!([]);
        value["portfolio"]["missions"][0]["reconciliation"] = Value::Null;
        assert_ready(value);
    }

    #[test]
    fn mission_projection_rejects_duplicate_mission_identities() {
        let mut duplicate_mission = valid_projection_value();
        let second = duplicate_mission["portfolio"]["missions"][0].clone();
        duplicate_mission["portfolio"]["missions"]
            .as_array_mut()
            .expect("invariant: missions array")
            .push(second);
        assert_malformed(duplicate_mission, "duplicate mission id");
    }

    #[test]
    fn mission_projection_rejects_duplicate_child_identities() {
        for pointer in [
            "/portfolio/missions/0/requirements",
            "/portfolio/missions/0/units",
            "/portfolio/missions/0/evidence",
        ] {
            let mut value = valid_projection_value();
            let duplicate = value
                .pointer(pointer)
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .cloned()
                .expect("invariant: populated identity array");
            value
                .pointer_mut(pointer)
                .and_then(Value::as_array_mut)
                .expect("invariant: identity array")
                .push(duplicate);
            assert_malformed(value, "duplicate");
        }
    }

    #[test]
    fn mission_projection_enforces_v1_collection_bounds() {
        let mut no_requirements = valid_projection_value();
        no_requirements["portfolio"]["missions"][0]["requirements"] = json!([]);
        assert_malformed(no_requirements, "at least one requirement");

        let mut too_many = valid_projection_value();
        let mission = too_many["portfolio"]["missions"][0].clone();
        too_many["portfolio"]["missions"] =
            Value::Array(std::iter::repeat_n(mission, MAX_PROJECTED_MISSIONS + 1).collect());
        assert_malformed(too_many, "exceeds 500 entries");
    }

    #[test]
    fn mission_projection_rejects_invalid_evidence_links_and_proven_sets() {
        for (pointer, replacement, message) in [
            (
                "/portfolio/missions/0/evidence/0/requirement_id",
                json!("MISSING"),
                "unknown requirement",
            ),
            (
                "/portfolio/missions/0/evidence/0/kind",
                json!("wrong-kind"),
                "kind does not match",
            ),
            (
                "/portfolio/missions/0/requirements/0/evidence_ids",
                json!([]),
                "has no evidence",
            ),
        ] {
            assert_field_malformed(pointer, replacement, message);
        }
    }

    #[test]
    fn mission_projection_rejects_invalid_evidence_provenance() {
        for (pointer, replacement) in [
            (
                "/portfolio/missions/0/evidence/0/artifact_digest",
                json!("not-a-digest"),
            ),
            (
                "/portfolio/missions/0/evidence/0/source_revision",
                json!("abc123"),
            ),
        ] {
            assert_field_malformed(pointer, replacement, "invalid provenance");
        }
    }

    #[test]
    fn mission_projection_rejects_inexact_or_unlinked_evidence() {
        let mut unlisted = valid_projection_value();
        unlisted["portfolio"]["missions"][0]["evidence"]
            .as_array_mut()
            .expect("invariant: evidence array")
            .push(json!({
                "id": "EVD-UNLISTED", "requirement_id": "REQ-1", "kind": "browser",
                "artifact_digest": "c".repeat(64), "source_revision": "b".repeat(40),
                "recorded_at": "2026-08-31T12:00:00Z"
            }));
        assert_malformed(unlisted, "exact evidence set");

        let mut open_claim = valid_projection_value();
        open_claim["portfolio"]["missions"][0]["requirements"][1]["evidence_ids"] =
            json!(["EVD-1"]);
        assert_malformed(open_claim, "must not claim evidence");

        let mut unlinked_open = valid_projection_value();
        unlinked_open["portfolio"]["missions"][0]["evidence"]
            .as_array_mut()
            .expect("invariant: evidence array")
            .push(json!({
                "id": "EVD-OPEN", "requirement_id": "REQ-2", "kind": "review",
                "artifact_digest": "d".repeat(64), "source_revision": "b".repeat(40),
                "recorded_at": "2026-08-31T12:00:00Z"
            }));
        assert_malformed(unlinked_open, "links disagree");
    }

    #[test]
    fn mission_projection_rejects_the_auditors_false_done_snapshot() {
        let mut value = valid_projection_value();
        set_mission_status(&mut value, "MISSION_DONE");
        let mission = mission_mut(&mut value);
        mission.insert("outcome_proven".to_string(), json!(false));
        mission.insert("decision".to_string(), Value::Null);
        mission.insert("approval".to_string(), Value::Null);
        mission.insert("reconciliation".to_string(), Value::Null);
        assert_malformed(value, "requires outcome_proven");
    }

    #[test]
    fn mission_projection_enforces_active_and_held_operational_states() {
        let mut running = valid_projection_value();
        running["portfolio"]["missions"][0]["units"] = json!([]);
        assert_malformed(running, "RUNNING requires");

        let mut decision = valid_projection_value();
        set_mission_status(&mut decision, "NEEDS_DECISION");
        assert_malformed(decision, "open decision");

        let mut held = valid_projection_value();
        set_mission_status(&mut held, "NEEDS_DECISION");
        held["portfolio"]["missions"][0]["decision"]["status"] = json!("OPEN");
        assert_malformed(held, "Held unit");

        let mut one_held = valid_projection_value();
        set_mission_status(&mut one_held, "NEEDS_DECISION");
        one_held["portfolio"]["missions"][0]["decision"]["status"] = json!("OPEN");
        one_held["portfolio"]["missions"][0]["units"][0]["status"] = json!("HELD");
        assert_ready(one_held);

        let mut two_held = valid_projection_value();
        set_mission_status(&mut two_held, "NEEDS_DECISION");
        two_held["portfolio"]["missions"][0]["decision"]["status"] = json!("OPEN");
        two_held["portfolio"]["missions"][0]["units"] = json!([
            {"id": "UNIT-1", "owner": "agent", "status": "HELD", "lease_count": 1},
            {"id": "UNIT-2", "owner": "agent", "status": "HELD", "lease_count": 1}
        ]);
        assert_malformed(two_held, "exactly one Held unit");
    }

    #[test]
    fn mission_projection_accepts_the_complete_lifecycle_matrix() {
        for status in [
            "DRAFT",
            "QUEUED",
            "RUNNING",
            "NEEDS_SUPPORT",
            "EXTERNAL_BLOCK",
            "PAUSED",
            "CANCELED",
        ] {
            let mut value = valid_projection_value();
            set_mission_status(&mut value, status);
            assert_ready(value);
        }
        assert_ready(lifecycle_projection("NEEDS_DECISION"));
        assert_ready(completion_projection("OUTCOME_PROVEN", "GRANTED"));
        assert_ready(completion_projection("APPROVAL_PENDING", "REQUESTED"));
        assert_ready(completion_projection("MISSION_DONE", "EXECUTED"));
    }

    #[test]
    fn mission_projection_enforces_outcome_proven_gates() {
        let mut misleading_flag = valid_projection_value();
        misleading_flag["portfolio"]["missions"][0]["outcome_proven"] = json!(true);
        assert_malformed(misleading_flag, "open requirement");

        let mut unproven_status = valid_projection_value();
        set_mission_status(&mut unproven_status, "OUTCOME_PROVEN");
        assert_malformed(unproven_status, "requires outcome_proven");

        let mut outcome = valid_projection_value();
        set_mission_status(&mut outcome, "OUTCOME_PROVEN");
        outcome["portfolio"]["missions"][0]["outcome_proven"] = json!(true);
        assert_malformed(outcome, "open requirement");
    }

    #[test]
    fn mission_projection_enforces_approval_pending_gates() {
        let approval_ready = completion_projection("APPROVAL_PENDING", "REQUESTED");
        assert_ready(approval_ready);

        let mut approval_active = completion_projection("APPROVAL_PENDING", "REQUESTED");
        approval_active["portfolio"]["missions"][0]["units"][0]["status"] = json!("ACTIVE");
        assert_malformed(approval_active, "all units released");

        let mut approval_unreconciled = completion_projection("APPROVAL_PENDING", "REQUESTED");
        approval_unreconciled["portfolio"]["missions"][0]["reconciliation"]["status"] =
            json!("FAIL");
        assert_malformed(approval_unreconciled, "passing reconciliation");

        let mut approval = completion_projection("APPROVAL_PENDING", "REQUESTED");
        approval["portfolio"]["missions"][0]["approval"]["status"] = json!("GRANTED");
        assert_malformed(approval, "requested approval");
    }

    #[test]
    fn mission_projection_enforces_mission_done_release_and_reconciliation() {
        let done_ready = completion_projection("MISSION_DONE", "GRANTED");
        assert_ready(done_ready);

        let mut done_without_approval = completion_projection("MISSION_DONE", "GRANTED");
        mission_mut(&mut done_without_approval).remove("approval");
        assert_ready(done_without_approval);

        let mut done_active = completion_projection("MISSION_DONE", "GRANTED");
        done_active["portfolio"]["missions"][0]["units"][0]["status"] = json!("ACTIVE");
        assert_malformed(done_active, "all units released");

        let mut done_unreconciled = completion_projection("MISSION_DONE", "GRANTED");
        done_unreconciled["portfolio"]["missions"][0]["reconciliation"]["status"] = json!("FAIL");
        assert_malformed(done_unreconciled, "passing reconciliation");

        let mut stale_reconciliation = completion_projection("MISSION_DONE", "GRANTED");
        stale_reconciliation["portfolio"]["missions"][0]["reconciliation"]["evidence_ids"] =
            json!(["EVD-1"]);
        assert_malformed(stale_reconciliation, "exact current evidence");
    }

    #[test]
    fn mission_projection_enforces_mission_done_decision_and_approval() {
        let mut done = completion_projection("MISSION_DONE", "GRANTED");
        done["portfolio"]["missions"][0]["decision"]["status"] = json!("OPEN");
        assert_malformed(done, "open decision");

        let mut requested = completion_projection("MISSION_DONE", "REQUESTED");
        requested["portfolio"]["missions"][0]["decision"]["status"] = json!("RESOLVED");
        assert_malformed(requested, "granted or executed");

        let rejected = completion_projection("MISSION_DONE", "REJECTED");
        assert_malformed(rejected, "granted or executed");
    }

    #[test]
    fn mission_projection_binds_every_reconciliation_to_current_evidence() {
        let mut value = valid_projection_value();
        value["portfolio"]["missions"][0]["reconciliation"]["evidence_ids"] = json!([]);
        assert_malformed(value, "exact current evidence");
    }

    #[test]
    fn mission_projection_prevents_empty_or_nonterminal_complete_portfolios() {
        let mut empty = valid_projection_value();
        empty["portfolio"]["status"] = json!("PORTFOLIO_COMPLETE");
        empty["portfolio"]["missions"] = json!([]);
        assert_malformed(empty, "at least one mission");

        let mut running = valid_projection_value();
        running["portfolio"]["status"] = json!("PORTFOLIO_COMPLETE");
        assert_malformed(running, "every mission terminal");

        let mut done = completion_projection("MISSION_DONE", "EXECUTED");
        done["portfolio"]["status"] = json!("PORTFOLIO_COMPLETE");
        assert_ready(done);

        let mut canceled = valid_projection_value();
        canceled["portfolio"]["status"] = json!("PORTFOLIO_COMPLETE");
        set_mission_status(&mut canceled, "CANCELED");
        canceled["portfolio"]["missions"][0]["units"][0]["status"] = json!("RELEASED");
        assert_ready(canceled);
    }

    #[test]
    fn mission_projection_caps_file_bytes_before_parsing() {
        let dir = tempdir().expect("invariant: tempdir");
        let path = dir.path().join("oversized.json");
        let bytes = vec![b' '; (MAX_PROJECTION_BYTES + 1) as usize];
        fs::write(&path, bytes).expect("invariant: write oversized fixture");
        let loaded = load_mission_projection_at(&path, at(0, 0));
        assert!(matches!(loaded, MissionProjectionLoad::Malformed { .. }));
    }

    fn completion_projection(status: &str, approval_status: &str) -> Value {
        let mut value = valid_projection_value();
        set_mission_status(&mut value, status);
        value["portfolio"]["missions"][0]["outcome_proven"] = json!(true);
        value["portfolio"]["missions"][0]["requirements"][1]["status"] = json!("PROVEN");
        value["portfolio"]["missions"][0]["requirements"][1]["evidence_ids"] = json!(["EVD-2"]);
        value["portfolio"]["missions"][0]["units"][0]["status"] = json!("RELEASED");
        value["portfolio"]["missions"][0]["approval"]["status"] = json!(approval_status);
        value["portfolio"]["missions"][0]["reconciliation"]["status"] = json!("PASS");
        value["portfolio"]["missions"][0]["reconciliation"]["evidence_ids"] =
            json!(["EVD-1", "EVD-2"]);
        value["portfolio"]["missions"][0]["evidence"]
            .as_array_mut()
            .expect("invariant: evidence array")
            .push(second_evidence());
        value
    }

    fn lifecycle_projection(status: &str) -> Value {
        let mut value = valid_projection_value();
        set_mission_status(&mut value, status);
        value["portfolio"]["missions"][0]["decision"]["status"] = json!("OPEN");
        value["portfolio"]["missions"][0]["units"][0]["status"] = json!("HELD");
        value
    }

    fn set_mission_status(value: &mut Value, status: &str) {
        value["portfolio"]["missions"][0]["status"] = json!(status);
        value["portfolio"]["missions"][0]["next_step"] = json!(match status {
            "QUEUED" => "Assign the next outcome-shaped unit",
            "RUNNING" => "Monitor active unit evidence",
            "NEEDS_DECISION" => "Resolve the versioned decision",
            "OUTCOME_PROVEN" => "Reconcile the mission contract and evidence",
            "APPROVAL_PENDING" => "Grant or reject explicit completion approval",
            "MISSION_DONE" => "No further mission action",
            _ => "Inspect mission state",
        });
        value["portfolio"]["missions"][0]["next_owner"] = json!(match status {
            "MISSION_DONE" => "",
            "NEEDS_DECISION" | "APPROVAL_PENDING" => "user",
            "RUNNING" => "agent",
            _ => "orchestrator",
        });
    }

    fn second_evidence() -> Value {
        json!({
            "id": "EVD-2", "requirement_id": "REQ-2", "kind": "review",
            "artifact_digest": "b".repeat(64), "source_revision": "b".repeat(40),
            "recorded_at": "2026-08-31T12:00:00Z"
        })
    }

    fn mission_mut(value: &mut Value) -> &mut Map<String, Value> {
        value["portfolio"]["missions"][0]
            .as_object_mut()
            .expect("invariant: mission object")
    }

    fn assert_malformed(value: Value, expected: &str) {
        let loaded = parse_projection(
            Path::new("snapshot.json"),
            value.to_string().as_bytes(),
            at(0, 0),
        );
        let MissionProjectionLoad::Malformed { message, .. } = loaded else {
            panic!("expected malformed projection, got {loaded:?}");
        };
        assert!(
            message.contains(expected),
            "{message:?} did not contain {expected:?}"
        );
    }

    fn assert_field_malformed(pointer: &str, replacement: Value, expected: &str) {
        let mut value = valid_projection_value();
        *value
            .pointer_mut(pointer)
            .expect("invariant: evidence field") = replacement;
        assert_malformed(value, expected);
    }

    fn assert_field_ready(pointer: &str, replacement: Value) {
        let mut value = valid_projection_value();
        *value
            .pointer_mut(pointer)
            .expect("invariant: fixture field") = replacement;
        assert_ready(value);
    }

    fn assert_ready(value: Value) {
        let loaded = parse_projection(
            Path::new("snapshot.json"),
            value.to_string().as_bytes(),
            at(0, 0),
        );
        assert!(
            matches!(loaded, MissionProjectionLoad::Ready { .. }),
            "{loaded:?}"
        );
    }
}
