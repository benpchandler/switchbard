//! One definition of "safe to remove a worktree".
//!
//! Before this module the answer existed in three places that had quietly
//! drifted apart: the Workspace row's "Safe-delete checks" pill (three
//! checks), the bulk-remove sweep (five gates), and the single-row confirm
//! dialog (dirty files plus its own force/no-force reasoning). The same
//! worktree could read green on the row and land in the sweep's "needs
//! review" list in the same frame, because "safe" meant three different
//! things depending on which surface asked.
//!
//! The rules now live here once, as a pure function of [`RemovalFacts`].
//! Callers differ only in how they *source* those facts: the Workspace row
//! reads cached background probes and may legitimately not have an answer
//! yet, while the two dialogs call [`probe_facts`] synchronously as they
//! open. That difference is exactly why every input is a [`Fact`] rather than
//! a bare value - "not answered yet" and "answered no" must not be the same
//! value, because collapsing them is how a pending probe ends up rendering as
//! a blocker, or worse, as a pass.
//!
//! # The invariant
//!
//! **Only [`RemovalVerdict::Safe`] is ever acted on without an explicit force
//! gesture from the user.** An unanswered check never counts as a passed one.
//! The bulk sweep removes `Safe` candidates and nothing else; that is what
//! keeps "never force-removed in bulk" true by construction rather than by
//! each call site remembering to check.
//!
//! # Why intent is a parameter and not a second rule table
//!
//! Removing a worktree does not lose commits - the branch ref survives, so
//! the work is still there. Deleting the branch does lose them. That is the
//! whole reason the three old definitions disagreed: the bulk sweep defaults
//! its "also delete branches" checkbox **on**, so it must refuse an unmerged
//! branch, while the single-row dialog happily removes an unmerged worktree
//! and only warns at the branch checkbox itself.
//!
//! Rather than encode that as two rule sets that can drift apart again,
//! [`RemovalIntent`] selects which checks are required. [`RemovalCheck::
//! WorkLanded`] is required only when the branch is going away with the
//! worktree.

use std::path::Path;

use crate::worktree_remove::{commits_ahead, default_branch, unlanded_commits};

/// A fact the safety rules need, which the caller may not be able to supply.
///
/// The three variants are not interchangeable and the difference is the point
/// of this type:
///
/// - [`Fact::Known`] - the probe ran and produced an answer.
/// - [`Fact::Pending`] - the probe has not returned yet. Renders as
///   "checking", never as a warning, because a row that is merely still
///   loading has not failed anything.
/// - [`Fact::Unavailable`] - the probe ran and could not answer (git errored,
///   detached HEAD, no default branch to compare against). This **blocks**,
///   carrying its own reason, because "I could not check" is exactly as
///   disqualifying as "I checked and the answer is no".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact<T> {
    Known(T),
    Pending,
    Unavailable(String),
}

/// Nothing probed yet. The default must be the *unhelpful* variant: a
/// `..Default::default()` construction has to yield "I don't know", never
/// "fine".
impl<T> Default for Fact<T> {
    fn default() -> Self {
        Self::Pending
    }
}

impl<T> Fact<T> {
    /// The answer, when there is one. Deliberately the only accessor: a
    /// caller cannot reach a value without acknowledging it might be absent.
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Pending | Self::Unavailable(_) => None,
        }
    }
}

/// What the caller is about to remove. Selects which checks are required -
/// see the module doc's "Why intent is a parameter".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalIntent {
    /// `git worktree remove` alone. The branch survives, so unlanded commits
    /// are not at risk and [`RemovalCheck::WorkLanded`] is not required.
    WorktreeOnly,
    /// `git worktree remove` followed by `git branch -d`. Unlanded commits
    /// would be lost, so [`RemovalCheck::WorkLanded`] is required.
    WorktreeAndBranch,
}

/// The five named preconditions. Adding a sixth means adding it here and
/// nowhere else; every surface renders whatever this enum contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RemovalCheck {
    NotPrimary,
    NotLocked,
    FilesClear,
    WorkLanded,
    NoProcesses,
}

impl RemovalCheck {
    /// Evaluation order, which is also display order: cheapest and most
    /// absolute first, so the reason a user sees first is the one they can
    /// least argue with.
    pub const ALL: [RemovalCheck; 5] = [
        Self::NotPrimary,
        Self::NotLocked,
        Self::FilesClear,
        Self::WorkLanded,
        Self::NoProcesses,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NotPrimary => "linked worktree",
            Self::NotLocked => "not locked",
            Self::FilesClear => "files clear",
            Self::WorkLanded => "work landed",
            Self::NoProcesses => "nothing running",
        }
    }

    /// Whether a failure of this check blocks removal under `intent`.
    pub fn is_required(self, intent: RemovalIntent) -> bool {
        match self {
            Self::WorkLanded => intent == RemovalIntent::WorktreeAndBranch,
            _ => true,
        }
    }
}

/// The state of one evaluated check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOutcome {
    Pass,
    Fail,
    /// The fact could not be obtained. Blocks, like [`CheckOutcome::Fail`].
    Unknown,
    /// The probe has not returned yet. Does not block; it defers.
    Pending,
}

impl CheckOutcome {
    /// True when this outcome stands between the user and a safe removal.
    /// [`CheckOutcome::Pending`] is not blocking - it is not an answer yet.
    pub fn blocks(self) -> bool {
        matches!(self, Self::Fail | Self::Unknown)
    }

