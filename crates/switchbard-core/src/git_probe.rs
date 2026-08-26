//! Per-worktree git status probes. Each function runs one or more `git`
//! subprocesses and returns `None` on any failure (missing remote, weird state,
//! exec error) — we never panic and never propagate errors, because the
//! worktrees view should always render even when half the worktrees have
//! unusual git state.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::git_cmd;

/// One commit's summary line. Used to fill the drift tooltip and the
/// recent-activity column with subjects + timestamps.
#[derive(Debug, Clone)]
pub struct CommitSummary {
    pub short_sha: String,
    pub subject: String,
    /// Commit time in unix epoch seconds. Drift-detail probe ignores this
    /// (we already have it from elsewhere); recent-commits probe relies on
    /// it to compute velocity buckets.
    pub committed_unix: u64,
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

/// Ahead/behind status for `HEAD` against a comparison ref.
///
/// `Ready` means the comparison ran and the branch may still be perfectly
/// in-sync (`ahead = behind = 0`). The non-ready states are intentionally
/// explicit so the UI does not make "no upstream" look the same as "clean".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftProbe {
    Ready {
        base: String,
        ahead: u32,
        behind: u32,
    },
    MissingBase {
        base: String,
    },
    NoUpstream,
}

impl DriftProbe {
    pub fn counts(&self) -> Option<(u32, u32)> {
        match self {
            Self::Ready { ahead, behind, .. } => Some((*ahead, *behind)),
            Self::MissingBase { .. } | Self::NoUpstream => None,
        }
    }

    pub fn is_drifted(&self) -> bool {
        self.counts()
            .is_some_and(|(ahead, behind)| ahead + behind > 0)
    }

    pub fn needs_attention(&self) -> bool {
        match self {
            Self::Ready { ahead, behind, .. } => ahead + behind > 0,
            Self::MissingBase { .. } | Self::NoUpstream => true,
        }
    }

    pub fn base(&self) -> Option<&str> {
        match self {
            Self::Ready { base, .. } | Self::MissingBase { base } => Some(base),
            Self::NoUpstream => None,
        }
    }
}

/// Where a worktree sits relative to the repo's cleanup lifecycle — computed
/// alongside `DriftProbe` by the same git-probe worker (see `workers.rs`'s
/// cadence table). Orthogonal to dirty state: a worktree can be `Merged` and
/// still have uncommitted scratch files, which is exactly why the Workspace
/// view renders staleness and dirty as separate signals (a badge + a filter
/// chip each) rather than folding one into the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeStaleness {
    /// HEAD has no commits unique to it relative to `base` (the repo's local
    /// `main`/`master`) — everything on this branch already landed. Safe
    /// candidate for the bulk-remove sweep once dirty state is also clean.
    Merged { base: String },
    /// The worktree's branch has no configured upstream (`@{u}` unset,
    /// including a detached HEAD). Common right after a squash-merged PR
    /// whose remote branch was deleted — still worth surfacing distinctly
    /// from `Live`, since nothing is tracking it anymore.
    Orphan,
    /// Neither of the above: still ahead of/behind an upstream, i.e.
    /// probably active work.
    Live,
    /// The classification could not be made - git failed on the comparison
    /// itself. Its own variant because the alternative is to guess, and both
    /// guesses lie: `Live` claims active work nobody verified, and `Orphan`
    /// nominates a worktree for retirement on no evidence at all. The badge
    /// renders this as an explicit unknown and no filter chip claims it.
    Unknown,
}

