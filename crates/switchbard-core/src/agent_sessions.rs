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
//! ## What counts as "an agent session"
//!
//! A process whose own command name (`ps -o comm=` / `/proc/<pid>/comm`) is
//! **exactly** `claude` or `codex` — see [`classify_command`]. This is a
//! deliberately narrow, honest boundary, not a guess: many real installs of
//! either CLI actually run as a wrapping interpreter (`node`, a shim
//! script), in which case this scan will not see them. Widening the match to
//! catch those would mean matching on argv content instead of the process's
//! own identity, which risks false positives (any process that happens to
//! mention "claude" in an argument) for a feature whose only job is to tell
//! the truth about what is running. Documented as a known gap rather than
//! quietly worked around — see the TASK-98 report.
//!
//! ## Read-only and bounded
//!
//! This module only ever reads process tables (`ps`) or `/proc`. It has no
//! kill path of its own — Command's Kill action for a fleet row is the
//! *existing* dispatch kill (`dispatch_kill::kill_dispatch_run`), gated to
//! dispatch-run rows only; an interactive session found here has no kill
//! affordance at all (see the GUI's `ui::places::command` module doc for why
//! that is a deliberate scope boundary, not an oversight).

use crate::types::WorktreeRef;
use anyhow::Result;
use std::path::PathBuf;

#[cfg(not(target_os = "linux"))]
use anyhow::anyhow;
#[cfg(not(target_os = "linux"))]
use std::process::Command;

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

/// One live process this scan believes is an interactive agent CLI —
/// pre-attribution, OS-agnostic. The parse layer's output type; see
/// [`AgentSession`] for the attributed form the GUI actually renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProcessRow {
    pub pid: u32,
    pub kind: AgentProcessKind,
    pub cwd: Option<PathBuf>,
    /// Unix seconds the process started, when cheaply available (`ps
    /// etime` / `/proc/<pid>/stat`'s `starttime` field). `None` degrades the
    /// Command row's "now" line to an honest "session" with no age, rather
    /// than fabricating a start time.
    pub started_unix: Option<u64>,
}

/// One agent session attributed to a worktree — what
/// `ui::places::command::render` actually builds fleet rows from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub pid: u32,
    pub kind: AgentProcessKind,
    pub repo_name: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub worktree_branch: Option<String>,
    pub started_unix: Option<u64>,
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
/// the identical longest-specific-path algorithm `attribution::attribute`
/// uses for listeners (most-specific worktree path wins), reimplemented here
/// rather than shared because the two source types (`LocalListener` vs.
/// `AgentProcessRow`) carry no port/pgid in common for a generic function to
/// key on profitably.
pub fn attribute_agent_sessions(
    rows: &[AgentProcessRow],
    worktrees: &[WorktreeRef],
) -> Vec<AgentSession> {
    let mut sorted: Vec<&WorktreeRef> = worktrees.iter().collect();
    sorted.sort_by_key(|w| std::cmp::Reverse(w.path.as_os_str().len()));

    rows.iter()
        .map(|row| {
            let matched = row
                .cwd
                .as_ref()
                .and_then(|cwd| sorted.iter().find(|w| cwd.starts_with(&w.path)));
            AgentSession {
                pid: row.pid,
                kind: row.kind,
                repo_name: matched.map(|w| w.repo_name.clone()),
                worktree_path: matched.map(|w| w.path.clone()),
                worktree_branch: matched.and_then(|w| w.branch.clone()),
                started_unix: row.started_unix,
            }
        })
        .collect()
}

/// Scan the OS for running `claude`/`codex` processes. Read-only, bounded to
/// one process-table read (plus, on macOS, one batched `lsof -d cwd` call
/// for the matched pids only — never one `lsof` per pid).
pub fn scan_agent_sessions() -> Result<Vec<AgentProcessRow>> {
    #[cfg(target_os = "linux")]
    {
        linux::scan()
    }
    #[cfg(not(target_os = "linux"))]
    {
        scan_ps()
    }
}

#[cfg(not(target_os = "linux"))]
fn scan_ps() -> Result<Vec<AgentProcessRow>> {
    let raw = run_ps()?;
    let now = crate::dispatch_inspect::now_unix();
    let mut rows = parse_ps_agent_rows(&raw, now);
    fill_cwds(&mut rows);
    Ok(rows)
}

