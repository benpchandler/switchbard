//! Read-only, bounded open-PR observations for one resolved GitHub repository.
//! Local task associations belong to generic reference matching, never title guessing.
mod parse;
mod process;

use std::path::Path;
use std::time::SystemTime;

pub const MAX_PULL_REQUESTS: usize = 100;

#[derive(Debug, Clone)]
pub struct PrSnapshot {
    pub repository: String,
    pub repository_url: String,
    pub observed_at: SystemTime,
    pub rows: Vec<PrListRow>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrListRow {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub head_oid: String,
    pub draft: bool,
    pub checks: PrChecks,
    pub review: PrReview,
    pub merge: PrMerge,
}

impl PrListRow {
    /// Inspection ordering only, not readiness or ownership of the next action.
    pub fn attention_rank(&self) -> u8 {
        if self.checks == PrChecks::Failed
            || self.merge == PrMerge::Conflicting
            || self.review == PrReview::ChangesRequested
        {
            0
        } else if matches!(self.checks, PrChecks::Unknown | PrChecks::NoneObserved)
            || self.merge == PrMerge::Unknown
            || self.review == PrReview::Unknown
        {
            1
        } else if self.checks == PrChecks::Running || self.review == PrReview::Required {
            2
        } else {
            3
        }
    }
}

/// Observed head-commit checks, never overall delivery or merge readiness.
/// `Passing` means all returned contexts passed; required-check coverage is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrChecks {
    Unknown,
    NoneObserved,
    Passing,
    Running,
    Failed,
}

impl PrChecks {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Checks unknown",
            Self::NoneObserved => "No checks observed",
            Self::Passing => "Observed checks pass",
            Self::Running => "Checks pending",
            Self::Failed => "Checks failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrReview {
    Unknown,
    Required,
    Approved,
    ChangesRequested,
}

impl PrReview {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Review unknown",
            Self::Required => "Review required",
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes requested",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrMerge {
    Unknown,
    Mergeable,
    Conflicting,
}

impl PrMerge {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Mergeability unknown",
            Self::Mergeable => "No merge conflict",
            Self::Conflicting => "Merge conflict",
        }
    }
}

/// Blocks for at most two bounded `gh` queries. Invoke on a background worker.
/// Auth, malformed data and inaccessible repositories return errors, never empty rows.
pub fn fetch_pull_requests(repo: &Path) -> Result<PrSnapshot, String> {
    let identity = process::run_gh(repo, &["repo", "view", "--json", "nameWithOwner,url"])?;
    let (repository, repository_url) = parse::repository(&identity)?;
    let locator = repository_url.trim_start_matches("https://");
    let data = process::run_gh(repo, &[
        "pr", "list", "--repo", locator, "--state", "open", "--limit", "101", "--json",
        "id,number,title,url,state,headRefOid,isDraft,statusCheckRollup,reviewDecision,mergeable",
    ])?;
    let (rows, truncated) = parse::rows(&data, &repository_url)?;
    Ok(PrSnapshot {
        repository,
        repository_url,
        observed_at: SystemTime::now(),
        rows,
        truncated,
    })
}
