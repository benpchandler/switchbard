use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::git_cmd;

#[derive(Debug, Clone)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: String,
    /// `Some(reason)` when git holds a lock on this worktree, where the reason
    /// is whatever `git worktree lock --reason` recorded (empty when none was
    /// given). `git worktree remove` refuses a locked worktree outright, so
    /// this is a removal precondition, not decoration - see
    /// [`crate::removal_safety::RemovalCheck::NotLocked`]. Porcelain reports
    /// it as a bare `locked` line or `locked <reason>`.
    pub locked: Option<String>,
}

pub fn enumerate_worktrees(repo_path: &Path) -> Result<Vec<WorktreeEntry>> {
    let Some(path_str) = repo_path.to_str() else {
        return Ok(vec![]);
    };
    let output = git_cmd()
        .args(["-C", path_str, "worktree", "list", "--porcelain"])
        .output()?;
    if !output.status.success() {
        return Ok(vec![]);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_worktree_porcelain(&text))
}

fn parse_worktree_porcelain(text: &str) -> Vec<WorktreeEntry> {
    let mut out = Vec::new();
    let mut cur_path: Option<PathBuf> = None;
    let mut cur_branch: Option<String> = None;
    let mut cur_head: String = String::new();
    let mut cur_locked: Option<String> = None;
    let mut prunable = false;

    let flush = |out: &mut Vec<WorktreeEntry>,
                 cur_path: &mut Option<PathBuf>,
                 cur_branch: &mut Option<String>,
                 cur_head: &mut String,
                 cur_locked: &mut Option<String>,
                 prunable: &mut bool| {
        if let Some(p) = cur_path.take() {
            if !*prunable {
                out.push(WorktreeEntry {
                    path: p,
                    branch: cur_branch.take(),
                    head: std::mem::take(cur_head),
                    locked: cur_locked.take(),
                });
            } else {
                *cur_branch = None;
                cur_head.clear();
                *cur_locked = None;
            }
        }
        *prunable = false;
    };

    for line in text.lines() {
        if line.is_empty() {
            flush(
                &mut out,
                &mut cur_path,
                &mut cur_branch,
                &mut cur_head,
                &mut cur_locked,
                &mut prunable,
            );
            continue;
        }
        if let Some(rest) = line.strip_prefix("worktree ") {
            // a new record starts; flush any pending one first (defensive — porcelain
            // is supposed to always separate with blank lines, but be safe)
            flush(
                &mut out,
                &mut cur_path,
                &mut cur_branch,
                &mut cur_head,
                &mut cur_locked,
                &mut prunable,
            );
            cur_path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            cur_head = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            cur_branch = Some(rest.trim_start_matches("refs/heads/").to_string());
        } else if line == "prunable" || line.starts_with("prunable ") {
            prunable = true;
        } else if let Some(rest) = line.strip_prefix("locked") {
            // Porcelain emits a bare `locked` when no reason was recorded,
            // and `locked <reason>` when one was. Both mean git will refuse
            // `git worktree remove`, so both must be captured; only the
            // sentence shown to the user differs.
            cur_locked = Some(rest.trim().to_string());
        }
        // "detached", "bare" etc. - ignored for now
    }
    // trailing record without blank line
    flush(
        &mut out,
        &mut cur_path,
        &mut cur_branch,
        &mut cur_head,
        &mut cur_locked,
        &mut prunable,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_main_and_worktree() {
        let text = "\
worktree /Users/x/Dev/repo
HEAD abc123
branch refs/heads/main

worktree /Users/x/Dev/.worktrees/feat
HEAD def456
branch refs/heads/feat/foo

";
        let out = parse_worktree_porcelain(text);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].path, PathBuf::from("/Users/x/Dev/repo"));
        assert_eq!(out[0].branch.as_deref(), Some("main"));
        assert_eq!(out[1].branch.as_deref(), Some("feat/foo"));
    }

    #[test]
    fn skips_prunable_worktrees() {
        let text = "\
worktree /Users/x/Dev/repo
HEAD abc
branch refs/heads/main

worktree /tmp/gone
HEAD def
detached
prunable gitdir file points to non-existent location

";
        let out = parse_worktree_porcelain(text);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, PathBuf::from("/Users/x/Dev/repo"));
    }

    #[test]
    fn handles_detached_head() {
        let text = "\
worktree /tmp/d
HEAD 9999
detached
";
        let out = parse_worktree_porcelain(text);
        assert_eq!(out.len(), 1);
        assert!(out[0].branch.is_none());
        assert_eq!(out[0].head, "9999");
    }
}
