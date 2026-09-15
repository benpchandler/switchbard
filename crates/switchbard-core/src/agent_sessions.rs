//! Interactive agent CLI sessions — the second half of the Command place's
//! fleet (TASK-98, trajectory: *Information architecture V2*). A dispatch
//! run is a task-labeled headless `claude -p` process this app itself
//! spawned (`switchbard_core::dispatch_inspect` already knows everything
//! about it). An *interactive* session is a `claude`/`codex` CLI a human
//! started by hand in a terminal, in some worktree — this app never spawned
//! it and holds no state about it, so the only way to know it exists at all
//! is to scan the OS for it, the same way `scanner::scan_listeners` scans
//! for listening ports.
//!
//! ## Split, like `scanner`: parse layer pure, OS walk behind a `cfg`
//!
//! [`parse_ps_agent_rows`] (macOS/BSD `ps` text) and the Linux `/proc` walk
//! below are both pure enough to unit-test against fixture text/rows without
//! actually spawning a process — see this module's tests. [`scan_agent_sessions`]
//! is the only impure entry point, cfg-gated exactly like `scanner::
//! scan_listeners`.
//!
//! ## Two sources, one row per pid
//!
//! The OS scan proves a process exists and yields its pid, cwd, pgid and
//! start time — nothing about what the session is doing. Claude Code keeps
//! its own registry of running sessions and exposes it for scripting as
//! `claude agents --json` (documented: session id, name, cwd, `idle`/`busy`).
//! [`scan_agent_sessions`] runs both and [`merge_agent_rows`] joins them on
//! pid: a listed session takes the scan's pgid (the dispatch dedup key) and
//! keeps its own state and name; a scanned process the listing does not
//! know — a `codex`, a headless `claude -p`, an older CLI — is kept with
//! [`AgentActivity::Unknown`]. A listing failure (no `claude` on `PATH`,
//! an older version without the flag) degrades to the scan alone and is
//! reported in [`AgentScan::listing_error`] rather than hidden.
//!
//! Listed sessions with no pid — Claude's own background sessions — are
//! skipped: this module vouches only for processes it can see (named gap,
//! TASK-225). Every listed pid is also checked with `kill(pid, 0)` so a
//! registry entry left behind by a crash never renders as a live session.
//!
//! ## What counts as "an agent session" in the OS scan
//!
//! A process whose own command name (`ps -o comm=` / `/proc/<pid>/comm`) is
//! **exactly** `claude` or `codex` — see [`classify_command`]. This is a
//! deliberately narrow, honest boundary, not a guess: an install that runs
//! as a wrapping interpreter (`node`, a shim script) is invisible to it.
//! Widening the match to argv content would risk false positives for a
//! feature whose only job is to tell the truth about what is running; the
//! Claude listing above is the path that catches such installs instead.
//!
//! One narrowing on top of the name match: a matched process that is a
//! *descendant of another matched process* is that session's own helper,
//! not a second session, and [`drop_agent_descendants`] removes it. The
//! case that motivated it: Codex's computer-use plugin spawns a `codex`
//! binary bundled inside ChatGPT.app under the interactive `codex` TUI,
//! and the fleet counted the helper as a session in the same worktree.
//! The walk is by parent pid, bounded by [`MAX_ANCESTOR_DEPTH`], and a
//! process whose parent is unknown is kept: the rule only ever removes a
//! row it can prove is a child.
//!
//! ## Read-only and bounded
//!
//! This module only ever reads process tables (`ps`) or `/proc`. It has no
//! kill path of its own — Command's Kill action for a fleet row is the
//! *existing* dispatch kill (`dispatch_kill::kill_dispatch_run`), gated to
//! dispatch-run rows only; an interactive session found here has no kill
//! affordance at all (see the GUI's `ui::places::command` module doc for why
//! that is a deliberate scope boundary, not an oversight).

use crate::agent_status::is_safe_file_stem;
use crate::attribution::{most_specific_worktree, sort_by_specificity};
use crate::types::WorktreeRef;
use crate::work_sessions::pid_alive;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Upper bound on the `claude agents --json` payload this module will read
/// and parse. A listing is a few hundred bytes per session; a child that
/// emits more than this is not producing a listing, so the read stops at
/// the bound, the child is killed, and the poll reports an error rather
/// than buffering whatever it was sending.
pub const MAX_LISTING_BYTES: usize = 4 * 1024 * 1024;

/// How far up the parent chain [`drop_agent_descendants`] will look for
/// another agent process. A helper sits a handful of levels below its
/// session; the bound keeps a cyclic or absurdly deep table from looping.
pub const MAX_ANCESTOR_DEPTH: usize = 64;

