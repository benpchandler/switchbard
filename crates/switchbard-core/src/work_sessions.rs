//! Which tasks a live agent session is working right now (TASK-150).
//!
//! A task's `In Progress` status says someone once started it; the board in
//! the owner's screenshot carries forty of those. This store answers the
//! narrower live question - *is a session working it at this moment* - so
//! `sbt` can mark those rows and the agent harness can enforce the loop:
//! claim before editing, keep the claim until the work is released.
//!
//! ## Shape
//!
//! One JSON record per session under [`default_work_dir`]
//! (`~/.switchbard/work/<session_id>.json`, `SWITCHBARD_WORK_DIR` overrides
//! it, the way `XPLAN_MISSION_SNAPSHOT` does for the mission projection). It
//! is machine-local runtime state, not repo state: it never lands in the
//! repo, so a session on another machine is simply not visible here.
//!
//! ## Liveness is the process, not a heartbeat
//!
//! A claim is live while the agent process that made it is alive
//! (`kill(pid, 0)`). A crashed or killed session therefore drops off the
//! board on the next read with no human step, and a long think with no tool
//! calls never reads as dead. Records whose process is gone are pruned by
//! [`list_work_sessions`]; the tasks themselves keep whatever status the
//! session left them in - the claim is the only thing that dies with the
//! process.
//!
//! ## Who writes
//!
//! Only this module writes the directory. `sb work` and `sbt`'s pass key both
//! call it; the harness hook (`sb work hook`) reads the session's record and
//! records its stop decisions here. Task-side effects of a claim (status,
//! ball) go through the backlog mutation layer as usual - this store never
//! touches task files.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One agent session and the tasks it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSession {
    pub session_id: String,
    /// The CLI that owns the session (`claude`, `codex`); free text, for display.
    pub agent: String,
    pub pid: u32,
    /// The backlog repo the claims belong to; task ids are repo-local.
    pub repo_root: PathBuf,
    pub claims: Vec<WorkClaim>,
    /// RFC 3339, local time.
    pub started_at: String,
    /// How many times the Stop hook has held this session in place. Bounded by
    /// the hook's cap so a session that cannot finish is let go rather than
    /// looped forever; see `abandoned`.
    #[serde(default)]
    pub stop_blocks: u32,
    /// The Stop hook gave up holding the session: its claims are no longer
    /// live work, but the record stays so a reader can see what was left.
    #[serde(default)]
    pub abandoned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkClaim {
    pub task_id: String,
    /// RFC 3339, local time.
    pub claimed_at: String,
}

/// Who is claiming: read from the harness environment or given explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkIdentity {
    pub session_id: String,
    pub pid: u32,
    pub agent: String,
}

impl WorkIdentity {
    /// The identity Claude Code exports into every shell it runs:
    /// `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID`. `None` outside a harness.
    pub fn from_env() -> Option<WorkIdentity> {
        let session_id = std::env::var("CLAUDE_CODE_SESSION_ID").ok()?;
        let pid = std::env::var("CLAUDE_PID").ok()?.parse().ok()?;
        Some(WorkIdentity {
            session_id,
            pid,
            agent: "claude".to_string(),
        })
    }
}

impl WorkSession {
    pub fn is_live(&self) -> bool {
        !self.abandoned && pid_alive(self.pid)
    }

    pub fn holds(&self, task_id: &str) -> bool {
        self.claims.iter().any(|claim| claim.task_id == task_id)
    }

    pub fn claims_in(&self, repo_root: &Path) -> bool {
        same_repo(&self.repo_root, repo_root)
    }

    /// The first eight characters: what a row or a status line prints.
    pub fn short_id(&self) -> &str {
        let end = self
            .session_id
            .char_indices()
            .nth(8)
            .map(|(index, _)| index)
            .unwrap_or(self.session_id.len());
        &self.session_id[..end]
    }
}

