//! Terminate one freshly authenticated agent PID, never a process group.
//!
//! Named threat: cached PID reuse or a forged frontend request must not signal
//! another process. Both public stages enforce the native identity and protect
//! this process's ancestor chain. Both platforms retain a final read-to-signal race.
use crate::{AgentProcessKind, AgentSession};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

mod native;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProcessIdentity {
    pub pid: u32,
    pub start_token: (u64, u64),
    pub started_unix: Option<u64>,
    pub kind: AgentProcessKind,
    pub cwd: PathBuf,
    pub executable: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedAgentTermination {
    identity: AgentProcessIdentity,
}

impl PreparedAgentTermination {
    pub fn identity(&self) -> &AgentProcessIdentity {
        &self.identity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTerminationOutcome {
    SignalSent,
    AlreadyGone,
}

/// Bounded native probe. No session registry, external command or network.
pub fn probe_agent_identity(pid: u32) -> Result<AgentProcessIdentity> {
    validate_pid(pid)?;
    let facts = native::probe(pid)?;
    let kind = native_agent_kind(&facts.executable)
        .context("PID is not an identifiable claude or codex executable")?;
    Ok(AgentProcessIdentity {
        pid,
        start_token: facts.start,
        started_unix: facts.started_unix,
        kind,
        cwd: facts.cwd,
        executable: facts.executable,
    })
}

fn native_agent_kind(executable: &std::path::Path) -> Option<AgentProcessKind> {
    let name = executable.file_name().and_then(|s| s.to_str());
    #[cfg(target_os = "linux")]
    let name = name.map(|s| s.strip_suffix(" (deleted)").unwrap_or(s));
    let kind = name
        .and_then(crate::agent_sessions::classify_command)
        .or_else(|| {
            (name == Some("claude.exe")
                && executable
                    .parent()
                    .is_some_and(|p| p.ends_with("node_modules/@anthropic-ai/claude-code/bin")))
            .then_some(AgentProcessKind::Claude)
        });
    kind
}

pub fn prepare_agent_termination(row: &AgentSession) -> Result<PreparedAgentTermination> {
    let identity = row
        .process_identity
        .as_ref()
        .context("Agent identity unavailable; refresh before retrying")?;
    if identity.pid != row.pid
        || identity.kind != row.kind
        || row.cwd.as_ref() != Some(&identity.cwd)
    {
        bail!("Agent row identity changed; refresh before retrying");
    }
    validate_target(identity)?;
    Ok(PreparedAgentTermination {
        identity: identity.clone(),
    })
}

pub fn terminate_agent(prepared: PreparedAgentTermination) -> Result<AgentTerminationOutcome> {
    validate_protected_pid(prepared.identity.pid)?;
    if let Err(error) = verify_identity(&prepared.identity) {
        if error.downcast_ref::<std::io::Error>().is_some_and(|e| {
            e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(libc::ESRCH)
        }) {
            return Ok(AgentTerminationOutcome::AlreadyGone);
        }
        return Err(error);
    }
    // SAFETY: validation proves 2 <= pid <= i32::MAX; this is strictly a
    // positive PID, never zero, a negative process-group ID or our ancestor.
    // No pointers are passed. The native syscall result is checked below.
    let result = unsafe { libc::kill(prepared.identity.pid as i32, libc::SIGTERM) };
    if result == 0 {
        return Ok(AgentTerminationOutcome::SignalSent);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(AgentTerminationOutcome::AlreadyGone);
    }
    Err(error).context("SIGTERM was not sent")
}

fn validate_pid(pid: u32) -> Result<()> {
    if !(2..=i32::MAX as u32).contains(&pid) {
        bail!("Invalid agent PID");
    }
    Ok(())
}

fn validate_target(identity: &AgentProcessIdentity) -> Result<()> {
    validate_protected_pid(identity.pid)?;
    verify_identity(identity)
}

fn validate_protected_pid(pid: u32) -> Result<()> {
    validate_pid(pid)?;
    protect_ancestors(pid)
}

fn verify_identity(identity: &AgentProcessIdentity) -> Result<()> {
    if probe_agent_identity(identity.pid)? != *identity {
        bail!("Agent process identity changed; nothing signalled");
    }
    Ok(())
}

fn protect_ancestors(target: u32) -> Result<()> {
    let mut current = std::process::id();
    let mut visited = std::collections::HashSet::new();
    for _ in 0..64 {
        if current == target {
            bail!("Refusing to signal this process or its ancestor");
        }
        if current <= 1 {
            return Ok(());
        }
        if !visited.insert(current) {
            bail!("Unverifiable ancestor cycle");
        }
        current = native::parent(current).context("Cannot verify protected ancestor chain")?;
    }
    bail!("Ancestor chain exceeds safety bound")
}