/// Pure fn of `(repo_path, worktree_path)`: is this worktree's branch merged
/// into the repo's default branch, orphaned (no upstream), or still live?
///
/// Priority is merged-first: a branch can lose its upstream *because* it was
/// squash-merged and the remote branch got deleted, so "no upstream" alone
/// isn't a reliable orphan signal — checking "fully contained in `main`/
/// `master`" first is what actually answers "is this safe to retire".
/// `default_branch` and the ahead-count itself (`commits_ahead`) are both
/// shared with `worktree_remove::assess_branch_delete` — the single-row
/// remove dialog's "is it merged" fact and this badge's must never be able
/// to disagree, so there is exactly one place that answers "how many
/// commits ahead" for the whole crate, not two similar-but-distinct git
/// queries that happen to usually agree.
///
/// Never panics; on any git failure this returns [`WorktreeStaleness::Unknown`]
/// rather than guessing. It previously documented a `Live` fallback while the
/// code actually fell through to `Orphan` - the single most retire-me-looking
/// badge - so a failed git call nominated a worktree for cleanup on no
/// evidence. An unclassifiable worktree must never look like a safe
/// bulk-remove candidate, and must never look like a stale one either.
pub fn probe_worktree_staleness(repo_path: &Path, worktree_path: &Path) -> WorktreeStaleness {
    if let Some(base) = crate::worktree_remove::default_branch(repo_path) {
        // Invoked at `worktree_path` (not `repo_path`) so `HEAD` resolves to
        // *this* worktree's checkout — each linked worktree has its own HEAD
        // file even though branch refs themselves are shared repo-wide.
        if let Some(0) = crate::worktree_remove::commits_ahead(worktree_path, &base, "HEAD") {
            return WorktreeStaleness::Merged { base };
        }
    }
    match probe_remote_drift(worktree_path) {
        Some(DriftProbe::NoUpstream) => WorktreeStaleness::Orphan,
        Some(_) => WorktreeStaleness::Live,
        None => WorktreeStaleness::Unknown,
    }
}

/// Changed files in the worktree (the `git status --porcelain` output, line by
/// line). Empty vec = clean; non-empty = dirty.
pub fn probe_dirty_files(path: &Path) -> Option<Vec<String>> {
    let out = git(path, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    Some(out.lines().map(|l| l.to_string()).collect())
}

/// Ignored local files in the worktree, surfaced separately from dirty files
/// because `git worktree remove` can delete ignored artifacts even when the
/// tracked tree is otherwise clean.
pub fn probe_ignored_files(path: &Path) -> Option<Vec<String>> {
    let out = git(
        path,
        &[
            "status",
            "--porcelain=v1",
            "--ignored",
            "--untracked-files=all",
        ],
    )?;
    Some(
        out.lines()
            .filter(|line| line.starts_with("!! "))
            .map(|line| line.to_string())
            .collect(),
    )
}

/// Ahead/behind of `HEAD` relative to the local `main` ref. Returns
/// `MissingBase` when this repo does not have a local `main` branch.
pub fn probe_main_drift(path: &Path) -> Option<DriftProbe> {
    probe_ref_drift(path, "main")
}

/// Ahead/behind of `HEAD` relative to the current branch's configured upstream.
/// Returns `NoUpstream` when `@{u}` is not configured.
pub fn probe_remote_drift(path: &Path) -> Option<DriftProbe> {
    let Some(upstream) = upstream_ref(path) else {
        return Some(DriftProbe::NoUpstream);
    };
    probe_ref_drift(path, &upstream)
}

/// (ahead, behind) relative to `<upstream>` if one is configured. None when
/// there's no upstream or git fails.
pub fn probe_ahead_behind(path: &Path) -> Option<(u32, u32)> {
    probe_remote_drift(path)?.counts()
}

/// Ahead/behind of `HEAD` relative to an arbitrary comparison ref.
pub fn probe_ref_drift(path: &Path, base_ref: &str) -> Option<DriftProbe> {
    if !ref_exists(path, base_ref) {
        return Some(DriftProbe::MissingBase {
            base: base_ref.to_string(),
        });
    }
    let raw = git(
        path,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{base_ref}"),
        ],
    )?;
    let mut parts = raw.split_whitespace();
    let ahead: u32 = parts.next()?.parse().ok()?;
    let behind: u32 = parts.next()?.parse().ok()?;
    Some(DriftProbe::Ready {
        base: base_ref.to_string(),
        ahead,
        behind,
    })
}

/// Lists of commits the local branch is ahead of and behind its upstream by,
/// each capped at `limit`. Returns None when there's no upstream or git fails;
/// returns an empty Default when in sync (both lists empty).
pub fn probe_drift_detail(path: &Path, limit: usize) -> Option<DriftDetail> {
    let upstream = upstream_ref(path)?;
    probe_ref_drift_detail(path, &upstream, limit)
}