/// `$SWITCHBARD_WORK_DIR`, else `~/.switchbard/work`.
pub fn default_work_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SWITCHBARD_WORK_DIR") {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|home| home.join(".switchbard").join("work"))
}

/// Record that `identity` is working `task_id` in `repo_root`. Idempotent for
/// a repeated claim of the same task; a session may hold several tasks. A
/// session record that belongs to another repo is replaced: one session works
/// one repo at a time.
pub fn claim_work(
    dir: &Path,
    identity: &WorkIdentity,
    repo_root: &Path,
    task_id: &str,
) -> Result<WorkSession> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating work dir {}", dir.display()))?;
    let mut session = load_work_session(dir, &identity.session_id)?
        .filter(|session| session.claims_in(repo_root) && !session.abandoned)
        .unwrap_or_else(|| WorkSession {
            session_id: identity.session_id.clone(),
            agent: identity.agent.clone(),
            pid: identity.pid,
            repo_root: repo_root.to_path_buf(),
            claims: Vec::new(),
            started_at: now(),
            stop_blocks: 0,
            abandoned: false,
        });
    session.pid = identity.pid;
    if !session.holds(task_id) {
        session.claims.push(WorkClaim {
            task_id: task_id.to_string(),
            claimed_at: now(),
        });
    }
    save(dir, &session)?;
    Ok(session)
}

/// Drop `task_id` from `session_id`'s claims. Errors when the session does
/// not hold it, so a release never silently succeeds against the wrong task.
pub fn release_work(dir: &Path, session_id: &str, task_id: &str) -> Result<WorkSession> {
    let mut session = load_work_session(dir, session_id)?
        .ok_or_else(|| anyhow!("session {session_id} holds no work"))?;
    if !session.holds(task_id) {
        bail!(
            "session {} does not hold {task_id} (it holds: {})",
            session.short_id(),
            held_ids(&session)
        );
    }
    session.claims.retain(|claim| claim.task_id != task_id);
    save(dir, &session)?;
    Ok(session)
}

/// A human passes the task: every session holding it in `repo_root` lets go.
/// Returns the sessions that held it (empty when nobody did).
pub fn pass_work(dir: &Path, repo_root: &Path, task_id: &str) -> Result<Vec<WorkSession>> {
    let mut released = Vec::new();
    for mut session in list_work_sessions(dir, repo_root)? {
        if session.holds(task_id) {
            session.claims.retain(|claim| claim.task_id != task_id);
            save(dir, &session)?;
            released.push(session);
        }
    }
    Ok(released)
}

/// Every session record for `repo_root` whose process is still alive, oldest
/// first. Records of dead processes are deleted on the way past: a crashed
/// session's claims are not live work, and nobody else will clean them up.
pub fn list_work_sessions(dir: &Path, repo_root: &Path) -> Result<Vec<WorkSession>> {
    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(sessions),
        Err(error) => return Err(error).with_context(|| format!("reading {}", dir.display())),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(session) = read(&path)? else {
            continue;
        };
        if !pid_alive(session.pid) {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        if session.claims_in(repo_root) {
            sessions.push(session);
        }
    }
    sessions.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    Ok(sessions)
}

/// The live sessions holding `task_id` in `repo_root`.
pub fn sessions_working<'a>(sessions: &'a [WorkSession], task_id: &str) -> Vec<&'a WorkSession> {
    sessions
        .iter()
        .filter(|session| !session.abandoned && session.holds(task_id))
        .collect()
}

pub fn load_work_session(dir: &Path, session_id: &str) -> Result<Option<WorkSession>> {
    read(&record_path(dir, session_id))
}

/// The Stop hook held the session in place once more; returns the new count.
pub fn record_stop_block(dir: &Path, session_id: &str) -> Result<u32> {
    let mut session = load_work_session(dir, session_id)?
        .ok_or_else(|| anyhow!("session {session_id} holds no work"))?;
    session.stop_blocks += 1;
    save(dir, &session)?;
    Ok(session.stop_blocks)
}

