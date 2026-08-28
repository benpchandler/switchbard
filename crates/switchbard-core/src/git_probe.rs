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

/// Where a worktree sits relative to the repo's cleanup lifecycle — computed
/// alongside `DriftProbe` by the same git-probe worker (see `workers.rs`'s
/// cadence table). Orthogonal to dirty state: a worktree can be `Merged` and
/// still have uncommitted scratch files, which is exactly why the Workspace
/// view renders staleness and dirty as separate signals (a badge + a filter
/// chip each) rather than folding one into the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeStaleness {
    /// Everything on this branch is already in `base` — safe candidate for the
    /// bulk-remove sweep once dirty state is also clean.
    ///
    /// `evidence` records *how* we know, because the two kinds are not
    /// interchangeable downstream: an ancestry match means `git branch -d`
    /// will agree, a patch-equivalent match (a rebase merge) means it will
    /// refuse even though the work is genuinely upstream. Carrying it here
    /// keeps this badge and `removal_safety`'s `WorkLanded` check in lockstep
    /// — they must never be able to disagree about whether a branch landed.
    Merged {
        base: String,
        evidence: crate::removal_safety::LandedEvidence,
    },
    /// The worktree's branch has no configured upstream (`@{u}` unset,
    /// including a detached HEAD). Common right after a squash-merged PR
    /// whose remote branch was deleted — still worth surfacing distinctly
    /// from `Live`, since nothing is tracking it anymore.
    ///
    /// Named for the fact, not for a metaphor. This was `Orphan`, which meant
    /// the row had two words for one condition: the badge said "orphan" while
    /// the remote chip beside it said "no upstream". One fact, one name.
    NoUpstream,
    /// Neither of the above: still ahead of/behind an upstream, i.e.
    /// probably active work.
    Live,
    /// The classification could not be made - git failed on the comparison
    /// itself. Its own variant because the alternative is to guess, and both
    /// guesses lie: `Live` claims active work nobody verified, and `NoUpstream`
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
    use crate::removal_safety::LandedEvidence;
    if let Some(base) = crate::worktree_remove::default_branch(repo_path) {
        // Invoked at `worktree_path` (not `repo_path`) so `HEAD` resolves to
        // *this* worktree's checkout — each linked worktree has its own HEAD
        // file even though branch refs themselves are shared repo-wide.
        if let Some(0) = crate::worktree_remove::commits_ahead(worktree_path, &base, "HEAD") {
            return WorktreeStaleness::Merged {
                base,
                evidence: LandedEvidence::Ancestry,
            };
        }
        // Ancestry says no, which is not the same as "not landed". A
        // rebase-merged branch keeps its patches under new SHAs, so asking
        // only about reachability called a third of this machine's worktrees
        // unmerged when their work was already in the trunk.
        if let Some(0) = crate::worktree_remove::unlanded_commits(worktree_path, &base, "HEAD") {
            return WorktreeStaleness::Merged {
                base,
                evidence: LandedEvidence::PatchEquivalent,
            };
        }
    }
    match probe_remote_drift(worktree_path) {
        Some(DriftProbe::NoUpstream) => WorktreeStaleness::NoUpstream,
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

/// What removing this worktree's branch would actually cost.
///
/// Deliberately *not* a [`DriftProbe`]. `DriftProbe::ahead` counts by
/// ancestry, which is the right question for "am I in sync with my upstream"
/// and the wrong one for "what would I lose", because a rebase-merged commit
/// is ahead by ancestry and already upstream by content. Giving one field both
/// meanings is how the row ends up contradicting its own removal badge - on
/// one real machine the two disagreed for 16 of 41 worktrees, 9 of which were
/// entirely rebase-merged and would have rendered `+N` next to `remove ok`.
///
/// So this measures the way `removal_safety`'s `WorkLanded` check measures:
/// [`crate::worktree_remove::unlanded_commits`], patch-equivalence aware,
/// against [`crate::worktree_remove::default_branch`]. One question, one
/// answer, whichever surface asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkDivergence {
    /// The trunk this was measured against (`origin/main`, `main`, …).
    pub base: String,
    /// Commits whose content is not upstream. These are what a branch delete
    /// would discard, and the number the removal badge reports.
    pub unlanded: u32,
    /// Commits ahead by plain ancestry. Not a risk measure - it is kept
    /// because it is the difference between the two kinds of "landed"
    /// (`git branch -d` agrees only when this is zero) and because
    /// `already_upstream` falls out of it for free rather than costing
    /// another rev-list.
    pub ancestry_ahead: u32,
    /// Commits on the trunk this worktree doesn't have. Ancestry is the right
    /// measure here - being behind is about position, not about risk.
    pub behind: u32,
}

/// The commits behind a [`TrunkDivergence`], for the row's hover.
///
/// `unlanded` is the list form of the same rev-walk that produced
/// [`TrunkDivergence::unlanded`], so the count and the list cannot disagree -
/// they must move together if either changes.
#[derive(Debug, Clone)]
pub struct TrunkDetail {
    pub unlanded: Vec<CommitSummary>,
    pub unlanded_truncated: bool,
    pub behind: Vec<CommitSummary>,
    pub behind_truncated: bool,
    /// Commits ahead by ancestry whose content is already upstream - i.e.
    /// rebase-merged. Reported so the tooltip can account for every commit
    /// the user might expect to see, without counting them as at risk.
    pub already_upstream: u32,
}