    /// The tooltip marker. Four states, four markers, each honest about what
    /// it knows: passed, failed, could not tell, still looking.
    pub fn marker(self) -> &'static str {
        match self {
            Self::Pass => "[x]",
            Self::Fail => "[ ]",
            Self::Unknown => "[?]",
            Self::Pending => "[…]",
        }
    }
}

/// One check plus the sentence a user reads about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub check: RemovalCheck,
    pub outcome: CheckOutcome,
    /// One sentence stating what was found, written to stand alone in a
    /// tooltip or in the bulk dialog's per-row reason.
    pub detail: String,
}

/// The answer every surface renders. There is no "partly safe" and no score:
/// a fraction like "2 of 3" tells a user how close they are without telling
/// them what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalVerdict {
    /// The repo's primary checkout. Not a failure - it is simply not
    /// removable from here, and saying so in the same red as "you have
    /// uncommitted work" would be a lie about what the user should do.
    Primary,
    /// One or more probes are still in flight and nothing has failed yet.
    Checking,
    /// Every required check passed. The only verdict any caller may act on
    /// without an explicit force gesture.
    Safe,
    /// At least one required check failed or could not be answered.
    Blocked,
}

/// Whether the branch's work is already contained in the comparison base.
///
/// Two variants rather than a bare count, because a caller can honestly
/// establish *that* a branch is unlanded without establishing *how far*: the
/// Workspace row reuses the Merged/NoUpstream/Live badge, which only records
/// whether the ahead-count was zero. Encoding that as `count: 1` would have
/// put a number on screen that nothing measured.
/// What proves a branch's work is already in the base.
///
/// The distinction is load-bearing because git's own `branch -d` guard is
/// ancestry-based and will refuse a branch whose *content* landed under
/// different SHAs. Recording which kind of proof we have is what keeps the
/// tool from promising a deletion git will reject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandedEvidence {
    /// The commits themselves are reachable from the base. `git branch -d`
    /// will agree.
    Ancestry,
    /// The commits are not reachable, but every patch has an equivalent in
    /// the base — a rebase merge. The work is safe; `git branch -d` will
    /// nonetheless refuse, because it does not look at content.
    PatchEquivalent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landed {
    /// Every commit is already in `base`; deleting the branch loses nothing.
    Yes {
        /// The ref the comparison ran against, named so the sentence a user
        /// reads says what "landed" was measured against rather than implying
        /// a universal truth.
        base: String,
        /// How we know. This is not decoration: it decides whether
        /// `git branch -d` will agree with us — see [`LandedEvidence`].
        evidence: LandedEvidence,
    },
    /// Work exists outside the base and would go with the branch.
    No {
        /// How many commits, when the caller measured it. `None` when it only
        /// established that the branch is unlanded.
        commits: Option<u32>,
        /// The base compared against, when there was a named one.
        base: Option<String>,
    },
}

/// Everything currently attached to the worktree that a removal would
/// disturb. Three separate counts because they have three different
/// remedies, and a single total would hide which one applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttachedProcesses {
    /// Listening sockets attributed to this worktree by cwd.
    pub listeners: usize,
    /// Services this Switchbard instance started.
    pub switchbard_runs: usize,
    /// Dispatch agent runs whose process group is alive, or whose sidecar
    /// could not be authenticated. Counted because a dispatched agent writes
    /// into the worktree without necessarily listening on any port, which is
    /// precisely the case the old listener-only check could not see.
    pub dispatch_runs: usize,
}

impl AttachedProcesses {
    pub fn total(&self) -> usize {
        self.listeners + self.switchbard_runs + self.dispatch_runs
    }
}

/// The inputs the rules run on. Every caller fills this in; nobody
/// re-implements the rules that read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalFacts {
    /// Always knowable, so not a [`Fact`]. Callers must supply the
    /// canonicalizing [`crate::worktree_remove::is_primary_worktree`] answer,
    /// not a raw path comparison.
    pub is_primary: bool,
    /// `Some(reason)` when git holds a lock on the worktree; `None` when it
    /// does not. Git's own reason string is passed through when it gave one.
    pub lock: Fact<Option<String>>,
    /// Count of uncommitted and untracked files.
    pub dirty_files: Fact<usize>,
    /// Ignored files that removal would also delete. Context, never a
    /// blocker - they are ignored precisely because nobody is tracking them -
    /// but a user deleting 12,000 build artifacts deserves to be told.
    pub ignored_files: Option<usize>,
    /// Whether the work would be lost if the branch went away with the
    /// worktree.
    pub landed: Fact<Landed>,
    pub attached: Fact<AttachedProcesses>,
}

impl RemovalFacts {
    /// A facts set with nothing probed yet, for a caller to fill in field by
    /// field. Defaults to [`Fact::Pending`] rather than to anything
    /// permissive, so a half-populated struct can never evaluate to
    /// [`RemovalVerdict::Safe`].
    pub fn pending(is_primary: bool) -> Self {
        Self {
            is_primary,
            lock: Fact::Pending,
            dirty_files: Fact::Pending,
            ignored_files: None,
            landed: Fact::Pending,
            attached: Fact::Pending,
        }
    }
}

/// The evaluated rule set: the single answer to "can this worktree go".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalSafety {
    checks: Vec<CheckResult>,
}