/// Lists of commits the local branch is ahead of and behind a named ref by,
/// each capped at `limit`.
pub fn probe_ref_drift_detail(path: &Path, base_ref: &str, limit: usize) -> Option<DriftDetail> {
    let ahead = log_commits(path, &format!("{base_ref}..HEAD"), limit)?;
    let behind = log_commits(path, &format!("HEAD..{base_ref}"), limit)?;
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

/// Up to `limit` most recent commits on the current branch, newest first. Each
/// entry has its short SHA, subject, and unix-seconds commit time so the GUI
/// can derive both a velocity badge ("+3 commits / 30m") and a hover with
/// subjects ("fix: foo · feat: bar · …").
///
/// Returns `Some(vec)` (possibly empty for a brand-new branch) on success,
/// `None` on git failure.
pub fn probe_recent_commits(path: &Path, limit: usize) -> Option<Vec<CommitSummary>> {
    log_commits(path, "HEAD", limit)
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

fn ref_exists(path: &Path, reference: &str) -> bool {
    git(
        path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .is_some()
}

fn log_commits(path: &Path, range: &str, limit: usize) -> Option<Vec<CommitSummary>> {
    // Format: `<short-sha>\t<unix-time>\t<subject>` — tab-separated so subjects
    // containing arbitrary characters don't confuse the parser.
    let out = git(
        path,
        &[
            "log",
            &format!("-n{limit}"),
            "--format=%h%x09%ct%x09%s",
            range,
            "--",
        ],
    )?;
    Some(
        out.lines()
            .filter_map(|l| {
                let mut parts = l.splitn(3, '\t');
                let short_sha = parts.next()?.to_string();
                let committed_unix: u64 = parts.next()?.parse().ok()?;
                let subject = parts.next().unwrap_or("").to_string();
                Some(CommitSummary {
                    short_sha,
                    subject,
                    committed_unix,
                })
            })
            .collect(),
    )
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = git_cmd();
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
    use std::fs;
    use tempfile::TempDir;

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

    #[test]
    fn main_drift_compares_head_to_local_main() {
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "feature.txt", "one", "feature one");
        commit_file(&repo, "feature-2.txt", "two", "feature two");

        assert_eq!(
            probe_main_drift(&repo),
            Some(DriftProbe::Ready {
                base: "main".into(),
                ahead: 2,
                behind: 0,
            })
        );
    }

    #[test]
    fn main_drift_reports_missing_local_main() {
        let (_tmp, repo) = setup_repo("trunk");
        commit_file(&repo, "base.txt", "base", "base");

        assert_eq!(
            probe_main_drift(&repo),
            Some(DriftProbe::MissingBase {
                base: "main".into(),
            })
        );
    }

    #[test]
    fn remote_drift_compares_head_to_upstream() {
        let tmp = TempDir::new().unwrap();
        let remote = tmp.path().join("origin.git");
        let repo = tmp.path().join("repo");
        run_raw_git(&["init", "--bare", remote.to_str().unwrap()]);
        fs::create_dir(&repo).unwrap();
        run_raw_git(&["-C", repo.to_str().unwrap(), "init", "-b", "main"]);
        configure_identity(&repo);
        run_git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["push", "-u", "origin", "main"]);
        run_git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "feature.txt", "one", "feature one");
        run_git(&repo, &["push", "-u", "origin", "feature"]);
        commit_file(&repo, "feature-2.txt", "two", "feature two");

        assert_eq!(
            probe_remote_drift(&repo),
            Some(DriftProbe::Ready {
                base: "origin/feature".into(),
                ahead: 1,
                behind: 0,
            })
        );
    }

    #[test]
    fn remote_drift_reports_no_upstream() {
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["checkout", "-b", "scratch"]);

        assert_eq!(probe_remote_drift(&repo), Some(DriftProbe::NoUpstream));
    }

    #[test]
    fn dirty_probe_lists_nested_untracked_files() {
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        fs::create_dir_all(repo.join("scratch/nested")).unwrap();
        fs::write(repo.join("scratch/nested/local.txt"), "local").unwrap();

        let files = probe_dirty_files(&repo).unwrap();

        assert!(
            files.iter().any(|f| f == "?? scratch/nested/local.txt"),
            "expected nested untracked file, got {files:?}"
        );
    }

    #[test]
    fn ignored_probe_lists_ignored_files() {
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        fs::write(repo.join(".gitignore"), "cache/\n*.local\n").unwrap();
        run_git(&repo, &["add", ".gitignore"]);
        run_git(&repo, &["commit", "-m", "ignore local artifacts"]);
        fs::create_dir(repo.join("cache")).unwrap();
        fs::write(repo.join("cache/app.log"), "cache").unwrap();
        fs::write(repo.join("settings.local"), "secret").unwrap();

        let files = probe_ignored_files(&repo).unwrap();

        assert!(
            files.iter().any(|f| f == "!! cache/app.log"),
            "expected ignored cache file, got {files:?}"
        );
        assert!(
            files.iter().any(|f| f == "!! settings.local"),
            "expected ignored local file, got {files:?}"
        );
    }

    #[test]
    fn staleness_merged_when_branch_fully_contained_in_main() {
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["checkout", "-b", "feature"]);
        // No new commits on `feature` — it's at the same tip as `main`, i.e.
        // trivially merged.
        assert_eq!(
            probe_worktree_staleness(&repo, &repo),
            WorktreeStaleness::Merged {
                base: "main".into()
            }
        );
    }

    #[test]
    fn staleness_orphan_when_no_upstream_and_not_merged() {
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["checkout", "-b", "scratch"]);
        commit_file(&repo, "scratch.txt", "one", "unique commit");

        assert_eq!(
            probe_worktree_staleness(&repo, &repo),
            WorktreeStaleness::Orphan
        );
    }

    #[test]
    fn staleness_live_when_ahead_of_a_configured_upstream() {
        let tmp = TempDir::new().unwrap();
        let remote = tmp.path().join("origin.git");
        let repo = tmp.path().join("repo");
        run_raw_git(&["init", "--bare", remote.to_str().unwrap()]);
        fs::create_dir(&repo).unwrap();
        run_raw_git(&["-C", repo.to_str().unwrap(), "init", "-b", "main"]);
        configure_identity(&repo);
        run_git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["push", "-u", "origin", "main"]);
        run_git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "feature.txt", "one", "feature one");
        run_git(&repo, &["push", "-u", "origin", "feature"]);
        commit_file(&repo, "feature-2.txt", "two", "unpushed");

        assert_eq!(
            probe_worktree_staleness(&repo, &repo),
            WorktreeStaleness::Live
        );
    }

    #[test]
    fn staleness_is_orthogonal_to_dirty_state() {
        // A worktree can be Merged (or Orphan/Live) and still have
        // uncommitted scratch files — dirty is a separate, independently
        // probed signal, not folded into the staleness classification.
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["checkout", "-b", "feature"]);
        fs::write(repo.join("scratch.txt"), "uncommitted").unwrap();

        assert_eq!(
            probe_worktree_staleness(&repo, &repo),
            WorktreeStaleness::Merged {
                base: "main".into()
            }
        );
        let dirty = probe_dirty_files(&repo).unwrap();
        assert!(
            !dirty.is_empty(),
            "scratch file should still show up as dirty"
        );
    }

    fn setup_repo(initial_branch: &str) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        run_raw_git(&["-C", repo.to_str().unwrap(), "init", "-b", initial_branch]);
        configure_identity(&repo);
        (tmp, repo)
    }

    fn configure_identity(repo: &Path) {
        run_git(repo, &["config", "user.email", "switchbard@example.test"]);
        run_git(repo, &["config", "user.name", "Switchbard Tests"]);
    }

    fn commit_file(repo: &Path, file: &str, body: &str, message: &str) {
        fs::write(repo.join(file), body).unwrap();
        run_git(repo, &["add", file]);
        run_git(repo, &["commit", "-m", message]);
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let mut full_args = vec!["-C", repo.to_str().unwrap()];
        full_args.extend_from_slice(args);
        run_raw_git(&full_args);
    }

    fn run_raw_git(args: &[&str]) {
        let output = git_cmd().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