/// Which agent CLI a session belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProcessKind {
    Claude,
    Codex,
}

impl AgentProcessKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

/// What a session is doing right now, as its own CLI reports it. `Unknown`
/// is the honest state for a process only the OS scan saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentActivity {
    /// Waiting for a human: the prompt is empty and nothing is running.
    Idle,
    /// A turn is in progress.
    Busy,
    #[default]
    Unknown,
}

impl AgentActivity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Unknown => "unknown",
        }
    }

    fn from_listed_status(status: Option<&str>) -> Self {
        match status {
            Some("idle") => Self::Idle,
            Some("busy") => Self::Busy,
            _ => Self::Unknown,
        }
    }
}

/// One live process this scan believes is an interactive agent CLI —
/// pre-attribution, OS-agnostic. The parse layer's output type; see
/// [`AgentSession`] for the attributed form the GUI actually renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProcessRow {
    pub pid: u32,
    pub kind: AgentProcessKind,
    pub cwd: Option<PathBuf>,
    /// The CLI's own session id (`sessionId` in the Claude listing) — the
    /// join key to `work_sessions` claims and to the transcript on disk.
    /// `None` for a process only the OS scan saw.
    pub session_id: Option<String>,
    /// The name the CLI lists the session under: a `--name`/`/rename`,
    /// a generated title, or the derived default (`<dir>-<2 chars>`) —
    /// `session_title::resolve_session_title` decides which it is.
    pub name: Option<String>,
    pub activity: AgentActivity,
    /// Unix seconds the process started, when cheaply available (`ps
    /// etime` / `/proc/<pid>/stat`'s `starttime` field). `None` degrades the
    /// Command row's "now" line to an honest "session" with no age, rather
    /// than fabricating a start time.
    pub started_unix: Option<u64>,
    /// Process group id (`ps pgid=` / `/proc/<pid>/stat`'s `pgrp` field).
    /// The fleet-union dedup key: a dispatch run's own spawned `claude`
    /// process shares its supervising shell's pgid (`spawn::
    /// spawn_in_session`'s `setsid()`), so `ui::places::command` cross-checks
    /// this against every in-flight run's `DispatchRunLiveness::Alive`
    /// pgid before rendering a session as a *separate* interactive row — see
    /// that module's doc. `None` when the OS scan couldn't determine it,
    /// which degrades the dedup to a same-worktree fallback rather than a
    /// hard failure.
    pub pgid: Option<i32>,
}

/// One agent session attributed to a worktree — what
/// `ui::places::command::render` actually builds fleet rows from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub pid: u32,
    pub kind: AgentProcessKind,
    /// The process's working directory, kept past attribution because the
    /// derived-default session name is built from its basename.
    pub cwd: Option<PathBuf>,
    pub repo_name: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub worktree_branch: Option<String>,
    pub started_unix: Option<u64>,
    /// See [`AgentProcessRow::pgid`] — carried through attribution unchanged.
    pub pgid: Option<i32>,
    /// See [`AgentProcessRow::session_id`].
    pub session_id: Option<String>,
    /// See [`AgentProcessRow::name`].
    pub name: Option<String>,
    pub activity: AgentActivity,
    /// The one line that says what the session is about — filled by
    /// `session_title::entitle_sessions`, never by attribution, which has
    /// no claims or transcripts to consult. `None` until then.
    pub title: Option<String>,
}

/// What one [`scan_agent_sessions`] produced: the merged rows, plus why the
/// Claude listing contributed nothing when it did not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentScan {
    pub rows: Vec<AgentProcessRow>,
    /// `Some(reason)` when `claude agents --json` could not be run or read;
    /// every Claude row then carries [`AgentActivity::Unknown`] and no name.
    pub listing_error: Option<String>,
}

/// Which binary names count as an interactive agent CLI — see this module's
/// doc for why this is an exact match on the process's own command name, not
/// a substring or argv scan.
fn classify_command(name: &str) -> Option<AgentProcessKind> {
    match name {
        "claude" => Some(AgentProcessKind::Claude),
        "codex" => Some(AgentProcessKind::Codex),
        _ => None,
    }
}