/// Measure a worktree against the repo's trunk the way the removal checks do.
///
/// `None` when there is no trunk to compare against, or git failed - callers
/// render that as an explicit unknown rather than as zero.
pub fn probe_trunk_divergence(repo_path: &Path, worktree_path: &Path) -> Option<TrunkDivergence> {
    let base = crate::worktree_remove::default_branch(repo_path)?;
    // Both run at the worktree, not the repo: `HEAD` has to resolve to *this*
    // checkout, and a detached worktree has no branch ref to ask about at the
    // repo. Same reasoning as `probe_worktree_staleness`.
    let unlanded = crate::worktree_remove::unlanded_commits(worktree_path, &base, "HEAD")?;
    let ancestry_ahead = crate::worktree_remove::commits_ahead(worktree_path, &base, "HEAD")?;
    let behind = crate::worktree_remove::commits_ahead(worktree_path, "HEAD", &base)?;
    Some(TrunkDivergence {
        base,
        unlanded,
        ancestry_ahead,
        behind,
    })
}

/// Classify a worktree's place in the cleanup lifecycle from a trunk
/// comparison that has already been made.
///
/// Pure, and deliberately so. [`probe_worktree_staleness`] used to re-run
/// `default_branch`, `commits_ahead` and `unlanded_commits` for itself, which
/// meant the git-probe worker asked git the same three questions twice per
/// worktree per tick - and left open the possibility of the badge and the
/// trunk chip landing on answers from different moments. Deriving both from
/// one [`TrunkDivergence`] costs nothing and makes disagreement unrepresentable.
///
/// `remote` is the worktree's already-probed upstream drift, for the same
/// reason: the fallback classification needs it, and the worker has it.
pub fn staleness_from_trunk(
    trunk: Option<&TrunkDivergence>,
    remote: Option<&DriftProbe>,
) -> WorktreeStaleness {
    use crate::removal_safety::LandedEvidence;
    if let Some(t) = trunk {
        if t.unlanded == 0 {
            return WorktreeStaleness::Merged {
                base: t.base.clone(),
                // Ancestry is the stronger claim and the one `git branch -d`
                // will accept; patch-equivalence means the content landed but
                // the SHAs did not, so a plain `-d` still refuses.
                evidence: if t.ancestry_ahead == 0 {
                    LandedEvidence::Ancestry
                } else {
                    LandedEvidence::PatchEquivalent
                },
            };
        }
    }
    match remote {
        Some(DriftProbe::NoUpstream) => WorktreeStaleness::NoUpstream,
        Some(_) => WorktreeStaleness::Live,
        None => WorktreeStaleness::Unknown,
    }
}

/// The commit lists behind a [`TrunkDivergence`], capped at `limit` per side.
pub fn probe_trunk_detail(
    worktree_path: &Path,
    divergence: &TrunkDivergence,
    limit: usize,
) -> Option<TrunkDetail> {
    let base = &divergence.base;
    let unlanded = unlanded_commit_list(worktree_path, base, "HEAD", limit)?;
    let behind = log_commits(worktree_path, &format!("HEAD..{base}"), limit)?;
    Some(TrunkDetail {
        unlanded_truncated: unlanded.len() == limit,
        unlanded,
        behind_truncated: behind.len() == limit,
        behind,
        // Both counts come from the same divergence, so this needs no git at
        // all. Saturating anyway: the invariant is `unlanded <=
        // ancestry_ahead`, and an arithmetic panic is not how we would want to
        // learn it was violated.
        already_upstream: divergence
            .ancestry_ahead
            .saturating_sub(divergence.unlanded),
    })
}

/// The list form of [`crate::worktree_remove::unlanded_commits`].
///
/// The rev-walk flags are identical to the count's on purpose: `--right-only
/// --cherry-pick` over the symmetric difference. If one changes, both change,
/// or the row will list a different set of commits than it counted.
fn unlanded_commit_list(
    path: &Path,
    base_ref: &str,
    head_ref: &str,
    limit: usize,
) -> Option<Vec<CommitSummary>> {
    log_commits_with(
        path,
        &[
            "--right-only",
            "--cherry-pick",
            &format!("{base_ref}...{head_ref}"),
        ],
        limit,
    )
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
    log_commits_with(path, &[range], limit)
}