impl RemovalSafety {
    /// Run the rules. Only checks required by `intent` are evaluated, so a
    /// caller cannot accidentally read an advisory check as a blocking one.
    pub fn evaluate(facts: &RemovalFacts, intent: RemovalIntent) -> Self {
        let checks = RemovalCheck::ALL
            .into_iter()
            .filter(|check| check.is_required(intent))
            .map(|check| {
                let (outcome, detail) = evaluate_one(check, facts);
                CheckResult {
                    check,
                    outcome,
                    detail,
                }
            })
            .collect();
        Self { checks }
    }

    pub fn checks(&self) -> &[CheckResult] {
        &self.checks
    }

    /// Every check standing in the way, in display order.
    pub fn blocking(&self) -> impl Iterator<Item = &CheckResult> {
        self.checks.iter().filter(|c| c.outcome.blocks())
    }

    pub fn verdict(&self) -> RemovalVerdict {
        // Primary first: it is the one failure that is not a warning, and
        // reporting the other four checks for a worktree that can never be
        // removed here would be noise.
        if self
            .checks
            .iter()
            .any(|c| c.check == RemovalCheck::NotPrimary && c.outcome == CheckOutcome::Fail)
        {
            return RemovalVerdict::Primary;
        }
        // A definite failure outranks an outstanding probe: if the tree is
        // already dirty, finishing the process scan cannot change the answer.
        if self.blocking().next().is_some() {
            return RemovalVerdict::Blocked;
        }
        if self
            .checks
            .iter()
            .any(|c| c.outcome == CheckOutcome::Pending)
        {
            return RemovalVerdict::Checking;
        }
        RemovalVerdict::Safe
    }

    /// The short label for an inline badge.
    pub fn headline(&self) -> &'static str {
        match self.verdict() {
            RemovalVerdict::Primary => "primary",
            RemovalVerdict::Checking => "checking…",
            RemovalVerdict::Safe => "remove ok",
            RemovalVerdict::Blocked => "remove blocked",
        }
    }

    /// The hover block: one marked line per check, so a user can see what was
    /// verified and not merely what failed.
    pub fn tooltip(&self) -> String {
        let mut lines = Vec::with_capacity(self.checks.len() + 1);
        lines.push("Safe-delete checks".to_string());
        for check in &self.checks {
            lines.push(format!("{} {}", check.outcome.marker(), check.detail));
        }
        lines.join("\n")
    }

    /// One line naming why this is not removable, for a list where each row
    /// gets a single sentence. `None` when nothing blocks.
    pub fn blocking_reason(&self) -> Option<String> {
        let mut blocking = self.blocking();
        let first = blocking.next()?;
        let rest = blocking.count();
        if rest == 0 {
            return Some(first.detail.clone());
        }
        Some(format!(
            "{} (and {} other check{})",
            first.detail,
            rest,
            plural(rest)
        ))
    }
}

fn evaluate_one(check: RemovalCheck, facts: &RemovalFacts) -> (CheckOutcome, String) {
    match check {
        RemovalCheck::NotPrimary => eval_not_primary(facts),
        RemovalCheck::NotLocked => eval_not_locked(facts),
        RemovalCheck::FilesClear => eval_files_clear(facts),
        RemovalCheck::WorkLanded => eval_work_landed(facts),
        RemovalCheck::NoProcesses => eval_no_processes(facts),
    }
}

/// Fold the two non-answers into outcomes so each rule below only has to
/// describe the answered case.
fn resolve<T>(
    fact: &Fact<T>,
    pending_detail: &str,
    decide: impl FnOnce(&T) -> (CheckOutcome, String),
) -> (CheckOutcome, String) {
    match fact {
        Fact::Known(value) => decide(value),
        Fact::Pending => (CheckOutcome::Pending, pending_detail.to_string()),
        Fact::Unavailable(reason) => (CheckOutcome::Unknown, reason.clone()),
    }
}

fn eval_not_primary(facts: &RemovalFacts) -> (CheckOutcome, String) {
    if facts.is_primary {
        return (
            CheckOutcome::Fail,
            "Primary checkout - can't be removed here".to_string(),
        );
    }
    (
        CheckOutcome::Pass,
        "Linked worktree - removing it leaves the repo intact".to_string(),
    )
}

fn eval_not_locked(facts: &RemovalFacts) -> (CheckOutcome, String) {
    resolve(
        &facts.lock,
        "Checking whether git holds a lock…",
        |lock| match lock {
            Some(reason) if !reason.is_empty() => {
                (CheckOutcome::Fail, format!("Locked by git: {reason}"))
            }
            Some(_) => (
                CheckOutcome::Fail,
                "Locked by git (no reason recorded)".to_string(),
            ),
            None => (CheckOutcome::Pass, "Not locked".to_string()),
        },
    )
}

fn eval_files_clear(facts: &RemovalFacts) -> (CheckOutcome, String) {
    resolve(
        &facts.dirty_files,
        "Checking for uncommitted files…",
        |count| {
            if *count == 0 {
                return (
                    CheckOutcome::Pass,
                    format!(
                        "No uncommitted or untracked files{}",
                        ignored_note(facts.ignored_files, "would also be deleted")
                    ),
                );
            }
            (
                CheckOutcome::Fail,
                format!(
                    "{} changed or untracked file{} need{} review{}",
                    thousands(*count),
                    plural(*count),
                    if *count == 1 { "s" } else { "" },
                    ignored_note(facts.ignored_files, "also present")
                ),
            )
        },
    )
}

