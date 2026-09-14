//! What each running session reports about itself: model, context use,
//! cost, and when it last spoke (TASK-225, slice 2).
//!
//! Claude Code hands its status-line command a JSON payload on every
//! session update — model, context window, cost, session id, cwd — and
//! that payload is the only documented, push-style source for those
//! facts. `sb agent status` reads one payload on stdin and writes it here;
//! the Agents page and the GUI fleet read it back, joined on session id
//! with the session listing. Nothing here is polled from the CLI: a
//! session that never runs a status line simply has no record, and its
//! row shows nothing for these columns.
//!
//! ## Shape
//!
//! One JSON record per session under [`default_agent_status_dir`]
//! (`~/.switchbard/agent-status/<session_id>.json`;
//! `SWITCHBARD_AGENT_STATUS_DIR` overrides it, the way `SWITCHBARD_WORK_DIR`
//! does for claims). Machine-local runtime state, never repo state.
//!
//! ## Liveness and cleanup
//!
//! A record is stale when its pid (when the writer knew one) is gone, or
//! when it has not been touched for [`MAX_RECORD_AGE`]. [`load_agent_statuses`]
//! deletes stale records on the way past, the same pruning discipline
//! `work_sessions` uses, so a crashed session's numbers do not outlive it.

use crate::work_sessions::pid_alive;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A status-line payload is a few kilobytes; anything past this is not one.
pub const MAX_STATUS_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Records untouched for longer than this are pruned on read, whether or
/// not a pid was recorded: a week is far longer than any session lives.
pub const MAX_RECORD_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// One session's last self-report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStatus {
    pub session_id: String,
    /// The CLI's pid when the writer's environment carried `CLAUDE_PID`;
    /// liveness falls back to age alone without it.
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    /// The API model id (`claude-opus-5`).
    #[serde(default)]
    pub model_id: Option<String>,
    /// The short name the CLI shows (`Opus`).
    #[serde(default)]
    pub model_name: Option<String>,
    /// Context window used, 0-100, as the CLI reports it.
    #[serde(default)]
    pub context_used_percent: Option<f64>,
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
    /// The custom or generated session name, when the CLI had one.
    #[serde(default)]
    pub session_name: Option<String>,
    #[serde(default)]
    pub claude_version: Option<String>,
    /// Unix seconds when this record was written.
    pub updated_at_unix: u64,
}

impl AgentStatus {
    /// Seconds since the session last reported, given `now`.
    pub fn age(&self, now_unix: u64) -> Duration {
        Duration::from_secs(now_unix.saturating_sub(self.updated_at_unix))
    }

    /// Gone pid, or untouched past [`MAX_RECORD_AGE`].
    pub fn is_stale(&self, now_unix: u64) -> bool {
        self.pid.is_some_and(|pid| !pid_alive(pid)) || self.age(now_unix) > MAX_RECORD_AGE
    }
}