/// `git log` over an arbitrary rev-walk, parsed into [`CommitSummary`].
///
/// Split out so the unlanded list can pass the same `--right-only
/// --cherry-pick` flags its count uses instead of re-implementing the parse.
fn log_commits_with(path: &Path, revs: &[&str], limit: usize) -> Option<Vec<CommitSummary>> {
    // Format: `<short-sha>\t<unix-time>\t<subject>` — tab-separated so subjects
    // containing arbitrary characters don't confuse the parser.
    let mut args = vec![
        "log".to_string(),
        format!("-n{limit}"),
        "--format=%h%x09%ct%x09%s".to_string(),
    ];
    args.extend(revs.iter().map(|r| r.to_string()));
    args.push("--".to_string());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = git(path, &arg_refs)?;
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
                base: "main".into(),
                evidence: crate::removal_safety::LandedEvidence::Ancestry,
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
            WorktreeStaleness::NoUpstream
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
        // A worktree can be Merged (or NoUpstream/Live) and still have
        // uncommitted scratch files — dirty is a separate, independently
        // probed signal, not folded into the staleness classification.
        let (_tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        run_git(&repo, &["checkout", "-b", "feature"]);
        fs::write(repo.join("scratch.txt"), "uncommitted").unwrap();

        assert_eq!(
            probe_worktree_staleness(&repo, &repo),
            WorktreeStaleness::Merged {
                base: "main".into(),
                evidence: crate::removal_safety::LandedEvidence::Ancestry,
            }
        );
        let dirty = probe_dirty_files(&repo).unwrap();
        assert!(
            !dirty.is_empty(),
            "scratch file should still show up as dirty"
        );
    }

    /// A repo whose worktree is 3 commits "ahead" of `main` by ancestry, but
    /// whose first two commits are already on `main` under different SHAs -
    /// i.e. rebase-merged. This is the shape that made the drift chip and the
    /// removal badge contradict each other: ancestry says 3 at risk, content
    /// says 1.
    fn repo_with_rebase_merged_worktree() -> (TempDir, PathBuf, PathBuf) {
        let (tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");

        let wt = tmp.path().join("wt");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature",
                wt.to_str().unwrap(),
            ],
        );
        commit_file(&wt, "a.txt", "a", "landed one");
        commit_file(&wt, "b.txt", "b", "landed two");
        commit_file(&wt, "c.txt", "c", "still mine");

        // Replay the first two onto main, which is exactly what a rebase merge
        // does: same patches, new SHAs. `-x` appends a provenance line to the
        // message, which is what forces new SHAs — without it git produces
        // byte-identical commits (same tree, parent, identity and second) and
        // `main` simply fast-forwards onto the originals, which is not the
        // shape under test.
        run_git(&repo, &["cherry-pick", "-x", "feature~2", "feature~1"]);
        (tmp, repo, wt)
    }

    /// The whole reason `TrunkDivergence` is not a `DriftProbe`: it must count
    /// what a branch delete would *discard*, not what is ahead by ancestry.
    #[test]
    fn trunk_divergence_counts_content_not_ancestry() {
        let (_tmp, repo, wt) = repo_with_rebase_merged_worktree();

        let ancestry = crate::worktree_remove::commits_ahead(&wt, "main", "HEAD").unwrap();
        assert_eq!(ancestry, 3, "precondition: 3 commits ahead by ancestry");

        let d = probe_trunk_divergence(&repo, &wt).unwrap();
        assert_eq!(d.base, "main");
        assert_eq!(
            d.unlanded, 1,
            "only the commit whose content is not upstream is at risk"
        );
    }

    /// The row's hover and the row's number come from the same rev-walk, so a
    /// chip reading `+1` can never list three commits.
    #[test]
    fn the_unlanded_list_matches_the_unlanded_count() {
        let (_tmp, repo, wt) = repo_with_rebase_merged_worktree();
        let d = probe_trunk_divergence(&repo, &wt).unwrap();
        let detail = probe_trunk_detail(&wt, &d, 10).unwrap();

        assert_eq!(detail.unlanded.len(), d.unlanded as usize);
        assert_eq!(detail.unlanded[0].subject, "still mine");
        assert!(!detail.unlanded_truncated);
    }

    /// The rebase-merged commits are still accounted for, just not as risk -
    /// otherwise the tooltip silently drops two commits the user knows exist.
    #[test]
    fn rebase_merged_commits_are_reported_as_already_upstream() {
        let (_tmp, repo, wt) = repo_with_rebase_merged_worktree();
        let d = probe_trunk_divergence(&repo, &wt).unwrap();
        let detail = probe_trunk_detail(&wt, &d, 10).unwrap();

        assert_eq!(
            detail.already_upstream, 2,
            "3 ahead by ancestry, 1 at risk, so 2 landed under new SHAs"
        );
    }

    /// A detached worktree has no branch ref, and this must still answer -
    /// same fix as `removal_safety::probe_landed`. It runs at the worktree so
    /// `HEAD` resolves to this checkout.
    #[test]
    fn a_detached_worktree_still_gets_a_trunk_comparison() {
        let (tmp, repo) = setup_repo("main");
        commit_file(&repo, "base.txt", "base", "base");
        let wt = tmp.path().join("wt");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                wt.to_str().unwrap(),
                "main",
            ],
        );
        commit_file(&wt, "mine.txt", "mine", "detached work");

        let d = probe_trunk_divergence(&repo, &wt).unwrap();
        assert_eq!(d.unlanded, 1);
        assert_eq!(d.behind, 0);
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
