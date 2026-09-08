//! Read-only, bounded PR observations across every lifecycle state for one resolved GitHub repository.
//! Local task associations belong to generic reference matching, never title guessing.
mod enrich;
mod parse;
pub(crate) mod process;

use std::path::Path;
use std::time::SystemTime;

/// Default observation window; the UI may progressively request more rows.
pub const DEFAULT_PULL_REQUEST_LIMIT: usize = 100;
/// Hard allocation/query row bound, independently protected by the 4 MiB stream cap.
pub const MAX_PULL_REQUESTS: usize = 1000;

#[derive(Debug, Clone)]
pub struct PrSnapshot {
    pub repository: String,
    pub repository_url: String,
    pub observed_at: SystemTime,
    pub rows: Vec<PrListRow>,
    pub truncated: bool,
    pub limit: usize,
    /// Metadata succeeded, but optional active-PR delivery observations are incomplete.
    pub enrichment_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrListRow {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub head_oid: String,
    pub draft: bool,
    pub lifecycle: PrLifecycle,
    /// Authoritative GitHub merge instant, normalized to UTC. Missing or invalid
    /// metadata remains unknown; neither closure nor update time implies a merge.
    pub merged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub checks: PrChecks,
    pub review: PrReview,
    pub merge: PrMerge,
}

impl PrListRow {
    /// Inspection ordering only, not readiness or ownership of the next action.
    pub fn attention_rank(&self) -> u8 {
        if self.lifecycle != PrLifecycle::Open {
            return 4;
        }
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

/// GitHub lifecycle observation; task completion remains independently owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrLifecycle {
    Open,
    Closed,
    Merged,
}

impl PrLifecycle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Closed => "Closed",
            Self::Merged => "Merged",
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

/// Blocks for at most three bounded `gh` queries. Invoke on a background worker.
/// Auth, malformed data and inaccessible repositories return errors, never empty rows.
pub fn fetch_pull_requests(repo: &Path) -> Result<PrSnapshot, String> {
    fetch_pull_requests_with_limit(repo, DEFAULT_PULL_REQUEST_LIMIT)
}

/// Observe every lifecycle, retaining at most `limit` rows (1 through 1000).
/// One extra row detects truncation, including at the hard cap. Increasing the
/// limit refetches the bounded window; it does not append inconsistent pages.
/// Historical rows carry unknown delivery details; a separate bounded read enriches
/// open rows by opaque identity and the same observed head revision. Failure of
/// that optional read leaves metadata usable with an explicit warning.
/// The 4 MiB output bound may reject large observations before the row limit.
pub fn fetch_pull_requests_with_limit(repo: &Path, limit: usize) -> Result<PrSnapshot, String> {
    validate_limit(limit)?;
    let identity = process::run_gh(repo, &["repo", "view", "--json", "nameWithOwner,url"])?;
    let (repository, repository_url) = parse::repository(&identity)?;
    let locator = repository_url.trim_start_matches("https://");
    let query_limit = (limit + 1).to_string();
    let data = process::run_gh(
        repo,
        &[
            "pr",
            "list",
            "--repo",
            locator,
            "--state",
            "all",
            "--limit",
            &query_limit,
            "--json",
            "id,number,title,url,state,headRefOid,isDraft,mergedAt",
        ],
    )?;
    let (rows, truncated) = parse::rows(&data, &repository_url, limit)?;
    let mut snapshot = PrSnapshot {
        repository,
        repository_url,
        observed_at: SystemTime::now(),
        rows,
        truncated,
        limit,
        enrichment_warning: None,
    };
    snapshot.enrichment_warning = enrich::fetch(repo, &mut snapshot.rows, &snapshot.repository_url);
    Ok(snapshot)
}

fn validate_limit(limit: usize) -> Result<(), String> {
    if !(1..=MAX_PULL_REQUESTS).contains(&limit) {
        return Err(format!(
            "PR observation limit must be between 1 and {MAX_PULL_REQUESTS}"
        ));
    }
    Ok(())
}
