//! Per-worktree git status probes. Each function runs one or more `git`
//! subprocesses and returns `None` on any failure (missing remote, weird state,
//! exec error) — we never panic and never propagate errors, because the
//! worktrees view should always render even when half the worktrees have
//! unusual git state.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// One commit's summary line. Used to fill the drift tooltip so users can see
/// *which* commits are out of sync, not just the count.
#[derive(Debug, Clone)]
pub struct CommitSummary {
    pub short_sha: String,
    pub subject: String,
}

/// The "why" behind a non-zero drift count: the actual commit lists, capped at
/// a small N per side so the tooltip stays a reasonable size.
#[derive(Debug, Clone, Default)]
pub struct DriftDetail {
    pub ahead: Vec<CommitSummary>,
    pub behind: Vec<CommitSummary>,
    /// True when the lists were truncated by the `limit` arg. Lets the UI
    /// render "showing 5 of 12" without a second probe.
    pub ahead_truncated: bool,
    pub behind_truncated: bool,
}

/// Changed files in the worktree (the `git status --porcelain` output, line by
/// line). Empty vec = clean; non-empty = dirty.
pub fn probe_dirty_files(path: &Path) -> Option<Vec<String>> {
    let out = git(path, &["status", "--porcelain"])?;
    Some(out.lines().map(|l| l.to_string()).collect())
}

/// (ahead, behind) relative to `<upstream>` if one is configured. None when
/// there's no upstream or git fails.
pub fn probe_ahead_behind(path: &Path) -> Option<(u32, u32)> {
    let upstream = upstream_ref(path)?;
    let raw = git(
        path,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{upstream}"),
        ],
    )?;
    let mut parts = raw.split_whitespace();
    let ahead: u32 = parts.next()?.parse().ok()?;
    let behind: u32 = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// Lists of commits the local branch is ahead of and behind its upstream by,
/// each capped at `limit`. Returns None when there's no upstream or git fails;
/// returns an empty Default when in sync (both lists empty).
pub fn probe_drift_detail(path: &Path, limit: usize) -> Option<DriftDetail> {
    let upstream = upstream_ref(path)?;
    let ahead = log_commits(path, &format!("{upstream}..HEAD"), limit)?;
    let behind = log_commits(path, &format!("HEAD..{upstream}"), limit)?;
    // Truncation flags: the rev-list count probe is authoritative, but here we
    // can detect "we filled the bucket" — caller compares against ahead/behind
    // counts to refine.
    let ahead_truncated = ahead.len() == limit;
    let behind_truncated = behind.len() == limit;
    Some(DriftDetail {
        ahead,
        behind,
        ahead_truncated,
        behind_truncated,
    })
}

/// Unix epoch seconds of the HEAD commit, or None if git fails.
pub fn probe_head_commit_time(path: &Path) -> Option<u64> {
    let out = git(path, &["log", "-1", "--format=%ct", "HEAD"])?;
    out.trim().parse().ok()
}

/// Unix epoch seconds of the last `git fetch` against this repo, derived from
/// the mtime of `<git-common-dir>/FETCH_HEAD`. Worktrees share the parent
/// repo's gitdir so we resolve via `rev-parse --git-common-dir` instead of
/// assuming `.git/` lives in the worktree itself.
///
/// Returns None if the file doesn't exist yet (a never-fetched clone), or if
/// the git/stat calls fail.
pub fn probe_fetch_age(path: &Path) -> Option<u64> {
    let common_dir = git(path, &["rev-parse", "--git-common-dir"])?;
    let common_dir = common_dir.trim();
    let common_path: PathBuf = if Path::new(common_dir).is_absolute() {
        PathBuf::from(common_dir)
    } else {
        path.join(common_dir)
    };
    let fetch_head = common_path.join("FETCH_HEAD");
    let modified = std::fs::metadata(&fetch_head).ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Convert a unix epoch to a short "5m ago" / "3d ago" / "2w ago" string.
pub fn humanize_age(unix_secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now <= unix_secs {
        return "just now".into();
    }
    let secs = now - unix_secs;
    let (n, unit) = if secs < 60 {
        (secs, "s")
    } else if secs < 3600 {
        (secs / 60, "m")
    } else if secs < 86_400 {
        (secs / 3600, "h")
    } else if secs < 86_400 * 14 {
        (secs / 86_400, "d")
    } else if secs < 86_400 * 60 {
        (secs / (86_400 * 7), "w")
    } else if secs < 86_400 * 365 {
        (secs / (86_400 * 30), "mo")
    } else {
        (secs / (86_400 * 365), "y")
    };
    format!("{n}{unit} ago")
}

fn upstream_ref(path: &Path) -> Option<String> {
    let upstream = git(
        path,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )?;
    let upstream = upstream.trim();
    if upstream.is_empty() {
        None
    } else {
        Some(upstream.to_string())
    }
}

fn log_commits(path: &Path, range: &str, limit: usize) -> Option<Vec<CommitSummary>> {
    let out = git(
        path,
        &[
            "log",
            &format!("-n{limit}"),
            "--format=%h%x09%s",
            range,
            "--",
        ],
    )?;
    Some(
        out.lines()
            .filter_map(|l| {
                let mut parts = l.splitn(2, '\t');
                let short_sha = parts.next()?.to_string();
                let subject = parts.next().unwrap_or("").to_string();
                Some(CommitSummary { short_sha, subject })
            })
            .collect(),
    )
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(path);
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_age_buckets() {
        assert_eq!(humanize_age(u64::MAX), "just now");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(humanize_age(now - 30).ends_with("s ago"));
        assert!(humanize_age(now - 600).ends_with("m ago"));
        assert!(humanize_age(now - 7200).ends_with("h ago"));
        assert!(humanize_age(now - 86_400 * 3).ends_with("d ago"));
        assert!(humanize_age(now - 86_400 * 30).ends_with("w ago"));
        assert!(humanize_age(now - 86_400 * 90).ends_with("mo ago"));
        assert!(humanize_age(now - 86_400 * 400).ends_with("y ago"));
    }
}