/// The Stop hook let the session go with claims still held.
pub fn abandon_work_session(dir: &Path, session_id: &str) -> Result<()> {
    if let Some(mut session) = load_work_session(dir, session_id)? {
        session.abandoned = true;
        save(dir, &session)?;
    }
    Ok(())
}

/// The session ended: its record is gone, claims and all.
pub fn end_work_session(dir: &Path, session_id: &str) -> Result<()> {
    match std::fs::remove_file(record_path(dir, session_id)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// `kill(pid, 0)`: alive when the signal is deliverable. `EPERM` means the
/// process exists but belongs to someone else, which for this purpose is alive.
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

pub fn held_ids(session: &WorkSession) -> String {
    if session.claims.is_empty() {
        return "nothing".to_string();
    }
    session
        .claims
        .iter()
        .map(|claim| claim.task_id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn same_repo(a: &Path, b: &Path) -> bool {
    let canon = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canon(a) == canon(b)
}

fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn record_path(dir: &Path, session_id: &str) -> PathBuf {
    let safe: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{safe}.json"))
}

fn read(path: &Path) -> Result<Option<WorkSession>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text)
            .with_context(|| format!("parsing work record {}", path.display()))
            .ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

fn save(dir: &Path, session: &WorkSession) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = record_path(dir, &session.session_id);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(session)?)?;
    std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> WorkIdentity {
        WorkIdentity {
            session_id: "abcdef12-3456".to_string(),
            pid: std::process::id(),
            agent: "claude".to_string(),
        }
    }

    #[test]
    fn claim_release_and_list_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let session = claim_work(dir.path(), &me(), &repo, "TASK-1").unwrap();
        assert!(session.holds("TASK-1"));
        claim_work(dir.path(), &me(), &repo, "TASK-2").unwrap();
        claim_work(dir.path(), &me(), &repo, "TASK-1").unwrap();
        let listed = list_work_sessions(dir.path(), &repo).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].claims.len(), 2, "a repeated claim is idempotent");
        assert_eq!(sessions_working(&listed, "TASK-2").len(), 1);
        release_work(dir.path(), "abcdef12-3456", "TASK-1").unwrap();
        assert!(release_work(dir.path(), "abcdef12-3456", "TASK-1").is_err());
        let other_repo = dir.path().join("other");
        std::fs::create_dir_all(&other_repo).unwrap();
        assert!(list_work_sessions(dir.path(), &other_repo)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_dead_process_is_pruned_and_a_pass_releases_every_holder() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let dead = WorkIdentity {
            session_id: "dead".to_string(),
            pid: u32::MAX - 1,
            agent: "claude".to_string(),
        };
        claim_work(dir.path(), &dead, &repo, "TASK-1").unwrap();
        claim_work(dir.path(), &me(), &repo, "TASK-1").unwrap();
        let listed = list_work_sessions(dir.path(), &repo).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(load_work_session(dir.path(), "dead").unwrap().is_none());
        let released = pass_work(dir.path(), &repo, "TASK-1").unwrap();
        assert_eq!(released.len(), 1);
        assert!(!load_work_session(dir.path(), "abcdef12-3456")
            .unwrap()
            .unwrap()
            .holds("TASK-1"));
    }

    #[test]
    fn stop_blocks_count_and_abandon_ends_liveness() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        claim_work(dir.path(), &me(), &repo, "TASK-1").unwrap();
        assert_eq!(record_stop_block(dir.path(), "abcdef12-3456").unwrap(), 1);
        assert_eq!(record_stop_block(dir.path(), "abcdef12-3456").unwrap(), 2);
        abandon_work_session(dir.path(), "abcdef12-3456").unwrap();
        let listed = list_work_sessions(dir.path(), &repo).unwrap();
        assert!(!listed[0].is_live());
        assert!(sessions_working(&listed, "TASK-1").is_empty());
        end_work_session(dir.path(), "abcdef12-3456").unwrap();
        assert!(list_work_sessions(dir.path(), &repo).unwrap().is_empty());
    }
}
