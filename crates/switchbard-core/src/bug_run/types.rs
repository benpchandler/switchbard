//! Durable run facts. Task content remains owned by the backlog write layer.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MAX_TEXT: usize = 64 * 1024;
pub const MAX_MESSAGES: usize = 512;
pub const MAX_RUNS: usize = 1_000;
pub const MAX_EVIDENCE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOptions {
    pub codex_binary: String,
    pub gate_command: String,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            codex_binary: "codex".into(),
            gate_command: "mise run ci".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    AgentQueued,
    Running,
    AwaitingAnswer,
    ResumeQueued,
    AwaitingReview,
    PublishQueued,
    Publishing,
    PrOpen,
    Failed,
    Unknown,
}

impl RunState {
    pub fn needs_owner(self) -> bool {
        matches!(
            self,
            Self::AwaitingAnswer | Self::AwaitingReview | Self::Failed | Self::Unknown
        )
    }
    pub fn is_queued(self) -> bool {
        matches!(
            self,
            Self::AgentQueued | Self::ResumeQueued | Self::PublishQueued
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAction {
    StartAgent,
    ResumeAgent,
    Publish,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStorageIdentity {
    pub repository_id: String,
    pub record_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub text: String,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    AgentQuestion,
    AgentReview,
    OwnerReply,
    OwnerPublish,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worker {
    pub pid: u32,
    pub boot_time: Option<u64>,
    pub token: String,
    pub process_group: Option<u32>,
    pub command_marker: Option<String>,
    pub child_reaped: bool,
    pub start_permit: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub repo_root: PathBuf,
    pub repo_key: String,
    pub task_storage: Option<TaskStorageIdentity>,
    pub report_snapshot: String,
    pub options: RunOptions,
    pub state: RunState,
    pub revision: u64,
    pub thread_id: Option<String>,
    pub worktree: Option<PathBuf>,
    pub branch: Option<String>,
    pub review_head: Option<String>,
    pub review_remote: Option<String>,
    pub review_repository: Option<String>,
    pub draft: String,
    pub prompt: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
    pub error: Option<String>,
    pub pr_url: Option<String>,
    pub lease: Option<Worker>,
    pub retry_action: Option<RunAction>,
    pub messages: Vec<Message>,
    pub created_at_unix: u64,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone)]
pub struct Lease {
    pub run: Run,
    pub token: String,
    pub action: RunAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentOutcome {
    AwaitingAnswer {
        question: String,
    },
    AwaitingReview {
        summary: String,
        tests: Vec<String>,
        risks: Vec<String>,
    },
}
