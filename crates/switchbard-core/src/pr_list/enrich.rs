//! Optional bounded delivery reads for open PRs. Metadata remains usable if this fails.
use super::{
    parse, process, PrChecks, PrLifecycle, PrListRow, PrMerge, PrReview, MAX_PULL_REQUESTS,
};
use std::path::Path;

const ENRICHMENT_LIMIT: usize = 100;

pub(super) fn fetch(repo: &Path, rows: &mut [PrListRow], repo_url: &str) -> Option<String> {
    if !rows
        .iter()
        .take(MAX_PULL_REQUESTS)
        .any(|row| row.lifecycle == PrLifecycle::Open)
    {
        return None;
    }
    let locator = repo_url.trim_start_matches("https://");
    let result = process::run_gh(repo, &[
        "pr", "list", "--repo", locator, "--state", "open", "--limit", "101", "--json",
        "id,number,title,url,state,headRefOid,isDraft,statusCheckRollup,reviewDecision,mergeable",
    ]).and_then(|data| parse::rows(&data, repo_url, ENRICHMENT_LIMIT));
    apply(rows, result)
}

fn apply(rows: &mut [PrListRow], result: Result<(Vec<PrListRow>, bool), String>) -> Option<String> {
    for row in rows.iter_mut().take(MAX_PULL_REQUESTS) {
        row.checks = PrChecks::Unknown;
        row.review = PrReview::Unknown;
        row.merge = PrMerge::Unknown;
    }
    let (observations, partial) = match result {
        Ok(value) => value,
        Err(error) => return Some(format!("Delivery details unavailable: {error}")),
    };
    if observations
        .iter()
        .take(ENRICHMENT_LIMIT)
        .any(|row| row.lifecycle != PrLifecycle::Open)
    {
        return Some(
            "Delivery details changed during refresh; active delivery remains unknown".into(),
        );
    }
    let missing = join(rows, &observations);
    if partial || missing > 0 {
        return Some(format!("Delivery detail coverage incomplete (first 100 open PRs); {missing} loaded open PRs unmatched or changed revision"));
    }
    None
}

fn join(rows: &mut [PrListRow], observations: &[PrListRow]) -> usize {
    let mut missing = 0;
    for row in rows
        .iter_mut()
        .take(MAX_PULL_REQUESTS)
        .filter(|row| row.lifecycle == PrLifecycle::Open)
    {
        let observation = observations.iter().take(ENRICHMENT_LIMIT).find(|pr| {
            pr.id == row.id
                && pr.head_oid == row.head_oid
                && pr.number == row.number
                && pr.url == row.url
        });
        if let Some(observation) = observation {
            row.checks = observation.checks;
            row.review = observation.review;
            row.merge = observation.merge;
        } else {
            missing += 1;
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(number: u64, state: &str) -> PrListRow {
        let data = json!([{"id":format!("PR_{number}"),"number":number,"title":"Title",
            "url":format!("https://github.com/owner/repo/pull/{number}"),"state":state,
            "headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","isDraft":false}]);
        parse::rows(
            &serde_json::to_vec(&data).expect("fixture"),
            "https://github.com/owner/repo",
            100,
        )
        .expect("metadata without delivery fields")
        .0
        .remove(0)
    }

    #[test]
    fn optional_failure_preserves_metadata_and_never_reuses_delivery() {
        let mut rows = vec![row(1, "OPEN"), row(2, "MERGED")];
        rows[0].checks = PrChecks::Passing;
        let warning = apply(&mut rows, Err("timeout".into()));
        assert!(warning.expect("warning").contains("timeout"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].checks, PrChecks::Unknown);
        assert_eq!(rows[1].lifecycle, PrLifecycle::Merged);
    }

    #[test]
    fn joining_requires_identity_and_revision_and_never_adds_remote_rows() {
        let mut rows = vec![row(1, "OPEN"), row(2, "CLOSED"), row(3, "OPEN")];
        let mut observed = vec![
            row(1, "OPEN"),
            row(2, "OPEN"),
            row(3, "OPEN"),
            row(4, "OPEN"),
        ];
        for pr in &mut observed {
            pr.checks = PrChecks::Passing;
        }
        observed[2].head_oid = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
        assert!(apply(&mut rows, Ok((observed, false))).is_some());
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].checks, PrChecks::Passing);
        assert_eq!(rows[1].checks, PrChecks::Unknown);
        assert_eq!(rows[2].checks, PrChecks::Unknown);
    }

    #[test]
    fn successful_matching_enrichment_is_distinct_from_partial_or_changed_lifecycle() {
        let mut rows = vec![row(1, "OPEN")];
        assert!(apply(&mut rows, Ok((vec![row(1, "OPEN")], false))).is_none());
        assert!(apply(&mut rows, Ok((vec![row(1, "OPEN")], true))).is_some());
        let mut changed = row(1, "MERGED");
        changed.checks = PrChecks::Passing;
        assert!(apply(&mut rows, Ok((vec![changed], false))).is_some());
        assert_eq!(rows[0].checks, PrChecks::Unknown);
    }
}