fn eval_work_landed(facts: &RemovalFacts) -> (CheckOutcome, String) {
    resolve(
        &facts.landed,
        "Checking whether the branch landed…",
        |landed| match landed {
            Landed::Yes {
                base,
                evidence: LandedEvidence::Ancestry,
            } => (CheckOutcome::Pass, format!("Fully merged into {base}")),
            Landed::Yes {
                base,
                evidence: LandedEvidence::PatchEquivalent,
            } => (
                CheckOutcome::Pass,
                format!("Already in {base} under different commits (rebase-merged)"),
            ),
            Landed::No { commits, base } => {
                let base = base.as_deref().unwrap_or("the default branch");
                let detail = match commits {
                    Some(n) => format!(
                        "{n} commit{} not in {base} would be lost with the branch",
                        plural(*n as usize)
                    ),
                    None => {
                        format!("Not fully merged into {base} - work would be lost with the branch")
                    }
                };
                (CheckOutcome::Fail, detail)
            }
        },
    )
}

fn eval_no_processes(facts: &RemovalFacts) -> (CheckOutcome, String) {
    resolve(
        &facts.attached,
        "Checking what's running here…",
        |attached| {
            if attached.total() == 0 {
                return (
                    CheckOutcome::Pass,
                    "No listeners, services, or dispatch runs here".to_string(),
                );
            }
            (
                CheckOutcome::Fail,
                format!("{} still running here", describe_attached(attached)),
            )
        },
    )
}

/// "2 listeners and 1 dispatch run" - only the non-zero kinds, because a
/// sentence that recites three zeroes buries the one that matters.
fn describe_attached(attached: &AttachedProcesses) -> String {
    let mut parts = Vec::with_capacity(3);
    for (count, noun) in [
        (attached.listeners, "listener"),
        (attached.switchbard_runs, "service"),
        (attached.dispatch_runs, "dispatch run"),
    ] {
        if count > 0 {
            parts.push(format!("{count} {noun}{}", plural(count)));
        }
    }
    match parts.len() {
        0 => "nothing".to_string(),
        1 => parts.remove(0),
        _ => {
            let last = parts.remove(parts.len() - 1);
            format!("{} and {last}", parts.join(", "))
        }
    }
}