/// The documented status-line payload, only the keys this store keeps.
/// Every field defaults so a newer CLI adding or dropping keys parses.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StatusLinePayload {
    session_id: Option<String>,
    session_name: Option<String>,
    cwd: Option<PathBuf>,
    model: PayloadModel,
    workspace: PayloadWorkspace,
    version: Option<String>,
    cost: PayloadCost,
    context_window: PayloadContext,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PayloadModel {
    id: Option<String>,
    display_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PayloadWorkspace {
    current_dir: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PayloadCost {
    total_cost_usd: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PayloadContext {
    used_percentage: Option<f64>,
}

/// Parse one status-line payload into a record stamped `now_unix`. `pid`
/// is the writer's `CLAUDE_PID`, when it had one. Pure.
pub fn parse_status_line_payload(
    raw: &str,
    pid: Option<u32>,
    now_unix: u64,
) -> Result<AgentStatus> {
    if raw.len() > MAX_STATUS_PAYLOAD_BYTES {
        bail!(
            "status payload is {} bytes, over the {} byte bound",
            raw.len(),
            MAX_STATUS_PAYLOAD_BYTES
        );
    }
    let payload: StatusLinePayload =
        serde_json::from_str(raw).map_err(|e| anyhow!("status payload: {e}"))?;
    let session_id = payload
        .session_id
        .filter(|id| !id.trim().is_empty() && is_safe_file_stem(id))
        .ok_or_else(|| anyhow!("status payload carries no usable session_id"))?;
    Ok(AgentStatus {
        session_id,
        pid,
        cwd: payload.workspace.current_dir.or(payload.cwd),
        model_id: payload.model.id,
        model_name: payload.model.display_name,
        context_used_percent: payload.context_window.used_percentage,
        total_cost_usd: payload.cost.total_cost_usd,
        session_name: payload.session_name.filter(|n| !n.trim().is_empty()),
        claude_version: payload.version,
        updated_at_unix: now_unix,
    })
}

/// The session id becomes a file name; only the characters Claude Code
/// uses in one (UUID hex and hyphens, plus what a fork suffix might add)
/// are accepted, so a payload can never write outside the store.
pub(crate) fn is_safe_file_stem(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.starts_with('.')
}

/// `$SWITCHBARD_AGENT_STATUS_DIR`, else `~/.switchbard/agent-status`.
pub fn default_agent_status_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SWITCHBARD_AGENT_STATUS_DIR") {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|home| home.join(".switchbard").join("agent-status"))
}

fn record_path(dir: &Path, session_id: &str) -> Option<PathBuf> {
    is_safe_file_stem(session_id).then(|| dir.join(format!("{session_id}.json")))
}

/// Write `status` as the session's current record, atomically.
pub fn record_agent_status(dir: &Path, status: &AgentStatus) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = record_path(dir, &status.session_id)
        .ok_or_else(|| anyhow!("status carries an unsafe session_id"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(status)?)?;
    std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn read(path: &Path) -> Result<Option<AgentStatus>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

/// Every live record, keyed by session id. Stale records (see the module
/// doc) and unreadable files are deleted on the way past.
pub fn load_agent_statuses(dir: &Path, now_unix: u64) -> Result<HashMap<String, AgentStatus>> {
    let mut out = HashMap::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(error) => return Err(error).with_context(|| format!("reading {}", dir.display())),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        match read(&path)? {
            Some(status) if !status.is_stale(now_unix) => {
                out.insert(status.session_id.clone(), status);
            }
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    Ok(out)
}

pub fn load_agent_status(dir: &Path, session_id: &str) -> Result<Option<AgentStatus>> {
    let Some(path) = record_path(dir, session_id) else {
        return Ok(None);
    };
    read(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = r#"{
  "cwd": "/current/working/directory",
  "session_id": "abc123-def",
  "session_name": "my-session",
  "transcript_path": "/path/to/transcript.jsonl",
  "model": { "id": "claude-opus-5", "display_name": "Opus" },
  "workspace": { "current_dir": "/w/app", "project_dir": "/w/app", "added_dirs": [] },
  "version": "2.1.90",
  "output_style": { "name": "default" },
  "cost": { "total_cost_usd": 0.01234, "total_duration_ms": 45000 },
  "context_window": { "context_window_size": 200000, "used_percentage": 8.5, "future": {"x": 1} }
}"#;

    #[test]
    fn payload_parses_the_documented_shape_and_tolerates_extra_or_missing_keys() {
        let status = parse_status_line_payload(PAYLOAD, Some(42), 1_000).unwrap();
        assert_eq!(status.session_id, "abc123-def");
        assert_eq!(status.pid, Some(42));
        assert_eq!(status.cwd.as_deref(), Some(Path::new("/w/app")));
        assert_eq!(status.model_id.as_deref(), Some("claude-opus-5"));
        assert_eq!(status.model_name.as_deref(), Some("Opus"));
        assert_eq!(status.context_used_percent, Some(8.5));
        assert_eq!(status.total_cost_usd, Some(0.01234));
        assert_eq!(status.session_name.as_deref(), Some("my-session"));
        assert_eq!(status.claude_version.as_deref(), Some("2.1.90"));
        assert_eq!(status.updated_at_unix, 1_000);

        let minimal = parse_status_line_payload(r#"{"session_id":"s1"}"#, None, 5).unwrap();
        assert_eq!(minimal.model_name, None);
        assert_eq!(minimal.cwd, None);
        assert!(
            parse_status_line_payload("{}", None, 5).is_err(),
            "no session id"
        );
        assert!(parse_status_line_payload(r#"{"session_id":"../x"}"#, None, 5).is_err());
        assert!(parse_status_line_payload(r#"{"session_id":"a/b"}"#, None, 5).is_err());
        assert!(parse_status_line_payload("nope", None, 5).is_err());
        let oversize = format!(
            "{{\"session_id\":\"s\",\"pad\":\"{}\"}}",
            "p".repeat(MAX_STATUS_PAYLOAD_BYTES)
        );
        assert!(parse_status_line_payload(&oversize, None, 5).is_err());
    }

    #[test]
    fn records_round_trip_and_stale_ones_are_pruned_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let now = 10_000_000;
        let live = parse_status_line_payload(PAYLOAD, Some(std::process::id()), now).unwrap();
        record_agent_status(dir.path(), &live).unwrap();
        let mut dead = live.clone();
        dead.session_id = "dead-1".into();
        dead.pid = Some(u32::MAX - 1);
        record_agent_status(dir.path(), &dead).unwrap();
        let mut old = live.clone();
        old.session_id = "old-1".into();
        old.pid = None;
        old.updated_at_unix = now - MAX_RECORD_AGE.as_secs() - 1;
        record_agent_status(dir.path(), &old).unwrap();
        let mut pidless = live.clone();
        pidless.session_id = "pidless-1".into();
        pidless.pid = None;
        record_agent_status(dir.path(), &pidless).unwrap();
        std::fs::write(dir.path().join("garbage.json"), "not json").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();

        let loaded = load_agent_statuses(dir.path(), now).unwrap();
        let mut ids: Vec<&String> = loaded.keys().collect();
        ids.sort();
        assert_eq!(ids, vec!["abc123-def", "pidless-1"]);
        assert_eq!(loaded["abc123-def"], live);
        assert!(!dir.path().join("dead-1.json").exists(), "dead pid pruned");
        assert!(!dir.path().join("old-1.json").exists(), "too old pruned");
        assert!(
            !dir.path().join("garbage.json").exists(),
            "unreadable pruned"
        );
        assert!(
            dir.path().join("notes.txt").exists(),
            "non-records untouched"
        );
        assert_eq!(
            load_agent_status(dir.path(), "abc123-def")
                .unwrap()
                .as_ref(),
            Some(&live)
        );
        assert_eq!(load_agent_status(dir.path(), "missing").unwrap(), None);
        assert_eq!(
            load_agent_statuses(&dir.path().join("absent"), now)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(live.age(now + 90), Duration::from_secs(90));
    }
}