/// Attribute each session to a (repo, worktree) pair via cwd-prefix match —
/// the same longest-specific-path algorithm `attribution::attribute` uses
/// for listeners (most-specific worktree path wins): both call
/// [`crate::attribution::most_specific_worktree`], the one place the
/// algorithm lives.
pub fn attribute_agent_sessions(
    rows: &[AgentProcessRow],
    worktrees: &[WorktreeRef],
) -> Vec<AgentSession> {
    let sorted = sort_by_specificity(worktrees);

    rows.iter()
        .map(|row| {
            let matched = most_specific_worktree(row.cwd.as_deref(), &sorted);
            AgentSession {
                pid: row.pid,
                kind: row.kind,
                cwd: row.cwd.clone(),
                repo_name: matched.map(|w| w.repo_name.clone()),
                worktree_path: matched.map(|w| w.path.clone()),
                worktree_branch: matched.and_then(|w| w.branch.clone()),
                started_unix: row.started_unix,
                pgid: row.pgid,
                session_id: row.session_id.clone(),
                name: row.name.clone(),
                activity: row.activity,
                title: None,
            }
        })
        .collect()
}

/// Everything this module can learn about running agent sessions: the OS
/// scan for `claude`/`codex` processes (read-only, one process-table read
/// plus, on macOS, one batched `lsof -d cwd` call for the matched pids)
/// merged with Claude Code's own session listing. See the module doc for
/// how the two are joined and what happens when the listing is unavailable.
pub fn scan_agent_sessions() -> Result<AgentScan> {
    let scanned = scan_processes()?;
    let (listed, listing_error) = match list_claude_sessions() {
        Ok(rows) => (rows, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let listed = listed
        .into_iter()
        .filter(|row| pid_alive(row.pid))
        .collect();
    Ok(AgentScan {
        rows: merge_agent_rows(scanned, listed),
        listing_error,
    })
}

/// Remove every row whose ancestor chain (via `parent_of`, `None` when the
/// parent is unknown or the pid is init) reaches another row's pid: a
/// session's own spawned agent helper is not a session. Pure; see the
/// module doc for the case this exists for. Order is preserved.
pub fn drop_agent_descendants(
    rows: Vec<AgentProcessRow>,
    parent_of: impl Fn(u32) -> Option<u32>,
) -> Vec<AgentProcessRow> {
    let agent_pids: std::collections::HashSet<u32> = rows.iter().map(|row| row.pid).collect();
    rows.into_iter()
        .filter(|row| !has_agent_ancestor(row.pid, &agent_pids, &parent_of))
        .collect()
}

fn has_agent_ancestor(
    pid: u32,
    agent_pids: &std::collections::HashSet<u32>,
    parent_of: &impl Fn(u32) -> Option<u32>,
) -> bool {
    let mut current = pid;
    for _ in 0..MAX_ANCESTOR_DEPTH {
        let Some(parent) = parent_of(current) else {
            return false;
        };
        if parent <= 1 || parent == current {
            return false;
        }
        if agent_pids.contains(&parent) {
            return true;
        }
        current = parent;
    }
    false
}

fn scan_processes() -> Result<Vec<AgentProcessRow>> {
    #[cfg(target_os = "linux")]
    {
        linux::scan()
    }
    #[cfg(not(target_os = "linux"))]
    {
        scan_ps()
    }
}

/// Join the OS scan and the Claude listing on pid. A listed session wins
/// (it knows its own state, name and session id) and borrows the scan's
/// pgid, plus cwd and start time when the listing lacked them; a scanned
/// process the listing did not name is kept as it was. Sorted by pid so
/// two consecutive scans of the same machine compare equal.
pub fn merge_agent_rows(
    mut scanned: Vec<AgentProcessRow>,
    listed: Vec<AgentProcessRow>,
) -> Vec<AgentProcessRow> {
    let mut merged = Vec::with_capacity(scanned.len() + listed.len());
    for mut row in listed {
        if let Some(position) = scanned.iter().position(|seen| seen.pid == row.pid) {
            let seen = scanned.swap_remove(position);
            row.pgid = seen.pgid;
            if row.cwd.is_none() {
                row.cwd = seen.cwd;
            }
            if row.started_unix.is_none() {
                row.started_unix = seen.started_unix;
            }
        }
        merged.push(row);
    }
    merged.extend(scanned);
    merged.sort_by_key(|row| row.pid);
    merged
}

/// One entry of `claude agents --json`. Every field defaults so a newer CLI
/// adding or dropping keys parses rather than fails; only what this module
/// reads is named.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ListedSession {
    pid: Option<u32>,
    cwd: Option<PathBuf>,
    /// Milliseconds since the epoch.
    started_at: Option<u64>,
    session_id: Option<String>,
    name: Option<String>,
    status: Option<String>,
}

/// Parse the documented `claude agents --json` array into rows. Pure; the
/// only impure step is running the command ([`list_claude_sessions`]).
/// Entries without a pid (background sessions) are skipped — see the
/// module doc.
pub fn parse_claude_agents_listing(raw: &str) -> Result<Vec<AgentProcessRow>> {
    if raw.len() > MAX_LISTING_BYTES {
        bail!(
            "claude agents listing is {} bytes, over the {} byte bound",
            raw.len(),
            MAX_LISTING_BYTES
        );
    }
    let listed: Vec<ListedSession> =
        serde_json::from_str(raw).map_err(|e| anyhow!("claude agents listing: {e}"))?;
    Ok(listed
        .into_iter()
        .filter_map(|session| {
            let pid = session.pid?;
            Some(AgentProcessRow {
                pid,
                kind: AgentProcessKind::Claude,
                cwd: session.cwd,
                started_unix: session.started_at.map(|ms| ms / 1000),
                pgid: None,
                session_id: session.session_id.filter(|id| is_safe_file_stem(id)),
                name: session.name,
                activity: AgentActivity::from_listed_status(session.status.as_deref()),
            })
        })
        .collect())
}

/// Run `claude agents --json` and parse it. `claude` is resolved through
/// `PATH` exactly as `dispatch` and `refine` resolve it; a missing binary
/// or an older CLI without the flag is an ordinary error for the caller
/// to report, never a panic. Stdout is read through [`MAX_LISTING_BYTES`]
/// plus one byte, so an over-long stream is detected without being held.
fn list_claude_sessions() -> Result<Vec<AgentProcessRow>> {
    let mut child = Command::new("claude")
        .args(["agents", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!("failed to spawn claude: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("claude stdout unavailable"))?;
    let mut raw = Vec::with_capacity(64 * 1024);
    let read = stdout
        .take(MAX_LISTING_BYTES as u64 + 1)
        .read_to_end(&mut raw);
    if raw.len() > MAX_LISTING_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        bail!(
            "claude agents listing exceeds the {} byte bound",
            MAX_LISTING_BYTES
        );
    }
    read.map_err(|e| anyhow!("reading claude agents: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| anyhow!("waiting for claude agents: {e}"))?;
    if !output.status.success() {
        bail!("claude agents exited {}", output.status);
    }
    parse_claude_agents_listing(&String::from_utf8_lossy(&raw))
}

#[cfg(not(target_os = "linux"))]
fn scan_ps() -> Result<Vec<AgentProcessRow>> {
    let raw = run_ps()?;
    let now = crate::dispatch_inspect::now_unix();
    let table = parse_ps_table(&raw, now);
    let mut rows = drop_agent_descendants(table.agents, |pid| table.parent_of.get(&pid).copied());
    fill_cwds(&mut rows);
    Ok(rows)
}

/// `-ww` disables BSD `ps`'s terminal-width truncation on the `comm` column
/// — defensive only (the two names we classify on, `claude`/`codex`, are
/// short enough to survive even a truncated column), kept because a future
/// third agent name might not be. `pgid=` is the fleet-union dedup key — see
/// [`AgentProcessRow::pgid`].
#[cfg(not(target_os = "linux"))]
fn run_ps() -> Result<String> {
    let output = Command::new("ps")
        .args(["-axwwo", "pid=,ppid=,pgid=,etime=,comm="])
        .output()
        .map_err(|e| anyhow!("failed to spawn ps: {e}"))?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(anyhow!(
            "ps exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What one `ps` table yields: the agent-CLI rows, plus every process's
/// parent so [`drop_agent_descendants`] can walk a helper up to its session.
#[cfg(not(target_os = "linux"))]
#[derive(Debug, Default)]
struct PsTable {
    agents: Vec<AgentProcessRow>,
    parent_of: std::collections::HashMap<u32, u32>,
}

/// Parse `ps -axwwo pid=,ppid=,pgid=,etime=,comm=` output. Every parsable
/// line contributes its parent link; only `claude`/`codex` lines become
/// rows. Pure and independent of `now` for which lines match; `now` only
/// converts each survivor's elapsed time to an absolute start stamp.
#[cfg(not(target_os = "linux"))]
fn parse_ps_table(raw: &str, now: u64) -> PsTable {
    let mut table = PsTable::default();
    for line in raw.lines() {
        let Some(parsed) = parse_ps_line(line) else {
            continue;
        };
        table.parent_of.insert(parsed.pid, parsed.ppid);
        let Some(basename) = std::path::Path::new(parsed.comm)
            .file_name()
            .and_then(|n| n.to_str())
        else {
            continue;
        };
        let Some(kind) = classify_command(basename) else {
            continue;
        };
        let started_unix = parse_ps_etime(parsed.etime).map(|elapsed| now.saturating_sub(elapsed));
        table.agents.push(AgentProcessRow {
            pid: parsed.pid,
            kind,
            cwd: None,
            started_unix,
            pgid: Some(parsed.pgid),
            session_id: None,
            name: None,
            activity: AgentActivity::Unknown,
        });
    }
    table
}

/// The agent rows alone from a `ps` table - the shape the tests assert on.
#[cfg(all(test, not(target_os = "linux")))]
fn parse_ps_agent_rows(raw: &str, now: u64) -> Vec<AgentProcessRow> {
    parse_ps_table(raw, now).agents
}

#[cfg(not(target_os = "linux"))]
struct PsLine<'a> {
    pid: u32,
    ppid: u32,
    pgid: i32,
    etime: &'a str,
    comm: &'a str,
}

/// One `pid  ppid  pgid  etime  comm` line. Column boundaries are found by
/// scanning for whitespace runs rather than a fixed split, because `ps`
/// right-pads the numeric columns to varying widths depending on the
/// widest value in the whole table.
#[cfg(not(target_os = "linux"))]
fn parse_ps_line(line: &str) -> Option<PsLine<'_>> {
    let mut rest = line.trim_start();
    let mut take = || -> Option<&str> {
        let end = rest.find(char::is_whitespace)?;
        let field = &rest[..end];
        rest = rest[end..].trim_start();
        Some(field)
    };
    let pid: u32 = take()?.parse().ok()?;
    let ppid: u32 = take()?.parse().ok()?;
    let pgid: i32 = take()?.parse().ok()?;
    let etime = take()?;
    let comm = rest.trim();
    if comm.is_empty() {
        return None;
    }
    Some(PsLine {
        pid,
        ppid,
        pgid,
        etime,
        comm,
    })
}

/// BSD `ps`'s `etime` format: `[[DD-]HH:]MM:SS`. Returns total seconds.
#[cfg(not(target_os = "linux"))]
fn parse_ps_etime(s: &str) -> Option<u64> {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<u64>().ok()?, r),
        None => (0, s),
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [h, m, s] => (
            h.parse::<u64>().ok()?,
            m.parse::<u64>().ok()?,
            s.parse::<u64>().ok()?,
        ),
        [m, s] => (0, m.parse::<u64>().ok()?, s.parse::<u64>().ok()?),
        _ => return None,
    };
    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

/// Batch-resolve cwd for every matched pid via one `lsof -d cwd` call —
/// mirrors `scanner::fill_cwds` exactly (reusing its underlying
/// `scanner::cwds_for_pids` rather than re-implementing the parse).
#[cfg(not(target_os = "linux"))]
fn fill_cwds(rows: &mut [AgentProcessRow]) {
    let mut pids: Vec<u32> = rows.iter().map(|r| r.pid).collect();
    pids.sort();
    pids.dedup();
    let cwds = crate::scanner::cwds_for_pids(&pids);
    for row in rows.iter_mut() {
        row.cwd = cwds.get(&row.pid).cloned();
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{classify_command, AgentActivity, AgentProcessRow};
    use anyhow::{Context, Result};
    use std::fs;

    /// `USER_HZ` — clock ticks per second `/proc/<pid>/stat`'s `starttime`
    /// field is measured in. Effectively always 100 on Linux regardless of
    /// timer frequency (the kernel has reported `USER_HZ = 100` on every
    /// mainstream distribution for two decades); `scanner.rs`'s own `/proc`
    /// walk makes the same simplifying assumption implicitly by not reading
    /// `sysconf(_SC_CLK_TCK)` either.
    const CLK_TCK: u64 = 100;

    pub fn scan() -> Result<Vec<AgentProcessRow>> {
        let mut out = Vec::new();
        for entry in fs::read_dir("/proc").context("read /proc")? {
            let Ok(entry) = entry else {
                continue;
            };
            let Some(pid) = pid_from_proc_entry(&entry) else {
                continue;
            };
            let Some(name) = read_comm(pid) else {
                continue;
            };
            let Some(kind) = classify_command(&name) else {
                continue;
            };
            let cwd = fs::read_link(format!("/proc/{pid}/cwd")).ok();
            let fields = stat_fields_after_comm(pid);
            let started_unix = fields.as_deref().and_then(started_unix_from_fields);
            let pgid = fields.as_deref().and_then(pgid_from_fields);
            out.push(AgentProcessRow {
                pid,
                kind,
                cwd,
                started_unix,
                pgid,
                session_id: None,
                name: None,
                activity: AgentActivity::Unknown,
            });
        }
        Ok(super::drop_agent_descendants(out, ppid_of))
    }

    /// `ppid` - field 4 overall, index 1 in [`stat_fields_after_comm`]'s
    /// slice after `state`. Read lazily per ancestor rather than for every
    /// pid in the walk: only matched rows are ever asked about.
    fn ppid_of(pid: u32) -> Option<u32> {
        stat_fields_after_comm(pid)?.get(1)?.parse().ok()
    }

    fn pid_from_proc_entry(entry: &fs::DirEntry) -> Option<u32> {
        entry.file_name().to_string_lossy().parse().ok()
    }

    fn read_comm(pid: u32) -> Option<String> {
        let text = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    /// Read `/proc/<pid>/stat` once and split off everything after the
    /// `comm` field, shared by [`started_unix_from_fields`] and
    /// [`pgid_from_fields`] so a pid's stat file is only read once per scan
    /// rather than once per fact.
    fn stat_fields_after_comm(pid: u32) -> Option<Vec<String>> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // Comm can itself contain spaces/parens; the *last* ")" is always
        // the comm field's own closing paren (same assumption `scanner.rs`'s
        // `pgid_for_pid` makes).
        let after_comm = stat.rsplit_once(')')?.1;
        Some(after_comm.split_whitespace().map(str::to_owned).collect())
    }

    /// `pgrp` — field 5 overall (index 2 in [`stat_fields_after_comm`]'s
    /// slice, after `state` and `ppid`) — the fleet-union dedup key, see
    /// [`AgentProcessRow::pgid`]. Same field `scanner.rs`'s own
    /// `linux::pgid_for_pid` reads for listeners.
    fn pgid_from_fields(fields: &[String]) -> Option<i32> {
        fields.get(2)?.parse().ok()
    }

    /// `starttime` (field 22 overall, index 19 in [`stat_fields_after_comm`]'s
    /// slice, clock ticks since boot) plus the machine's boot epoch — the
    /// same two facts `dispatch_inspect`'s liveness probe and
    /// `crate::boot_time` exist for, reused here rather than re-derived.
    fn started_unix_from_fields(fields: &[String]) -> Option<u64> {
        let starttime_ticks: u64 = fields.get(19)?.parse().ok()?;
        let boot = crate::boot_time::boot_epoch_unix()?;
        Some(boot + starttime_ticks / CLK_TCK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn wt(repo: &str, path: &str, branch: Option<&str>) -> WorktreeRef {
        WorktreeRef {
            repo_name: repo.into(),
            path: PathBuf::from(path),
            branch: branch.map(|b| b.into()),
            head: String::new(),
        }
    }

    fn row(pid: u32, kind: AgentProcessKind, cwd: Option<&str>) -> AgentProcessRow {
        AgentProcessRow {
            pid,
            kind,
            cwd: cwd.map(PathBuf::from),
            started_unix: Some(1_000),
            pgid: Some(pid as i32),
            session_id: None,
            name: None,
            activity: AgentActivity::Unknown,
        }
    }

    const LISTING: &str = r#"[
  {
    "id": "d775e809",
    "cwd": "/Users/dev/code/alpha",
    "kind": "background",
    "startedAt": 1781818916749,
    "sessionId": "d775e809-c321-4a98-a3ba-4c0169d39cab",
    "name": "onboarding loan creation",
    "state": "blocked"
  },
  {
    "pid": 38248,
    "cwd": "/Users/dev/code/alpha",
    "kind": "interactive",
    "startedAt": 1789320176950,
    "sessionId": "c7fad0be-37f2-4ee6-8b96-8911965c628f",
    "name": "alpha-c2",
    "status": "idle"
  },
  {
    "pid": 50066,
    "cwd": "/Users/dev/code/beta",
    "kind": "interactive",
    "startedAt": 1789171761616,
    "sessionId": "6af7e7b8-dccb-4617-aa9b-5b983d1d38e3",
    "name": "board-model-review-proposals",
    "status": "busy",
    "futureField": {"nested": true}
  }
]"#;

    #[test]
    fn listing_parses_interactive_sessions_and_skips_pidless_background_ones() {
        let rows = parse_claude_agents_listing(LISTING).unwrap();
        assert_eq!(rows.len(), 2, "the background entry has no pid");
        assert_eq!(rows[0].pid, 38248);
        assert_eq!(rows[0].kind, AgentProcessKind::Claude);
        assert_eq!(rows[0].activity, AgentActivity::Idle);
        assert_eq!(rows[0].name.as_deref(), Some("alpha-c2"));
        assert_eq!(
            rows[0].session_id.as_deref(),
            Some("c7fad0be-37f2-4ee6-8b96-8911965c628f")
        );
        assert_eq!(
            rows[0].cwd.as_deref(),
            Some(Path::new("/Users/dev/code/alpha"))
        );
        assert_eq!(rows[0].started_unix, Some(1_789_320_176), "ms become s");
        assert_eq!(rows[0].pgid, None, "the listing knows no pgid");
        assert_eq!(rows[1].activity, AgentActivity::Busy);
        assert_eq!(
            rows[1].name.as_deref(),
            Some("board-model-review-proposals")
        );
    }

    #[test]
    fn listing_tolerates_missing_keys_and_refuses_non_arrays_and_oversize() {
        let rows = parse_claude_agents_listing(r#"[{"pid": 7}]"#).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].activity, AgentActivity::Unknown);
        assert_eq!(rows[0].name, None);
        assert!(parse_claude_agents_listing("{}").is_err());
        assert!(parse_claude_agents_listing("not json").is_err());
        let oversize = " ".repeat(MAX_LISTING_BYTES + 1);
        assert!(parse_claude_agents_listing(&oversize).is_err());

        let rows =
            parse_claude_agents_listing(r#"[{"pid":7,"sessionId":"../../x","status":"idle"}]"#)
                .unwrap();
        assert_eq!(rows[0].session_id, None);
        assert_eq!(rows[0].activity, AgentActivity::Idle);
    }

    #[test]
    fn merge_joins_on_pid_keeps_unlisted_processes_and_sorts() {
        let scanned = vec![
            AgentProcessRow {
                started_unix: Some(1_000),
                ..row(50066, AgentProcessKind::Claude, Some("/scan/beta"))
            },
            row(1167, AgentProcessKind::Codex, Some("/scan/gamma")),
            row(99, AgentProcessKind::Claude, Some("/scan/headless")),
        ];
        let listed = parse_claude_agents_listing(LISTING).unwrap();

        let merged = merge_agent_rows(scanned, listed);

        let pids: Vec<u32> = merged.iter().map(|r| r.pid).collect();
        assert_eq!(pids, vec![99, 1167, 38248, 50066]);
        let busy = merged.iter().find(|r| r.pid == 50066).unwrap();
        assert_eq!(
            busy.activity,
            AgentActivity::Busy,
            "the listing's state wins"
        );
        assert_eq!(busy.pgid, Some(50066), "the scan's pgid is borrowed");
        assert_eq!(
            busy.cwd.as_deref(),
            Some(Path::new("/Users/dev/code/beta")),
            "the listing's own cwd is kept when it has one"
        );
        assert_eq!(busy.started_unix, Some(1_789_171_761));
        let idle = merged.iter().find(|r| r.pid == 38248).unwrap();
        assert_eq!(
            idle.pgid, None,
            "listed but never scanned: no pgid to borrow"
        );
        let codex = merged.iter().find(|r| r.pid == 1167).unwrap();
        assert_eq!(codex.kind, AgentProcessKind::Codex);
        assert_eq!(codex.activity, AgentActivity::Unknown);
        let headless = merged.iter().find(|r| r.pid == 99).unwrap();
        assert_eq!(headless.activity, AgentActivity::Unknown);
        assert_eq!(headless.session_id, None);
    }

    #[test]
    fn classifies_the_two_known_agent_binaries_and_nothing_else() {
        assert_eq!(classify_command("claude"), Some(AgentProcessKind::Claude));
        assert_eq!(classify_command("codex"), Some(AgentProcessKind::Codex));
        assert_eq!(classify_command("node"), None);
        assert_eq!(
            classify_command("claude-helper"),
            None,
            "no substring match"
        );
        assert_eq!(
            classify_command("my-codex-fork"),
            None,
            "no substring match"
        );
    }

    #[test]
    fn attributes_sessions_by_the_most_specific_worktree_cwd() {
        let worktrees = vec![
            wt("alpha", "/Users/dev/code/alpha", Some("main")),
            wt(
                "alpha",
                "/Users/dev/code/.worktrees/alpha/feat/tracks-tab",
                Some("feat/tracks-tab"),
            ),
        ];
        let rows = vec![
            row(
                1,
                AgentProcessKind::Claude,
                Some("/Users/dev/code/alpha/lyon"),
            ),
            row(
                2,
                AgentProcessKind::Codex,
                Some("/Users/dev/code/.worktrees/alpha/feat/tracks-tab/services"),
            ),
            row(3, AgentProcessKind::Claude, Some("/usr/bin")),
            row(4, AgentProcessKind::Claude, None),
        ];

        let sessions = attribute_agent_sessions(&rows, &worktrees);

        assert_eq!(sessions[0].repo_name.as_deref(), Some("alpha"));
        assert_eq!(sessions[0].worktree_branch.as_deref(), Some("main"));
        // The more specific worktree path wins over the primary checkout.
        assert_eq!(
            sessions[1].worktree_branch.as_deref(),
            Some("feat/tracks-tab")
        );
        assert_eq!(sessions[2].repo_name, None, "no worktree covers /usr/bin");
        assert_eq!(sessions[3].repo_name, None, "no cwd, nothing to attribute");
        assert_eq!(
            sessions[0].pgid,
            Some(1),
            "pgid carries through attribution unchanged"
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn ps_etime_parses_every_bsd_width() {
        assert_eq!(parse_ps_etime("00:05"), Some(5));
        assert_eq!(parse_ps_etime("05:10"), Some(310));
        assert_eq!(parse_ps_etime("01:23:45"), Some(5_025));
        assert_eq!(parse_ps_etime("1-02:00:00"), Some(93_600));
        assert_eq!(parse_ps_etime("garbage"), None);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn parses_ps_lines_and_drops_non_agent_processes() {
        let raw = "\
  501   400   501 01:23:45 /usr/local/bin/claude
  502   488   488    05:10 /opt/homebrew/bin/codex
  600   400   600 1-02:00:00 /usr/bin/node
";
        let now = 10_000_000;
        let rows = parse_ps_agent_rows(raw, now);

        assert_eq!(rows.len(), 2, "the node row must be dropped");
        assert_eq!(rows[0].pid, 501);
        assert_eq!(rows[0].kind, AgentProcessKind::Claude);
        assert_eq!(rows[0].started_unix, Some(now - 5_025));
        assert_eq!(
            rows[0].pgid,
            Some(501),
            "session-leader pgid == its own pid"
        );
        assert_eq!(rows[1].pid, 502);
        assert_eq!(rows[1].kind, AgentProcessKind::Codex);
        assert_eq!(rows[1].started_unix, Some(now - 310));
        assert_eq!(
            rows[1].pgid,
            Some(488),
            "a pgid distinct from pid must parse too, e.g. a dispatch's spawned agent"
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn a_malformed_ps_line_is_skipped_not_panicked_on() {
        let rows = parse_ps_agent_rows("not a valid line\n\n   \n", 0);
        assert!(rows.is_empty());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn ps_table_records_every_parent_link_not_only_agents() {
        let raw = "\
    1     0     1 9-00:00:00 /sbin/launchd
 4417  3980  4417    05:10 codex
 4507  4417  4507    05:00 /Applications/ChatGPT.app/Contents/Resources/cua_node/bin/node
17526  4507  4507    04:50 /Applications/ChatGPT.app/Contents/Resources/codex
";
        let table = parse_ps_table(raw, 10_000);
        assert_eq!(table.agents.len(), 2, "both codex binaries match by name");
        assert_eq!(table.parent_of.get(&17526), Some(&4507));
        assert_eq!(table.parent_of.get(&4507), Some(&4417));
        assert_eq!(table.parent_of.get(&1), Some(&0));
    }

    #[test]
    fn a_helper_spawned_under_a_session_is_not_a_second_session() {
        // codex 4417 -> node 4507 -> node_repl 4514 -> codex 17526 (helper);
        // codex 1167 stands alone; claude 900's parent is unknown to us.
        let parent: std::collections::HashMap<u32, u32> = [
            (4417, 3980),
            (3980, 1),
            (4507, 4417),
            (4514, 4507),
            (17526, 4514),
        ]
        .into_iter()
        .collect();
        let rows = vec![
            row(4417, AgentProcessKind::Codex, Some("/w/a")),
            row(17526, AgentProcessKind::Codex, Some("/w/a")),
            row(1167, AgentProcessKind::Codex, Some("/w/b")),
            row(900, AgentProcessKind::Claude, Some("/w/c")),
        ];
        let kept = drop_agent_descendants(rows, |pid| parent.get(&pid).copied());
        let pids: Vec<u32> = kept.iter().map(|r| r.pid).collect();
        assert_eq!(
            pids,
            vec![4417, 1167, 900],
            "only the proven child is dropped"
        );
    }

    #[test]
    fn ancestor_walk_is_bounded_against_cycles() {
        let rows = vec![
            row(10, AgentProcessKind::Codex, None),
            row(20, AgentProcessKind::Codex, None),
        ];
        // 20 -> 30 -> 40 -> 30 ... never reaches 10 and never terminates on its own.
        let kept = drop_agent_descendants(rows, |pid| match pid {
            20 => Some(30),
            30 => Some(40),
            40 => Some(30),
            _ => None,
        });
        assert_eq!(kept.len(), 2);
    }
}
