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

use crate::git_cmd;

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
    let output = git_cmd()
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
    let mut cmd = git_cmd();
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

/// Local git facts about deleting the branch that backs a worktree, computed
/// before the confirm dialog offers the "also delete branch" option. The
/// dialog needs to answer two questions, mirroring the worktree's own
/// "safe to remove" checks:
///   - Can we offer deletion at all? (Git refuses to delete a branch that's
///     checked out in another worktree — including the primary repo, which is
///     always sitting on the default branch.)
///   - Is the deletion safe, or would it discard unlanded work? (A branch with
///     commits not yet in the default branch needs `git branch -D`, and the
///     user should see exactly how many commits are at stake.)
///
/// Everything here is read-only; `delete_branch` is the destructive sibling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchDeleteAssessment {
    pub branch: String,
    /// Worktrees *other than the one being removed* that have this branch
    /// checked out. Non-empty ⇒ git would refuse `git branch -d/-D`, so the
    /// dialog hides the option entirely.
    pub other_checkouts: Vec<PathBuf>,
    /// Commits on the branch not reachable from the repo's default branch.
    /// `Some(0)` ⇒ fully landed (a plain `git branch -d` is safe). `Some(n)`
    /// with n > 0 ⇒ n commits unique to the branch (force-delete loses them,
    /// unless the PR was squash-merged). `None` ⇒ no default branch to compare
    /// against, or git failed — treated as "can't prove it's safe".
    pub unmerged_commits: Option<u32>,
    /// The default branch the comparison ran against (`main`/`master`), for the
    /// dialog tooltip. `None` when neither exists.
    pub compared_against: Option<String>,
}

impl BranchDeleteAssessment {
    /// True when git would refuse to delete the branch (checked out elsewhere).
    /// In this case the dialog must not offer deletion at all.
    pub fn is_blocked(&self) -> bool {
        !self.other_checkouts.is_empty()
    }

    /// True when deletion would discard work that isn't on the default branch,
    /// so it requires `git branch -D` and an explicit, loud confirmation.
    /// Also true when we couldn't prove the branch is landed.
    pub fn needs_force(&self) -> bool {
        match self.unmerged_commits {
            Some(0) => false,
            Some(_) => true,
            None => true,
        }
    }

    /// Count of commits unique to the branch, or 0 when fully landed / unknown.
    pub fn unmerged_count(&self) -> u32 {
        self.unmerged_commits.unwrap_or(0)
    }
}

/// Gather the local facts that decide whether the branch behind a worktree can
/// be deleted, and how loud the warning needs to be. `removing_worktree` is the
/// worktree the user is about to remove — it's excluded from the
/// "checked out elsewhere" check because it's about to go away.
pub fn assess_branch_delete(
    repo_path: &Path,
    branch: &str,
    removing_worktree: &Path,
) -> BranchDeleteAssessment {
    let other_checkouts = other_worktrees_on_branch(repo_path, branch, removing_worktree);
    let compared_against = default_branch(repo_path);
    let unmerged_commits = match compared_against.as_deref() {
        Some(base) if base != branch => count_commits_ahead(repo_path, base, branch),
        // The branch *is* the default branch — nothing is unique to it relative
        // to itself. (Deletion is still blocked via `other_checkouts`.)
        Some(_) => Some(0),
        None => None,
    };
    BranchDeleteAssessment {
        branch: branch.to_string(),
        other_checkouts,
        unmerged_commits,
        compared_against,
    }
}

/// `git -C <repo> branch -d|-D <branch>`. Plain `-d` lets git enforce its own
/// "is it merged?" guard; `-D` forces past it. Bubbles git's stderr verbatim so
/// the dialog can show the real reason ("not fully merged", "checked out at
/// …") rather than a generic failure.
pub fn delete_branch(repo_path: &Path, branch: &str, force: bool) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };
    let output = git_cmd()
        .arg("-C")
        .arg(repo_path)
        .args(["branch", flag, branch])
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Err(anyhow!(
        "git branch {flag} failed: {}",
        stderr.trim().replace('\n', "; ")
    ))
}

