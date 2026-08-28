//! How far a worktree's unlanded work has got on its way to the trunk.
//!
//! The Workspace row could already say *how much* work a worktree holds that
//! the trunk doesn't (`TrunkDivergence`). It could not say why that work was
//! still sitting there, because everything it read was local git. Measured
//! across one real machine, "19 worktrees with unlanded commits" turned out to
//! be four different situations wanting four different responses:
//!
//!   - three were pushed with a PR open — in review, working as intended;
//!   - one had its PR **closed** — rejected work, and re-offering it would be
//!     actively wrong;
//!   - four were pushed but no PR was ever opened — fell through a crack;
//!   - eleven were never pushed at all, including a dispatch run whose agent
//!     committed and whose pipeline died before `push` (TASK-39).
//!
//! Only the middle two are stalls. A row that shows unlanded commits without
//! the stage invites the user to act on all nineteen.
//!
//! # Two facts, deliberately separate
//!
//! [`PushState`] is local git and costs nothing beyond a `rev-parse` — the
//! remote-tracking ref is already on disk. [`PrState`] needs the network via
//! `gh`, which is far too slow for the per-worktree probe tick and can fail
//! for reasons that say nothing about the worktree (no `gh`, not logged in,
//! not a GitHub remote). They are probed separately, on different cadences,
//! and [`LandingStage::derive`] is a pure function of the two — so a missing
//! PR answer degrades the row to "pushed, PR state unknown" instead of
//! blocking it or, worse, guessing.

use std::path::Path;

use crate::git_env::git_cmd;

/// Whether the branch exists on the remote, and whether it is current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushState {
    /// The remote has never seen this branch.
    NotPushed,
    /// The remote has it, and local `HEAD` matches.
    Pushed,
    /// The remote has it, but local `HEAD` is ahead — the last push predates
    /// the newest commits. Distinct from `Pushed` because it is the state
    /// where an open PR is reviewing something other than what is on disk.
    PushedStale { local_ahead: u32 },
    /// The question could not be answered (git failed, no such remote).
    Unknown,
}

/// A pull request for the branch, as GitHub reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrState {
    Open {
        number: u32,
        url: String,
    },
    /// Closed without merging. The work was offered and declined; a landing
    /// action must never silently re-offer it.
    Closed {
        number: u32,
        url: String,
    },
    /// Merged. Paired with a non-zero unlanded count this is the **squash
    /// tell**: `unlanded_commits` is patch-id based, so N commits squashed
    /// into one match none of them and read as unlanded forever. That false
    /// negative is recorded in the trajectory's known gaps, which names a
    /// merged-PR lookup as one of the two signals that would catch it. This
    /// is that signal.
    Merged {
        number: u32,
        url: String,
    },
}

impl PrState {
    pub fn number(&self) -> u32 {
        match self {
            Self::Open { number, .. }
            | Self::Closed { number, .. }
            | Self::Merged { number, .. } => *number,
        }
    }

    pub fn url(&self) -> &str {
        match self {
            Self::Open { url, .. } | Self::Closed { url, .. } | Self::Merged { url, .. } => url,
        }
    }
}

/// The row's one-line answer to "why is this still not on the trunk?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LandingStage {
    /// Nothing unlanded. The stage question does not arise.
    Landed,
    /// Committed locally and never pushed.
    Unpushed,
    /// On the remote, but nobody was ever asked to take it. The clearest
    /// stall: everything mechanical is done and one step is missing.
    PushedNoPr,
    /// A PR is open — in review, not stalled.
    InReview { number: u32, url: String },
    /// A PR was opened and closed unmerged.
    Rejected { number: u32, url: String },
    /// A PR was merged and yet commits still read as unlanded — see
    /// [`PrState::Merged`].
    MergedButUnlanded { number: u32, url: String },
    /// Push state is known, PR state is not. Never guessed: "I could not ask
    /// GitHub" and "GitHub says no PR" are different answers, and collapsing
    /// them is how a row would claim a stall that is really a review.
    PrStateUnknown { pushed: bool, why: String },
}

impl LandingStage {
    /// Pure: the row's stage from the two facts that produce it.
    pub fn derive(unlanded: u32, push: &PushState, pr: Option<&PrState>) -> Self {
        if unlanded == 0 {
            return Self::Landed;
        }
        match pr {
            Some(PrState::Open { number, url }) => Self::InReview {
                number: *number,
                url: url.clone(),
            },
            Some(PrState::Closed { number, url }) => Self::Rejected {
                number: *number,
                url: url.clone(),
            },
            Some(PrState::Merged { number, url }) => Self::MergedButUnlanded {
                number: *number,
                url: url.clone(),
            },
            None => match push {
                PushState::NotPushed => Self::Unpushed,
                PushState::Pushed | PushState::PushedStale { .. } => Self::PushedNoPr,
                PushState::Unknown => Self::PrStateUnknown {
                    pushed: false,
                    why: "Couldn't read the branch's remote-tracking ref".to_string(),
                },
            },
        }
    }

