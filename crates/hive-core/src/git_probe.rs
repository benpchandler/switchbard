//! Per-worktree git status probes. Each function runs one `git` subprocess and
//! returns `None` on any failure (missing remote, weird state, exec error) — we
//! never panic and never propagate errors, because the worktrees view should
//! always render even when half the worktrees have unusual git state.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// True if the worktree has any uncommitted changes (modified, staged,
/// untracked). False if perfectly clean. None if git failed.
pub fn probe_dirty(path: &Path) -> Option<bool> {
    let out = git(path, &["status", "--porcelain"])?;
    Some(!out.trim().is_empty())
}

/// (ahead, behind) relative to `<upstream>` if one is configured. None when
/// there's no upstream or git fails.
pub fn probe_ahead_behind(path: &Path) -> Option<(u32, u32)> {
    // Resolve upstream first so we can give a clean None when there isn't one,
    // instead of letting `rev-list` error noisily.
    let upstream = git(path, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])?;
    let upstream = upstream.trim();
    if upstream.is_empty() {
        return None;
    }
    let raw = git(path, &["rev-list", "--left-right", "--count", &format!("HEAD...{upstream}")])?;
    let mut parts = raw.split_whitespace();
    let ahead: u32 = parts.next()?.parse().ok()?;
    let behind: u32 = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// Unix epoch seconds of the HEAD commit, or None if git fails.
pub fn probe_head_commit_time(path: &Path) -> Option<u64> {
    let out = git(path, &["log", "-1", "--format=%ct", "HEAD"])?;
    out.trim().parse().ok()
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
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert!(humanize_age(now - 30).ends_with("s ago"));
        assert!(humanize_age(now - 600).ends_with("m ago"));
        assert!(humanize_age(now - 7200).ends_with("h ago"));
        assert!(humanize_age(now - 86_400 * 3).ends_with("d ago"));
        assert!(humanize_age(now - 86_400 * 30).ends_with("w ago"));
        assert!(humanize_age(now - 86_400 * 90).ends_with("mo ago"));
        assert!(humanize_age(now - 86_400 * 400).ends_with("y ago"));
    }
}
