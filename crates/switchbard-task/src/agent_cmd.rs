//! `sb agent`: what running agent sessions report about themselves
//! (TASK-225). The sink for Claude Code's status-line payload and the
//! listing that reads it back. Neither verb needs a Backlog repo: the
//! store is machine-local (`switchbard_core::agent_status`), and the
//! status line runs from whatever directory a session happens to be in.
//!
//! `status` is wired from the status-line script — one line, backgrounded,
//! so a slow write can never delay the status line itself:
//!
//! ```sh
//! printf '%s' "$input" | sb agent status >/dev/null 2>&1 &
//! ```
//!
//! It prints nothing on success (the payload is the record, and nobody
//! reads the script's stderr), and one line on stderr with exit 1 when
//! the payload is unusable.

use anyhow::{anyhow, Result};
use clap::Subcommand;
use std::io::Read;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AgentCmd {
    /// Record this session's status-line payload (JSON on stdin: model,
    /// context window, cost, session id). Identity: the payload's
    /// session_id; pid from CLAUDE_PID when set. Prints nothing
    Status,
    /// Every session with a live status record: one TSV row per session -
    /// session id, model, context used %, cost USD, seconds since last
    /// report, cwd
    List,
}

pub fn run_agent(cmd: &AgentCmd) -> Result<()> {
    let dir = status_dir()?;
    match cmd {
        AgentCmd::Status => {
            let mut raw = String::new();
            std::io::stdin()
                .take(switchbard_core::agent_status::MAX_STATUS_PAYLOAD_BYTES as u64 + 1)
                .read_to_string(&mut raw)?;
            let pid = std::env::var("CLAUDE_PID")
                .ok()
                .and_then(|p| p.parse().ok());
            let status = switchbard_core::parse_status_line_payload(
                &raw,
                pid,
                switchbard_core::dispatch_inspect::now_unix(),
            )?;
            switchbard_core::record_agent_status(&dir, &status)
        }
        AgentCmd::List => {
            let now = switchbard_core::dispatch_inspect::now_unix();
            let mut rows: Vec<_> = switchbard_core::load_agent_statuses(&dir, now)?
                .into_values()
                .collect();
            rows.sort_by_key(|status| std::cmp::Reverse(status.updated_at_unix));
            for status in rows {
                let age = status.age(now).as_secs();
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    status.session_id,
                    status.model_name.or(status.model_id).unwrap_or_default(),
                    status
                        .context_used_percent
                        .map(|p| format!("{p:.0}"))
                        .unwrap_or_default(),
                    status
                        .total_cost_usd
                        .map(|c| format!("{c:.4}"))
                        .unwrap_or_default(),
                    age,
                    status
                        .cwd
                        .map(|c| c.display().to_string())
                        .unwrap_or_default(),
                );
            }
            Ok(())
        }
    }
}

fn status_dir() -> Result<PathBuf> {
    switchbard_core::default_agent_status_dir()
        .ok_or_else(|| anyhow!("no home directory: set SWITCHBARD_AGENT_STATUS_DIR"))
}