    /// Is this a stage a user would want to *do something about*?
    ///
    /// `InReview` is not stalled — it is the system working. `Rejected` is
    /// not stalled either; the answer was no. Only work that is finished and
    /// un-offered qualifies.
    pub fn is_stalled(&self) -> bool {
        matches!(self, Self::Unpushed | Self::PushedNoPr)
    }
}

/// Ask GitHub whether this branch has ever had a pull request.
///
/// The only network call in this module, and the reason [`PrState`] is a
/// separate fact from [`PushState`]: `gh` costs roughly a second per branch,
/// needs auth, and fails for reasons that say nothing about the worktree.
/// **Never call this from the git-probe tick** — it belongs on its own low
/// cadence with a cache, the same shape as the size worker.
///
/// `Ok(None)` means GitHub answered and there is no PR. `Err` means the
/// question could not be asked. Callers must keep those apart: collapsing
/// them renders an in-review branch as an un-offered stall.
///
/// Runs `gh` in the repo so it infers the remote itself rather than us
/// re-deriving a slug from a URL — one fewer thing to get wrong for SSH
/// remotes, enterprise hosts, and forks.
pub fn probe_pr_state(
    repo_path: &Path,
    branch: &str,
) -> std::result::Result<Option<PrState>, String> {
    let output = std::process::Command::new("gh")
        .current_dir(repo_path)
        .args([
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "all",
            "--limit",
            "1",
            "--json",
            "number,state,url",
        ])
        .output()
        .map_err(|e| format!("couldn't run gh: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    parse_pr_list(&String::from_utf8_lossy(&output.stdout))
}

/// Split from the subprocess so the shapes `gh` can return are testable
/// without a network or an auth token.
fn parse_pr_list(json: &str) -> std::result::Result<Option<PrState>, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json.trim()).map_err(|e| format!("unreadable gh output: {e}"))?;
    let Some(first) = parsed.as_array().and_then(|a| a.first()) else {
        return Ok(None);
    };
    let number = first
        .get("number")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "gh returned a PR with no number".to_string())? as u32;
    let url = first
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    match first.get("state").and_then(serde_json::Value::as_str) {
        Some("OPEN") => Ok(Some(PrState::Open { number, url })),
        Some("MERGED") => Ok(Some(PrState::Merged { number, url })),
        Some("CLOSED") => Ok(Some(PrState::Closed { number, url })),
        // An unrecognised state is not "no PR". Erroring keeps the row on
        // "unknown" rather than promoting a PR we failed to read into a stall.
        other => Err(format!("unrecognised PR state from gh: {other:?}")),
    }
}

/// Local: has the remote seen this branch, and is it current?
///
/// Reads remote-tracking refs already on disk, so it costs one `rev-parse`
/// and no network. Note the consequence: it reflects the last `git fetch`,
/// not GitHub right now — the same caveat the remote-drift chip already
/// carries.
pub fn probe_push_state(worktree_path: &Path, remote: &str, branch: &str) -> PushState {
    let remote_ref = format!("refs/remotes/{remote}/{branch}");
    let Some(remote_sha) = rev_parse(worktree_path, &remote_ref) else {
        return PushState::NotPushed;
    };
    let Some(head_sha) = rev_parse(worktree_path, "HEAD") else {
        return PushState::Unknown;
    };
    if remote_sha == head_sha {
        return PushState::Pushed;
    }
    match count_range(worktree_path, &format!("{remote}/{branch}..HEAD")) {
        Some(0) => PushState::Pushed,
        Some(local_ahead) => PushState::PushedStale { local_ahead },
        None => PushState::Unknown,
    }
}

