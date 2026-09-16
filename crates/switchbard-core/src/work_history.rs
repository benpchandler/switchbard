//! Append-only history of work claims: when each claim started and how it ended.
//!
//! [`crate::work_sessions`] holds only live claims and deletes them when work
//! ends, so without this log nothing records how long a task was worked. Each
//! claim start and end appends one JSON line to `history.jsonl` in the work
//! directory. Wall-clock time for a task is the sum of its claim intervals,
//! which also covers work handed between sessions. The log is machine-local
//! runtime state, like the live records, and is never rewritten.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// File name of the log inside the work directory. Its extension keeps it out
/// of the live-record scan, which reads only `*.json`.
pub const WORK_HISTORY_FILE: &str = "history.jsonl";

/// Why a claim started or stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkEventKind {
    Claimed,
    Released,
    /// A human passed the task, releasing every session that held it.
    Passed,
    /// The Stop hook gave up holding the session.
    Abandoned,
    /// The session ended with the claim still held.
    Ended,
    /// The claiming process was found dead.
    Pruned,
    /// The session started claiming in a different repo.
    Replaced,
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkEvent {
    /// RFC 3339, local time.
    pub at: String,
    pub event: WorkEventKind,
    pub task_id: String,
    pub session_id: String,
    pub agent: String,
    pub repo_root: PathBuf,
}

/// Appends `events` to the log in `dir`, one line each.
pub fn append_work_events(dir: &Path, events: &[WorkEvent]) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    let mut text = String::new();
    for event in events {
        text.push_str(&serde_json::to_string(event)?);
        text.push('\n');
    }
    debug_assert!(text.ends_with('\n'), "invariant: every event ends a line");
    let path = dir.join(WORK_HISTORY_FILE);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .with_context(|| format!("appending to {}", path.display()))
}

/// Every event in `dir`'s log, oldest first. Unreadable lines are skipped so
/// one damaged line never hides the rest.
pub fn read_work_history(dir: &Path) -> Result<Vec<WorkEvent>> {
    let path = dir.join(WORK_HISTORY_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    Ok(text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}
