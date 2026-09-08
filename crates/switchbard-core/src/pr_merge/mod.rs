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
    prepare(&mut observe::Github { repo }, snapshot, row)
}

fn prepare(
    transport: &mut impl observe::Transport,
    snapshot: &PrSnapshot,
    row: &PrListRow,
) -> Result<PrMergePreparation, String> {
    let host = observe::validate_selection(snapshot, row)?;
    let observation = transport.observe(&host, &snapshot.repository, row.number)?;
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