fn rev_parse(path: &Path, reference: &str) -> Option<String> {
    let out = git_cmd()
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--verify", "--quiet", reference])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn count_range(path: &Path, range: &str) -> Option<u32> {
    let out = git_cmd()
        .arg("-C")
        .arg(path)
        .args(["rev-list", "--count", range])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn open_pr() -> PrState {
        PrState::Open {
            number: 452,
            url: "https://example.test/452".into(),
        }
    }

    /// The distinction the whole module exists for: a row with unlanded
    /// commits is not automatically a row with a problem.
    #[test]
    fn only_finished_but_unoffered_work_counts_as_stalled() {
        let cases: [(LandingStage, bool); 6] = [
            (LandingStage::derive(0, &PushState::NotPushed, None), false),
            (LandingStage::derive(5, &PushState::NotPushed, None), true),
            (LandingStage::derive(5, &PushState::Pushed, None), true),
            (
                LandingStage::derive(5, &PushState::Pushed, Some(&open_pr())),
                false,
            ),
            (
                LandingStage::derive(
                    5,
                    &PushState::Pushed,
                    Some(&PrState::Closed {
                        number: 18,
                        url: "u".into(),
                    }),
                ),
                false,
            ),
            (
                LandingStage::derive(
                    5,
                    &PushState::Pushed,
                    Some(&PrState::Merged {
                        number: 857,
                        url: "u".into(),
                    }),
                ),
                false,
            ),
        ];
        for (stage, stalled) in cases {
            assert_eq!(stage.is_stalled(), stalled, "{stage:?}");
        }
    }

    /// Rejected work must never read as a stall. Re-offering something whose
    /// PR was closed is worse than leaving it: it undoes a human decision.
    #[test]
    fn a_closed_pr_is_a_decision_not_a_stall() {
        let stage = LandingStage::derive(
            3,
            &PushState::Pushed,
            Some(&PrState::Closed {
                number: 18,
                url: "u".into(),
            }),
        );
        assert!(matches!(stage, LandingStage::Rejected { number: 18, .. }));
        assert!(!stage.is_stalled());
    }

    /// A merged PR whose commits still read unlanded is the squash-merge
    /// false negative the trajectory records as a known gap — surfaced as its
    /// own stage rather than mistaken for work that never landed.
    #[test]
    fn a_merged_pr_with_unlanded_commits_is_named_not_hidden() {
        let stage = LandingStage::derive(
            4,
            &PushState::Pushed,
            Some(&PrState::Merged {
                number: 857,
                url: "u".into(),
            }),
        );
        assert!(matches!(
            stage,
            LandingStage::MergedButUnlanded { number: 857, .. }
        ));
    }

    /// "I couldn't ask GitHub" is not "GitHub says there's no PR". Collapsing
    /// them would let an in-review branch render as an un-offered stall.
    #[test]
    fn an_unreadable_push_state_never_becomes_a_stall() {
        let stage = LandingStage::derive(5, &PushState::Unknown, None);
        assert!(matches!(stage, LandingStage::PrStateUnknown { .. }));
        assert!(!stage.is_stalled());
    }

    /// The three shapes `gh` returns, plus the two failure shapes that must
    /// not be mistaken for "no PR".
    #[test]
    fn gh_output_maps_to_pr_states_and_refuses_to_guess() {
        assert_eq!(parse_pr_list("[]").unwrap(), None, "no PR is a real answer");
        assert_eq!(
            parse_pr_list(r#"[{"number":452,"state":"OPEN","url":"u"}]"#).unwrap(),
            Some(PrState::Open {
                number: 452,
                url: "u".into()
            })
        );
        assert_eq!(
            parse_pr_list(r#"[{"number":857,"state":"MERGED","url":"u"}]"#).unwrap(),
            Some(PrState::Merged {
                number: 857,
                url: "u".into()
            })
        );
        assert_eq!(
            parse_pr_list(r#"[{"number":18,"state":"CLOSED","url":"u"}]"#).unwrap(),
            Some(PrState::Closed {
                number: 18,
                url: "u".into()
            })
        );
        // A state we don't know must error, not silently read as "no PR" —
        // that would turn a real PR into an un-offered stall on the row.
        assert!(parse_pr_list(r#"[{"number":1,"state":"DRAFTED","url":"u"}]"#).is_err());
        assert!(parse_pr_list("not json").is_err());
    }

    fn git(cwd: &std::path::Path, args: &[&str]) {
        let ok = git_cmd()
            .arg("-C")
            .arg(cwd)
            .args(args)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {args:?} failed in {cwd:?}");
    }

    /// Push state against a real remote, including the case an open PR makes
    /// dangerous: the branch is on the remote, but not what's on disk.
    #[test]
    fn push_state_distinguishes_never_pushed_current_and_stale() {
        let tmp = TempDir::new().unwrap();
        let origin = tmp.path().join("origin.git");
        let work = tmp.path().join("work");
        git(
            tmp.path(),
            &["init", "-q", "--bare", origin.to_str().unwrap()],
        );
        git(
            tmp.path(),
            &["init", "-q", "-b", "main", work.to_str().unwrap()],
        );
        git(&work, &["config", "user.email", "t@e.test"]);
        git(&work, &["config", "user.name", "T"]);
        git(
            &work,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        fs::write(work.join("a.txt"), "a\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-qm", "one"]);

        assert_eq!(
            probe_push_state(&work, "origin", "main"),
            PushState::NotPushed,
            "nothing pushed yet"
        );

        git(&work, &["push", "-q", "origin", "main"]);
        assert_eq!(probe_push_state(&work, "origin", "main"), PushState::Pushed);

        fs::write(work.join("b.txt"), "b\n").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-qm", "two"]);
        assert_eq!(
            probe_push_state(&work, "origin", "main"),
            PushState::PushedStale { local_ahead: 1 },
            "an open PR would be reviewing something other than what's on disk"
        );
    }
}