fn ignored_note(ignored: Option<usize>, suffix: &str) -> String {
    match ignored {
        Some(count) if count > 0 => format!(
            "; {} ignored file{} {suffix}",
            thousands(count),
            plural(count)
        ),
        _ => String::new(),
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

/// Group digits so a five-figure artifact count reads as a quantity rather
/// than a string of digits the eye has to parse.
fn thousands(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// Gather every fact synchronously, for the callers that are about to act -
/// the two confirm dialogs. They must never read a cached probe: the whole
/// point of a confirmation is that it describes the worktree as it is now,
/// not as a background worker last saw it.
///
/// `attached` is supplied by the caller because process attribution lives in
/// the GUI's scanner state, not in git.
pub fn probe_facts(
    repo_path: &Path,
    worktree_path: &Path,
    branch: Option<&str>,
    attached: Fact<AttachedProcesses>,
) -> RemovalFacts {
    RemovalFacts {
        is_primary: crate::worktree_remove::is_primary_worktree(repo_path, worktree_path),
        lock: probe_worktree_lock(repo_path, worktree_path),
        dirty_files: probe_dirty_count(worktree_path),
        ignored_files: crate::git_probe::probe_ignored_files(worktree_path).map(|f| f.len()),
        landed: probe_landed(repo_path, worktree_path, branch),
        attached,
    }
}

/// Whether git holds a lock on `worktree_path`, and why.
///
/// Public because the Workspace row needs this fact on a background cadence
/// rather than through the whole synchronous [`probe_facts`] sweep - it is
/// the one removal precondition the GUI's per-worktree probe worker has to
/// gather itself.
pub fn probe_worktree_lock(repo_path: &Path, worktree_path: &Path) -> Fact<Option<String>> {
    match crate::worktree::enumerate_worktrees(repo_path) {
        Err(e) => Fact::Unavailable(format!("Couldn't read the repo's worktree list: {e}")),
        Ok(entries) => {
            let target = worktree_path
                .canonicalize()
                .unwrap_or_else(|_| worktree_path.to_path_buf());
            let found = entries.into_iter().find(|entry| {
                entry
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| entry.path.clone())
                    == target
            });
            match found {
                Some(entry) => Fact::Known(entry.locked),
                None => Fact::Unavailable(
                    "git doesn't list this worktree any more - it may already be gone".to_string(),
                ),
            }
        }
    }
}

fn probe_dirty_count(worktree_path: &Path) -> Fact<usize> {
    match crate::worktree_remove::collect_dirty_files(worktree_path) {
        Ok(files) => Fact::Known(files.len()),
        Err(e) => Fact::Unavailable(format!("Couldn't read git status: {e}")),
    }
}

fn probe_landed(repo_path: &Path, worktree_path: &Path, branch: Option<&str>) -> Fact<Landed> {
    let Some(base) = default_branch(repo_path) else {
        return Fact::Unavailable("No local main or master branch to compare against".to_string());
    };
    // Where to ask, and about what.
    //
    // A named branch is a repo-wide ref, so the question is asked at the repo.
    // A *detached* worktree has no branch ref - but it does have its own HEAD,
    // and that is a perfectly good thing to compare. It is also exactly what
    // `git_probe::probe_worktree_staleness` asks about to paint the
    // Merged/NoUpstream/Live badge, which is the point: this function used to give
    // up on `branch: None` and report "there's no branch to prove the work
    // landed on", so a detached worktree parked on `main` read `remove ok` on
    // the row and was routed to the bulk sweep's needs-review list in the same
    // frame. Two sources for one fact, disagreeing - the thing this module
    // exists to prevent.
    //
    // "Detached" is not the same as "unprovable". Removing a detached worktree
    // drops the only ref reaching its commits, so the check still has to fail
    // when those commits are not in the base - it just has to *answer*.
    let (ask_in, head_ref) = match branch {
        Some(branch) if branch == base => {
            return Fact::Known(Landed::Yes {
                base,
                evidence: LandedEvidence::Ancestry,
            })
        }
        Some(branch) => (repo_path, branch),
        None => (worktree_path, "HEAD"),
    };
    // Ancestry first: it is the cheaper query, and when it says "landed" the
    // answer is the strongest kind — `git branch -d` will agree.
    match commits_ahead(ask_in, &base, head_ref) {
        Some(0) => {
            return Fact::Known(Landed::Yes {
                base,
                evidence: LandedEvidence::Ancestry,
            })
        }
        Some(_) => {}
        None => return Fact::Unavailable(format!("Couldn't compare {head_ref} against {base}")),
    }
    // Ancestry says no. Ask the question that actually matters — is the
    // *content* upstream — before calling work unlanded. A rebase-merged
    // branch fails the first check and passes this one.
    match unlanded_commits(ask_in, &base, head_ref) {
        Some(0) => Fact::Known(Landed::Yes {
            base,
            evidence: LandedEvidence::PatchEquivalent,
        }),
        Some(count) => Fact::Known(Landed::No {
            commits: Some(count),
            base: Some(base),
        }),
        None => Fact::Unavailable(format!("Couldn't compare {head_ref} against {base}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::git_env::git_cmd;

    fn git(cwd: &Path, args: &[&str]) {
        let status = git_cmd()
            .arg("-C")
            .arg(cwd)
            .args(args)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?} failed in {cwd:?}");
    }

    /// A repo whose linked worktree sits on a **detached HEAD** parked at a
    /// commit already on `main` — the shape a treehouse/agent tool leaves
    /// behind, and the one that had the row and the dialogs disagreeing.
    fn repo_with_detached_worktree_on_main() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("README.md"), "hello\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);

        let wt = tmp.path().join("wt-detached");
        git(
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
        (tmp, repo, wt)
    }

    /// The invariant this module was built to hold, checked against real git
    /// rather than hand-built facts: the Workspace row's badge and the
    /// dialogs' `probe_facts` must reach the same verdict for the same
    /// worktree.
    ///
    /// They did not, for a detached HEAD. The badge
    /// (`probe_worktree_staleness`) asks `commits_ahead` about the
    /// *worktree's own HEAD* and answers "merged"; `probe_landed` asked about
    /// a *branch name* and gave up when there wasn't one. So a detached
    /// worktree parked on `main` read "remove ok" on the row and landed in
    /// the bulk sweep's needs-review list with "Detached HEAD — there's no
    /// branch to prove the work landed on".
    #[test]
    fn a_detached_worktree_parked_on_main_is_landed_not_unprovable() {
        let (_tmp, repo, wt) = repo_with_detached_worktree_on_main();

        let facts = probe_facts(&repo, &wt, None, Fact::Known(AttachedProcesses::default()));
        assert!(
            matches!(facts.landed, Fact::Known(Landed::Yes { .. })),
            "a detached HEAD sitting on main has demonstrably landed; got {:?}",
            facts.landed
        );
        assert_eq!(
            RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).verdict(),
            RemovalVerdict::Safe,
            "the removal path must agree with the row badge"
        );

        // ...and the badge, the other source of the same fact, must say so too.
        assert!(
            matches!(
                crate::git_probe::probe_worktree_staleness(&repo, &wt),
                crate::git_probe::WorktreeStaleness::Merged { .. }
            ),
            "precondition: the badge calls this merged"
        );
    }

    /// The flip side, so the fix cannot become "detached always passes":
    /// a detached worktree carrying a commit that is *not* on `main` still
    /// blocks, because removing it drops the only ref reaching that commit.
    #[test]
    fn a_detached_worktree_with_unlanded_commits_still_blocks() {
        let (_tmp, repo, wt) = repo_with_detached_worktree_on_main();
        fs::write(wt.join("scratch.txt"), "work\n").unwrap();
        git(&wt, &["add", "."]);
        git(&wt, &["commit", "-qm", "unlanded work"]);

        let facts = probe_facts(&repo, &wt, None, Fact::Known(AttachedProcesses::default()));
        assert!(
            matches!(facts.landed, Fact::Known(Landed::No { .. })),
            "an unlanded detached HEAD must fail the check, not be unprovable; got {:?}",
            facts.landed
        );
        assert_eq!(
            RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).verdict(),
            RemovalVerdict::Blocked
        );
    }

    /// A worktree that passes everything, for tests to spoil one field at a
    /// time. Built explicitly rather than via a `..Default` so that adding a
    /// sixth check forces every test to acknowledge it.
    fn all_clear() -> RemovalFacts {
        RemovalFacts {
            is_primary: false,
            lock: Fact::Known(None),
            dirty_files: Fact::Known(0),
            ignored_files: None,
            landed: Fact::Known(Landed::Yes {
                base: "main".into(),
                evidence: LandedEvidence::Ancestry,
            }),
            attached: Fact::Known(AttachedProcesses::default()),
        }
    }

    fn verdict(facts: &RemovalFacts) -> RemovalVerdict {
        RemovalSafety::evaluate(facts, RemovalIntent::WorktreeAndBranch).verdict()
    }

    #[test]
    fn a_clean_linked_unlocked_idle_merged_worktree_is_safe() {
        assert_eq!(verdict(&all_clear()), RemovalVerdict::Safe);
    }

    /// The invariant the whole module exists to hold: a fact we could not
    /// obtain must never be counted as a fact that passed. Spoiling any
    /// single required check with `Unavailable` has to take `Safe` away.
    #[test]
    fn an_unanswered_check_can_never_read_as_safe() {
        type Spoiler = fn(&mut RemovalFacts);
        let spoilers: [(&str, Spoiler); 4] = [
            ("lock", |f| f.lock = Fact::Unavailable("nope".into())),
            ("dirty_files", |f| {
                f.dirty_files = Fact::Unavailable("nope".into())
            }),
            ("landed", |f| f.landed = Fact::Unavailable("nope".into())),
            ("attached", |f| {
                f.attached = Fact::Unavailable("nope".into())
            }),
        ];
        for (name, spoil) in spoilers {
            let mut facts = all_clear();
            spoil(&mut facts);
            assert_eq!(
                verdict(&facts),
                RemovalVerdict::Blocked,
                "unavailable `{name}` must block, not pass"
            );
        }
    }

    /// The other half: a probe still in flight must not be reported as a
    /// failure either. It defers, it does not accuse.
    #[test]
    fn a_pending_probe_defers_rather_than_blocking() {
        let mut facts = all_clear();
        facts.attached = Fact::Pending;
        assert_eq!(verdict(&facts), RemovalVerdict::Checking);
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert!(
            safety.blocking_reason().is_none(),
            "a pending probe is not a reason to refuse"
        );
    }

    /// Pending must never *upgrade* an already-failed verdict: if the tree is
    /// dirty, finishing the process scan cannot make it removable.
    #[test]
    fn a_definite_failure_outranks_an_outstanding_probe() {
        let mut facts = all_clear();
        facts.dirty_files = Fact::Known(3);
        facts.attached = Fact::Pending;
        assert_eq!(verdict(&facts), RemovalVerdict::Blocked);
    }

    #[test]
    fn the_primary_checkout_is_its_own_verdict_not_a_failure() {
        let mut facts = all_clear();
        facts.is_primary = true;
        assert_eq!(verdict(&facts), RemovalVerdict::Primary);
    }

    /// The primary verdict must win even when other checks also fail, so a
    /// worktree that cannot be removed here never shows a dirty-file scolding
    /// the user cannot act on.
    #[test]
    fn primary_outranks_other_failures() {
        let mut facts = all_clear();
        facts.is_primary = true;
        facts.dirty_files = Fact::Known(9);
        assert_eq!(verdict(&facts), RemovalVerdict::Primary);
    }

    #[test]
    fn a_locked_worktree_is_blocked_and_says_why() {
        let mut facts = all_clear();
        facts.lock = Fact::Known(Some("rebasing".into()));
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert_eq!(safety.verdict(), RemovalVerdict::Blocked);
        assert_eq!(
            safety.blocking_reason().as_deref(),
            Some("Locked by git: rebasing")
        );
    }

    #[test]
    fn a_lock_with_no_recorded_reason_still_blocks() {
        let mut facts = all_clear();
        facts.lock = Fact::Known(Some(String::new()));
        assert_eq!(verdict(&facts), RemovalVerdict::Blocked);
    }

    /// Intent is the whole reason the three old definitions could disagree.
    /// The same unmerged worktree must be removable on its own and refused
    /// when the branch is going with it.
    #[test]
    fn unlanded_commits_block_only_when_the_branch_goes_too() {
        let mut facts = all_clear();
        facts.landed = Fact::Known(Landed::No {
            commits: Some(4),
            base: Some("main".into()),
        });

        let worktree_only = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeOnly);
        assert_eq!(
            worktree_only.verdict(),
            RemovalVerdict::Safe,
            "removing a worktree leaves the branch, so the commits are not at risk"
        );

        let with_branch = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert_eq!(with_branch.verdict(), RemovalVerdict::Blocked);
        assert!(with_branch
            .blocking_reason()
            .unwrap()
            .contains("4 commits not in main"));
    }

    /// A check that is not required under the intent must not appear at all -
    /// otherwise a caller can read an advisory line as a blocking one.
    #[test]
    fn worktree_only_intent_omits_the_landed_check_entirely() {
        let safety = RemovalSafety::evaluate(&all_clear(), RemovalIntent::WorktreeOnly);
        assert!(!safety
            .checks()
            .iter()
            .any(|c| c.check == RemovalCheck::WorkLanded));
        assert_eq!(safety.checks().len(), 4);
    }

    #[test]
    fn a_dispatch_run_alone_is_enough_to_block() {
        let mut facts = all_clear();
        facts.attached = Fact::Known(AttachedProcesses {
            listeners: 0,
            switchbard_runs: 0,
            dispatch_runs: 1,
        });
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert_eq!(safety.verdict(), RemovalVerdict::Blocked);
        assert_eq!(
            safety.blocking_reason().as_deref(),
            Some("1 dispatch run still running here")
        );
    }

    #[test]
    fn attached_processes_name_every_non_zero_kind() {
        let mut facts = all_clear();
        facts.attached = Fact::Known(AttachedProcesses {
            listeners: 2,
            switchbard_runs: 1,
            dispatch_runs: 3,
        });
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert_eq!(
            safety.blocking_reason().as_deref(),
            Some("2 listeners, 1 service and 3 dispatch runs still running here")
        );
    }

    #[test]
    fn ignored_files_are_context_and_never_a_blocker() {
        let mut facts = all_clear();
        facts.ignored_files = Some(12_400);
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert_eq!(safety.verdict(), RemovalVerdict::Safe);
        assert!(safety
            .tooltip()
            .contains("12,400 ignored files would also be deleted"));
    }

    #[test]
    fn a_single_dirty_file_reads_as_singular() {
        let mut facts = all_clear();
        facts.dirty_files = Fact::Known(1);
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert_eq!(
            safety.blocking_reason().as_deref(),
            Some("1 changed or untracked file needs review")
        );
    }

    #[test]
    fn several_blockers_are_summarised_without_hiding_the_count() {
        let mut facts = all_clear();
        facts.dirty_files = Fact::Known(2);
        facts.attached = Fact::Known(AttachedProcesses {
            listeners: 1,
            ..Default::default()
        });
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert_eq!(
            safety.blocking_reason().as_deref(),
            Some("2 changed or untracked files need review (and 1 other check)")
        );
    }

    #[test]
    fn the_tooltip_marks_every_check_including_the_ones_that_passed() {
        let mut facts = all_clear();
        facts.dirty_files = Fact::Known(2);
        facts.attached = Fact::Pending;
        let tooltip = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).tooltip();
        assert!(tooltip.starts_with("Safe-delete checks\n"));
        assert!(tooltip.contains("[x] Linked worktree"));
        assert!(tooltip.contains("[ ] 2 changed or untracked files"));
        assert!(tooltip.contains("[…] Checking what's running here"));
        assert_eq!(tooltip.lines().count(), 6, "one header plus five checks");
    }

    #[test]
    fn headlines_never_report_a_score() {
        let mut facts = all_clear();
        assert_eq!(
            RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).headline(),
            "remove ok"
        );
        facts.dirty_files = Fact::Known(1);
        assert_eq!(
            RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch).headline(),
            "remove blocked"
        );
    }

    /// A half-filled facts set must not evaluate to `Safe` just because the
    /// caller forgot a field.
    #[test]
    fn a_freshly_constructed_facts_set_is_never_safe() {
        let facts = RemovalFacts::pending(false);
        assert_eq!(verdict(&facts), RemovalVerdict::Checking);
    }

    #[test]
    fn a_branch_that_is_the_base_has_nothing_unlanded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        run(&repo, &["config", "user.email", "t@example.com"]);
        run(&repo, &["config", "user.name", "T"]);
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "init"]);

        let facts = probe_facts(&repo, &repo, Some("main"), Fact::Known(Default::default()));
        assert_eq!(
            facts.landed,
            Fact::Known(Landed::Yes {
                base: "main".into(),
                evidence: LandedEvidence::Ancestry,
            })
        );
        assert_eq!(facts.lock, Fact::Known(None));
        assert_eq!(facts.dirty_files, Fact::Known(0));
        assert!(facts.is_primary, "the repo root is its own primary");
    }

    /// `probe_facts` must see a lock git placed, so the pill and the dialogs
    /// stop promising a removal git will refuse.
    #[test]
    fn probe_facts_sees_a_locked_worktree() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        run(&repo, &["config", "user.email", "t@example.com"]);
        run(&repo, &["config", "user.name", "T"]);
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-qm", "init"]);

        let wt = tmp.path().join("wt");
        run(
            &repo,
            &["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()],
        );
        run(
            &repo,
            &["worktree", "lock", "--reason", "held", wt.to_str().unwrap()],
        );

        let facts = probe_facts(&repo, &wt, Some("feat"), Fact::Known(Default::default()));
        assert_eq!(facts.lock, Fact::Known(Some("held".to_string())));
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeOnly);
        assert_eq!(safety.verdict(), RemovalVerdict::Blocked);

        run(&repo, &["worktree", "unlock", wt.to_str().unwrap()]);
        let facts = probe_facts(&repo, &wt, Some("feat"), Fact::Known(Default::default()));
        assert_eq!(facts.lock, Fact::Known(None));
        assert_eq!(
            RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeOnly).verdict(),
            RemovalVerdict::Safe
        );
    }

    fn run(cwd: &Path, args: &[&str]) {
        let status = crate::git_cmd()
            .arg("-C")
            .arg(cwd)
            .args(args)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?} failed in {cwd:?}");
    }
}