/// `-ww` disables BSD `ps`'s terminal-width truncation on the `comm` column
/// — defensive only (the two names we classify on, `claude`/`codex`, are
/// short enough to survive even a truncated column), kept because a future
/// third agent name might not be.
#[cfg(not(target_os = "linux"))]
fn run_ps() -> Result<String> {
    let output = Command::new("ps")
        .args(["-axwwo", "pid=,etime=,comm="])
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

/// Parse `ps -axwwo pid=,etime=,comm=` output into agent-CLI rows, dropping
/// every line that isn't `claude`/`codex`. Pure and independent of `now` for
/// which lines match; `now` only converts each survivor's elapsed time to an
/// absolute start stamp.
fn parse_ps_agent_rows(raw: &str, now: u64) -> Vec<AgentProcessRow> {
    raw.lines()
        .filter_map(|line| parse_ps_line(line, now))
        .collect()
}

/// One `pid  etime  comm` line. Column boundaries are found by scanning for
/// whitespace runs rather than a fixed split, because `ps` right-pads `pid`
/// and `etime` to varying widths depending on the widest value in the whole
/// table.
fn parse_ps_line(line: &str, now: u64) -> Option<AgentProcessRow> {
    let trimmed = line.trim_start();
    let pid_end = trimmed.find(char::is_whitespace)?;
    let pid: u32 = trimmed[..pid_end].parse().ok()?;
    let after_pid = trimmed[pid_end..].trim_start();
    let etime_end = after_pid.find(char::is_whitespace)?;
    let etime_str = &after_pid[..etime_end];
    let comm = after_pid[etime_end..].trim();
    if comm.is_empty() {
        return None;
    }
    let basename = std::path::Path::new(comm).file_name()?.to_str()?;
    let kind = classify_command(basename)?;
    let started_unix = parse_ps_etime(etime_str).map(|elapsed| now.saturating_sub(elapsed));
    Some(AgentProcessRow {
        pid,
        kind,
        cwd: None,
        started_unix,
    })
}

/// BSD `ps`'s `etime` format: `[[DD-]HH:]MM:SS`. Returns total seconds.
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
    use super::{classify_command, AgentProcessRow};
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
            let started_unix = started_unix_for_pid(pid);
            out.push(AgentProcessRow {
                pid,
                kind,
                cwd,
                started_unix,
            });
        }
        Ok(out)
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

    /// `starttime` (field 22 of `/proc/<pid>/stat`, clock ticks since boot)
    /// plus the machine's boot epoch — the same two facts
    /// `dispatch_inspect`'s liveness probe and `crate::boot_time` exist for,
    /// reused here rather than re-derived.
    fn started_unix_for_pid(pid: u32) -> Option<u64> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // Comm can itself contain spaces/parens; the *last* ")" is always
        // the comm field's own closing paren (same assumption `scanner.rs`'s
        // `pgid_for_pid` makes).
        let after_comm = stat.rsplit_once(')')?.1;
        let fields: Vec<&str> = after_comm.split_whitespace().collect();
        // `state` is field 3 overall (index 0 here); `starttime` is field 22
        // overall, i.e. index 19 in this slice.
        let starttime_ticks: u64 = fields.get(19)?.parse().ok()?;
        let boot = crate::boot_time::boot_epoch_unix()?;
        Some(boot + starttime_ticks / CLK_TCK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
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
    }

    #[test]
    fn ps_etime_parses_every_bsd_width() {
        assert_eq!(parse_ps_etime("00:05"), Some(5));
        assert_eq!(parse_ps_etime("05:10"), Some(310));
        assert_eq!(parse_ps_etime("01:23:45"), Some(5_025));
        assert_eq!(parse_ps_etime("1-02:00:00"), Some(93_600));
        assert_eq!(parse_ps_etime("garbage"), None);
    }

    #[test]
    fn parses_ps_lines_and_drops_non_agent_processes() {
        let raw = "\
  501 01:23:45 /usr/local/bin/claude
  502    05:10 /opt/homebrew/bin/codex
  600 1-02:00:00 /usr/bin/node
";
        let now = 10_000_000;
        let rows = parse_ps_agent_rows(raw, now);

        assert_eq!(rows.len(), 2, "the node row must be dropped");
        assert_eq!(rows[0].pid, 501);
        assert_eq!(rows[0].kind, AgentProcessKind::Claude);
        assert_eq!(rows[0].started_unix, Some(now - 5_025));
        assert_eq!(rows[1].pid, 502);
        assert_eq!(rows[1].kind, AgentProcessKind::Codex);
        assert_eq!(rows[1].started_unix, Some(now - 310));
    }

    #[test]
    fn a_malformed_ps_line_is_skipped_not_panicked_on() {
        let rows = parse_ps_agent_rows("not a valid line\n\n   \n", 0);
        assert!(rows.is_empty());
    }
}