/// Worktrees on `branch` whose path isn't `removing_worktree` (canonicalized so
/// symlinked `/var` vs `/private/var` on macOS doesn't produce a false match).
fn other_worktrees_on_branch(
    repo_path: &Path,
    branch: &str,
    removing_worktree: &Path,
) -> Vec<PathBuf> {
    let removing = removing_worktree
        .canonicalize()
        .unwrap_or_else(|_| removing_worktree.to_path_buf());
    crate::worktree::enumerate_worktrees(repo_path)
        .unwrap_or_default()
        .into_iter()
        .filter(|w| w.branch.as_deref() == Some(branch))
        .filter(|w| {
            let p = w.path.canonicalize().unwrap_or_else(|_| w.path.clone());
            p != removing
        })
        .map(|w| w.path)
        .collect()
}

/// The repo's default branch, preferring `main` over `master`. Returns `None`
/// when neither local branch exists (e.g. a repo using some other trunk name —
/// we'd rather decline to claim "landed" than guess wrong).
fn default_branch(repo_path: &Path) -> Option<String> {
    ["main", "master"]
        .into_iter()
        .find(|cand| branch_exists(repo_path, cand))
        .map(str::to_string)
}

fn branch_exists(repo_path: &Path, branch: &str) -> bool {
    git_cmd()
        .arg("-C")
        .arg(repo_path)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Number of commits on `branch` not reachable from `base` (`base..branch`).
/// `None` if the git call fails.
fn count_commits_ahead(repo_path: &Path, base: &str, branch: &str) -> Option<u32> {
    let output = git_cmd()
        .arg("-C")
        .arg(repo_path)
        .args(["rev-list", "--count", &format!("{base}..{branch}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
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
        let status = git_cmd()
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

    /// Add a commit on the worktree's branch so it's ahead of main.
    fn commit_on_worktree(wt: &Path, file: &str) {
        fs::write(wt.join(file), "feature work\n").unwrap();
        run(wt, &["add", "."]);
        run(wt, &["commit", "-qm", "feature commit"]);
    }

    #[test]
    fn fresh_branch_at_main_is_landed_and_not_forced() {
        // `worktree add -b feat/foo` branches from main's HEAD, so feat/foo has
        // no unique commits yet.
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        let a = assess_branch_delete(&repo, "feat/foo", &wt);
        assert_eq!(a.unmerged_commits, Some(0));
        assert!(!a.needs_force());
        assert!(!a.is_blocked());
        assert_eq!(a.compared_against.as_deref(), Some("main"));
    }

    #[test]
    fn branch_ahead_of_main_needs_force_and_counts_commits() {
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        commit_on_worktree(&wt, "feature.txt");
        let a = assess_branch_delete(&repo, "feat/foo", &wt);
        assert_eq!(a.unmerged_commits, Some(1));
        assert_eq!(a.unmerged_count(), 1);
        assert!(a.needs_force());
        assert!(!a.is_blocked());
    }

    #[test]
    fn branch_checked_out_in_primary_is_blocked() {
        // `main` is checked out at the primary repo, so deleting it while
        // removing the feat worktree must be blocked.
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        let a = assess_branch_delete(&repo, "main", &wt);
        assert!(a.is_blocked());
        assert!(a
            .other_checkouts
            .iter()
            .any(|p| p.canonicalize().ok() == repo.canonicalize().ok()));
    }

    #[test]
    fn delete_landed_branch_with_plain_d_succeeds() {
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        // Branch must not be checked out anywhere for git to delete it.
        remove_worktree(&repo, &wt, false).unwrap();
        delete_branch(&repo, "feat/foo", false).unwrap();
        assert!(!branch_exists(&repo, "feat/foo"));
    }

    #[test]
    fn delete_unmerged_branch_without_force_fails_but_force_succeeds() {
        let (_tmp, repo, wt) = setup_repo_with_worktree();
        commit_on_worktree(&wt, "feature.txt");
        remove_worktree(&repo, &wt, false).unwrap();

        let err = delete_branch(&repo, "feat/foo", false).unwrap_err();
        assert!(
            format!("{err}").contains("git branch -d failed"),
            "got: {err}"
        );
        assert!(branch_exists(&repo, "feat/foo"), "branch should survive -d");

        delete_branch(&repo, "feat/foo", true).unwrap();
        assert!(!branch_exists(&repo, "feat/foo"), "-D should remove it");
    }
}
