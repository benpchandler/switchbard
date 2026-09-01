//! Strict data boundary for xplan's bundled one-shot Mission helper.
//!
//! The UI never builds JSON directly. It submits typed intent through this
//! module, while xplan remains the only process that writes mission truth.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MISSION_SIDECAR_PROTOCOL: &str = "xplan-mission-sidecar-v1";
pub const MAX_MISSION_REQUEST_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionCommand {
    Hello,
    QueueMission,
    GetPendingDecision,
    ResumeDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub command_id: String,
    pub command: MissionCommand,
    pub payload: Value,
}

impl MissionRequest {
    #[must_use]
    pub fn new(command: MissionCommand, command_id: String, payload: Value) -> Self {
        Self {
            protocol_version: MISSION_SIDECAR_PROTOCOL.to_owned(),
            request_id: format!("request-{command_id}"),
            command_id,
            command,
            payload,
        }
    }

    #[must_use]
    pub fn retry_with_request_id(&self, request_id: String) -> Self {
        let mut request = self.clone();
        request.request_id = request_id;
        request
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueueRequirement {
    pub requirement_id: String,
    pub label: String,
    pub evidence_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractReview {
    pub decision_id: String,
    pub version: u64,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueueMissionPayload {
    pub mission_id: String,
    pub title: String,
    pub outcome: String,
    pub requirements: Vec<QueueRequirement>,
    pub approval_required: bool,
    pub contract_review: ContractReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PendingDecisionPayload {
    pub mission_id: String,
    pub mission_revision: u64,
    pub decision_id: String,
    pub decision_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeDecisionPayload {
    pub mission_id: String,
    pub mission_revision: u64,
    pub decision_id: String,
    pub decision_version: u64,
    pub answer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionResponse {
    protocol_version: String,
    request_id: String,
    command_id: String,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
}

impl MissionResponse {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let response: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if response.result.is_some() == response.error.is_some() {
            return Err("response must contain exactly one of result or error".to_owned());
        }
        if let Some(error) = &response.error {
            let valid = error
                .as_object()
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str)
                .is_some();
            if !valid {
                return Err("response error must contain a string code".to_owned());
            }
        }
        Ok(response)
    }

    pub fn validate_identity(&self, request: &MissionRequest) -> Result<(), String> {
        if self.protocol_version != MISSION_SIDECAR_PROTOCOL {
            return Err("response protocol identity mismatch".to_owned());
        }
        if self.request_id != request.request_id {
            return Err("response request identity mismatch".to_owned());
        }
        if self.command_id != request.command_id {
            return Err("response command identity mismatch".to_owned());
        }
        Ok(())
    }

    #[must_use]
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    #[must_use]
    pub fn command_id(&self) -> &str {
        &self.command_id
    }

    #[must_use]
    pub fn result(&self) -> &Value {
        self.result.as_ref().unwrap_or(&Value::Null)
    }

    #[must_use]
    pub fn remote_error_code(&self) -> Option<&str> {
        self.error.as_ref()?.as_object()?.get("code")?.as_str()
    }
}

pub fn payload<T: Serialize>(value: &T) -> Result<Value, serde_json::Error> {
    serde_json::to_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn response_boundary_is_strict_and_identity_bound() {
        let request = MissionRequest::new(MissionCommand::Hello, "fixture:hello".into(), json!({}));
        let response = MissionResponse::decode(
            br#"{"protocol_version":"xplan-mission-sidecar-v1","request_id":"request-fixture:hello","command_id":"fixture:hello","result":{}}"#,
        )
        .unwrap();
        assert!(response.validate_identity(&request).is_ok());
        assert!(MissionResponse::decode(
            br#"{"protocol_version":"xplan-mission-sidecar-v1","request_id":"request-fixture:hello","command_id":"fixture:hello","result":{},"extra":true}"#,
        )
        .is_err());
    }
}
