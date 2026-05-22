//! Worktree removal — surface preconditions, then shell out to `git worktree remove`.
//!
//! The UI's confirm dialog needs three pieces of information before it lets
//! the user pull the trigger:
//!   1. Is this the primary worktree? (refuse outright — removing the primary
//!      breaks the repo, and there's no escape hatch worth offering.)
//!   2. Are there uncommitted changes? (surface them so the user can decide
//!      whether to discard or cancel.)
//!   3. The actual removal call.
//!
//! Everything here is read-only-from-Switchbard's-POV until `remove_worktree`
//! is called; that one runs `git worktree remove [--force]` and is the only
//! destructive operation in this module.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One uncommitted change in the worktree. `status` is the two-character
/// porcelain code (`" M"`, `"??"`, `"A "`, etc.); `path` is the path the
/// porcelain output named (relative to the worktree root).
///
/// The UI renders these directly — we deliberately don't try to humanize the
/// status codes here. Two columns of `XY  path/to/file` is what `git status
/// --short` shows and it's what experienced users expect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyFile {
    pub status: String,
    pub path: PathBuf,
}

/// True iff `worktree_path` is the primary worktree of `repo_path`. We treat
/// "same canonical path" as the signal — Switchbard's `Repo.path` always points
/// at the primary worktree, so this is a path-equality check after canonicalize.
///
/// Falls back to `false` if either path can't be canonicalized; that's safer
/// than `true` (we'd block a legitimate removal) — the worst case is that we
/// let the user try, and `git worktree remove` itself refuses with a clear
/// error.
pub fn is_primary_worktree(repo_path: &Path, worktree_path: &Path) -> bool {
    let Ok(repo_canon) = repo_path.canonicalize() else {
        return false;
    };
    let Ok(wt_canon) = worktree_path.canonicalize() else {
        return false;
    };
    repo_canon == wt_canon
}

/// Parse `git status --porcelain=v1` into structured rows. Returns an empty
/// vec when the worktree is clean. Returns an error if the `git` invocation
/// fails — unlike `git_probe`'s probes, we want the caller to know, because
/// the confirm dialog can't truthfully say "no uncommitted changes" if the
/// status call errored.
pub fn collect_dirty_files(worktree_path: &Path) -> Result<Vec<DirtyFile>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["status", "--porcelain=v1"])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(anyhow!("git status failed: {}", stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().filter_map(parse_porcelain_line).collect())
}

/// Run `git -C <repo> worktree remove [--force] <worktree_path>`. Returns Ok
/// on success; bubbles up git's stderr verbatim on failure so the UI can show
/// the actual reason ("is dirty", "has submodules", "is locked", etc.) rather
/// than a generic "removal failed".
pub fn remove_worktree(repo_path: &Path, worktree_path: &Path, force: bool) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_path).args(["worktree", "remove"]);
    if force {
        cmd.arg("--force");
    }
    cmd.arg(worktree_path);
    let output = cmd.output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Err(anyhow!(
        "git worktree remove failed: {}",
        stderr.trim().replace('\n', "; ")
    ))
}

/// One porcelain line is `XY<sp><path>` where XY is the two-char status. We
/// keep the codes verbatim — the UI renders them as-is.
///
/// Renames take the form `R  old -> new`; for those we surface the destination
/// path (what's on disk now), since that's what the user will see in the
/// dialog and what gets removed.
fn parse_porcelain_line(line: &str) -> Option<DirtyFile> {
    if line.len() < 4 {
        return None;
    }
    let status = line.get(..2)?.to_string();
    let rest = line.get(3..)?.trim();
    let path_str = match rest.split_once(" -> ") {
        Some((_, dest)) => dest,
        None => rest,
    };
    if path_str.is_empty() {
        return None;
    }
    Some(DirtyFile {
        status,
        path: PathBuf::from(path_str),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// Set up a real git repo + a linked worktree on a feature branch.
    /// Returns (repo_root, worktree_path).
    fn setup_repo_with_worktree() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();

        run(&repo, &["init", "-q", "-b", "main"]);
        run(&repo, &["config", "user.email", "test@example.com"]);
        run(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("README.md"), "hello\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "init"]);

        let wt = tmp.path().join("wt-feat");
        run(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feat/foo",
                wt.to_str().unwrap(),
            ],
        );

        (tmp, repo, wt)
    }

    fn run(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .status()
            .expect("git");
        assert!(status.success(), "git {:?} failed in {:?}", args, cwd);
    }

    #[test]
    fn parses_simple_modification() {
        let f = parse_porcelain_line(" M src/foo.rs").unwrap();
        assert_eq!(f.status, " M");
        assert_eq!(f.path, PathBuf::from("src/foo.rs"));
    }

    #[test]
    fn parses_untracked() {
        let f = parse_porcelain_line("?? src/new.rs").unwrap();
        assert_eq!(f.status, "??");
        assert_eq!(f.path, PathBuf::from("src/new.rs"));
    }

    #[test]
    fn parses_rename_to_dest() {
        let f = parse_porcelain_line("R  old.rs -> new.rs").unwrap();
        assert_eq!(f.status, "R ");
        assert_eq!(f.path, PathBuf::from("new.rs"));
    }

    #[test]
    fn primary_worktree_detected() {
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        assert!(is_primary_worktree(&repo, &repo));
        assert!(!is_primary_worktree(&repo, &wt));
    }

    #[test]
    fn clean_worktree_has_no_dirty_files() {
        let (_tmp, _repo, wt) = setup_repo_with_worktree();
        let files = collect_dirty_files(&wt).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn dirty_worktree_surfaces_modified_and_untracked() {
        let (_tmp, _repo, wt) = setup_repo_with_worktree();
        fs::write(wt.join("README.md"), "changed\n").unwrap();
        fs::write(wt.join("new.txt"), "fresh\n").unwrap();

        let files = collect_dirty_files(&wt).unwrap();
        assert_eq!(files.len(), 2);
        let by_path: std::collections::HashMap<_, _> = files
            .into_iter()
            .map(|f| (f.path.clone(), f.status))
            .collect();
        assert_eq!(by_path[&PathBuf::from("README.md")], " M");
        assert_eq!(by_path[&PathBuf::from("new.txt")], "??");
    }

    #[test]
    fn remove_clean_worktree_succeeds() {
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        remove_worktree(&repo, &wt, false).unwrap();
        assert!(!wt.exists(), "worktree dir should be gone");
    }

    #[test]
    fn remove_dirty_worktree_without_force_fails() {
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        fs::write(wt.join("README.md"), "dirty\n").unwrap();
        let err = remove_worktree(&repo, &wt, false).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("git worktree remove failed"), "got: {msg}");
        assert!(wt.exists(), "worktree dir should still be there");
    }

    #[test]
    fn remove_dirty_worktree_with_force_succeeds() {
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        fs::write(wt.join("README.md"), "dirty\n").unwrap();
        remove_worktree(&repo, &wt, true).unwrap();
        assert!(!wt.exists(), "worktree dir should be gone after --force");
    }
}