/// Landed-detection against real git, exercising the two ways work reaches a
/// base and the one way it does not.
#[cfg(test)]
mod landed_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn git(cwd: &Path, args: &[&str]) -> String {
        let out = crate::git_cmd()
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn commit(repo: &Path, file: &str, body: &str) {
        fs::write(repo.join(file), body).unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-qm", file]);
    }

    /// A repo on `main` with one commit, plus a `feat` branch.
    fn repo_with_feature() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["config", "user.name", "T"]);
        commit(&repo, "base.txt", "base");
        git(&repo, &["checkout", "-q", "-b", "feat"]);
        commit(&repo, "feature.txt", "work");
        git(&repo, &["checkout", "-q", "main"]);
        (tmp, repo)
    }

    /// The bug this whole change exists to fix. A rebase merge replays the
    /// branch's patches onto the trunk under new SHAs, so ancestry says the
    /// work is unlanded while the content is sitting in `main`. On a real
    /// 11-repo machine this was 15 of 46 blocked worktrees.
    #[test]
    fn a_rebase_merged_branch_counts_as_landed() {
        let (_tmp, repo) = repo_with_feature();
        // Move main forward so the replay produces a different SHA, then
        // replay feat's patch onto it — exactly what a rebase merge does.
        commit(&repo, "trunk.txt", "meanwhile");
        git(&repo, &["cherry-pick", "feat"]);

        // Ancestry still insists there is unlanded work...
        assert_eq!(
            crate::worktree_remove::commits_ahead(&repo, "main", "feat"),
            Some(1),
            "precondition: the commit object is genuinely unreachable from main"
        );
        // ...but the patch is upstream, so nothing would be lost.
        let facts = probe_facts(&repo, &repo, Some("feat"), Fact::Known(Default::default()));
        assert_eq!(
            facts.landed,
            Fact::Known(Landed::Yes {
                base: "main".into(),
                evidence: LandedEvidence::PatchEquivalent,
            })
        );
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        assert!(safety
            .checks()
            .iter()
            .any(|c| c.check == RemovalCheck::WorkLanded && c.outcome == CheckOutcome::Pass));
    }

    /// The evidence has to survive to the caller, because `git branch -d` is
    /// ancestry-based and will refuse a rebase-merged branch. A caller that
    /// cannot tell the two apart promises a deletion git rejects.
    #[test]
    fn patch_equivalence_is_reported_distinctly_from_ancestry() {
        let (_tmp, repo) = repo_with_feature();
        git(
            &repo,
            &["merge", "-q", "--no-ff", "-m", "merge feat", "feat"],
        );
        let facts = probe_facts(&repo, &repo, Some("feat"), Fact::Known(Default::default()));
        assert_eq!(
            facts.landed,
            Fact::Known(Landed::Yes {
                base: "main".into(),
                evidence: LandedEvidence::Ancestry,
            }),
            "a real merge keeps the commits reachable, so ancestry is the proof"
        );
    }

    /// Genuinely unlanded work must still block. A content check that says
    /// "landed" too eagerly is far worse than the ancestry check it replaces.
    #[test]
    fn genuinely_unlanded_work_still_blocks() {
        let (_tmp, repo) = repo_with_feature();
        let facts = probe_facts(&repo, &repo, Some("feat"), Fact::Known(Default::default()));
        assert_eq!(
            facts.landed,
            Fact::Known(Landed::No {
                commits: Some(1),
                base: Some("main".into()),
            })
        );
        // The fixture's worktree *is* the repo root, so the verdict is
        // `Primary` (which outranks everything). Assert on the check itself,
        // which is what this test is about.
        let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
        let landed = safety
            .checks()
            .iter()
            .find(|c| c.check == RemovalCheck::WorkLanded)
            .unwrap();
        assert_eq!(landed.outcome, CheckOutcome::Fail);
        assert!(landed.detail.contains("1 commit not in main"));
    }

    /// The second cause, worth 8 more of that machine's 46: the local trunk is
    /// whatever was last pulled. Work that landed upstream reads as unlanded
    /// until the user happens to fetch, so the comparison prefers the
    /// remote-tracking ref when one exists.
    #[test]
    fn the_base_prefers_origin_main_over_a_stale_local_main() {
        let tmp = TempDir::new().unwrap();
        let origin = tmp.path().join("origin.git");
        let repo = tmp.path().join("repo");
        crate::git_cmd()
            .args(["init", "-q", "--bare", origin.to_str().unwrap()])
            .status()
            .unwrap();
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["config", "user.name", "T"]);
        git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        commit(&repo, "base.txt", "base");
        git(&repo, &["push", "-q", "-u", "origin", "main"]);

        git(&repo, &["checkout", "-q", "-b", "feat"]);
        commit(&repo, "feature.txt", "work");

        // The branch lands upstream, and the local `main` is left behind —
        // the ordinary state of a machine that dispatches agents.
        git(&repo, &["push", "-q", "origin", "feat:main"]);
        git(&repo, &["fetch", "-q", "origin"]);
        assert_eq!(
            crate::worktree_remove::commits_ahead(&repo, "main", "feat"),
            Some(1),
            "precondition: local main really is stale"
        );

        let facts = probe_facts(&repo, &repo, Some("feat"), Fact::Known(Default::default()));
        assert_eq!(
            facts.landed,
            Fact::Known(Landed::Yes {
                base: "origin/main".into(),
                evidence: LandedEvidence::Ancestry,
            }),
            "the comparison must use origin/main, and must say so on screen"
        );
    }

    /// No remote-tracking ref (never fetched, offline clone) must fall back to
    /// the local trunk rather than refusing to answer.
    #[test]
    fn the_base_falls_back_to_local_main_without_a_remote() {
        let (_tmp, repo) = repo_with_feature();
        git(
            &repo,
            &["merge", "-q", "--no-ff", "-m", "merge feat", "feat"],
        );
        let facts = probe_facts(&repo, &repo, Some("feat"), Fact::Known(Default::default()));
        assert!(matches!(
            facts.landed,
            Fact::Known(Landed::Yes { ref base, .. }) if base == "main"
        ));
    }
}
