//! Guarded, one-shot direct PR merge. Call blocking entry points off the UI thread.
//! GitHub enforces expectedHeadOid atomically, but offers no expected-base/policy
//! parameter. Fresh CLEAN gating follows normal gh policy; no admin override,
//! auto-merge, queue or branch-deletion operation is dispatched.
mod observe;
mod receipt;
mod submit;
#[cfg(test)]
mod tests;

use crate::{PrListRow, PrSnapshot};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How hard to press GitHub for a mergeability answer it has not computed
/// yet. `mergeable` is computed lazily: the first read of a PR GitHub has
/// not looked at recently answers `UNKNOWN` and *starts* the computation, so
/// one look is not an answer - reporting it as "not mergeable" is how
/// TASK-171 refused a PR that merged fine seconds later.
///
/// Both bounds matter, and the budget is the one that keeps this from being
/// annoying. A single `gh` call is already bounded at 30 seconds, so
/// attempts alone would let a slow network turn `m` into 91 seconds of
/// "Preparing merge" ending in the same refusal. The retry is only for the
/// case GitHub answers `UNKNOWN` *fast*, which is what it does while the
/// computation runs; once this much time is spent, the slow thing is the
/// request itself and asking again buys nothing.
#[derive(Debug, Clone, Copy)]
struct MergeabilityRetry {
    attempts: usize,
    settle: Duration,
    budget: Duration,
}

impl MergeabilityRetry {
    const LIVE: MergeabilityRetry = MergeabilityRetry {
        attempts: 3,
        settle: Duration::from_millis(700),
        budget: Duration::from_secs(5),
    };

    /// True while another look is both useful and affordable.
    fn may_retry(&self, attempt: usize, started: Instant) -> bool {
        attempt < self.attempts && started.elapsed() < self.budget
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PrMergeMethod {
    Merge,
    Squash,
    Rebase,
}
impl PrMergeMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "Merge commit",
            Self::Squash => "Squash",
            Self::Rebase => "Rebase",
        }
    }
    fn api(self) -> &'static str {
        match self {
            Self::Merge => "MERGE",
            Self::Squash => "SQUASH",
            Self::Rebase => "REBASE",
        }
    }
}

#[derive(Debug, Clone)]
pub enum PrMergePreparation {
    Ready(PreparedPrMerge),
    Disabled(String),
}

/// Private fields prevent a second frontend from forging merge authorization.
#[derive(Debug, Clone)]
pub struct PreparedPrMerge {
    observation: Box<observe::Observation>,
    host: String,
    operation_id: String,
}
impl PreparedPrMerge {
    pub fn repository(&self) -> &str {
        &self.observation.repository.name_with_owner
    }
    pub fn number(&self) -> u64 {
        self.observation.pr().number
    }
    pub fn title(&self) -> &str {
        &self.observation.pr().title
    }
    pub fn url(&self) -> &str {
        &self.observation.pr().url
    }
    pub fn head_oid(&self) -> &str {
        &self.observation.pr().head_ref_oid
    }
    pub fn base_ref(&self) -> &str {
        &self.observation.pr().base_ref_name
    }
    pub fn base_oid(&self) -> &str {
        &self.observation.pr().base_ref_oid
    }
    pub fn viewer(&self) -> &str {
        &self.observation.viewer.login
    }
    pub fn methods(&self) -> Vec<PrMergeMethod> {
        self.observation.methods()
    }
    /// What to tell the human about this merge's readiness beyond "green", so
    /// a merge GitHub allows but does not love is confirmed with that fact in
    /// view rather than behind it.
    pub fn readiness_caveat(&self) -> Option<String> {
        self.observation.readiness_caveat()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PrMergeOutcome {
    Confirmed,
    Rejected,
    OutcomeUnknown,
}
#[derive(Debug, Clone)]
pub struct PrMergeResult {
    pub outcome: PrMergeOutcome,
    pub message: String,
    pub receipt_path: Option<PathBuf>,
}

/// Refresh the exact selected identity before presenting confirmation.
pub fn prepare_pr_merge(
    repo: &Path,
    snapshot: &PrSnapshot,
    row: &PrListRow,
) -> Result<PrMergePreparation, String> {
    prepare(
        &mut observe::Github { repo },
        snapshot,
        row,
        MergeabilityRetry::LIVE,
    )
}

fn prepare(
    transport: &mut impl observe::Transport,
    snapshot: &PrSnapshot,
    row: &PrListRow,
    retry: MergeabilityRetry,
) -> Result<PrMergePreparation, String> {
    let host = observe::validate_selection(snapshot, row)?;
    let started = Instant::now();
    let mut observation = transport.observe(&host, &snapshot.repository, row.number)?;
    let mut attempt = 1;
    while observation.mergeability_pending() && retry.may_retry(attempt, started) {
        std::thread::sleep(retry.settle);
        observation = transport.observe(&host, &snapshot.repository, row.number)?;
        attempt += 1;
    }
    let pr = observation.pr();
    if observation.repository.name_with_owner != snapshot.repository
        || pr.id != row.id
        || pr.head_ref_oid != row.head_oid
        || pr.number != row.number
        || pr.url != row.url
    {
        return Ok(PrMergePreparation::Disabled(
            "PR identity or head changed; refresh and try again".into(),
        ));
    }
    if let Err(reason) = observation.eligible() {
        return Ok(PrMergePreparation::Disabled(reason));
    }
    Ok(PrMergePreparation::Ready(PreparedPrMerge {
        observation: Box::new(observation),
        host,
        operation_id: receipt::operation_id()?,
    }))
}

/// Revalidate confirmation, persist intent, dispatch exactly once, then read back.
/// Never retries an ambiguous write. A cloned preparation shares the receipt ID,
/// so a second execution cannot dispatch the same prepared operation again.
pub fn execute_pr_merge(
    repo: &Path,
    prepared: PreparedPrMerge,
    method: PrMergeMethod,
) -> PrMergeResult {
    let Some(home) = dirs::home_dir() else {
        return submit::rejected("Cannot locate merge receipt directory");
    };
    submit::execute(
        &mut observe::Github { repo },
        prepared,
        method,
        &home.join(".switchbard/pr-merge"),
    )
}
