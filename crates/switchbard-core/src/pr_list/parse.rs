use super::{PrChecks, PrLifecycle, PrListRow, PrMerge, PrReview};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    name_with_owner: String,
    url: String,
}

pub(super) fn repository(data: &[u8]) -> Result<(String, String), String> {
    let repo: Repository =
        serde_json::from_slice(data).map_err(|e| format!("Invalid GitHub repository: {e}"))?;
    let parts: Vec<_> = repo.name_with_owner.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| !valid_component(p)) {
        return Err("Invalid GitHub repository name".into());
    }
    // This first slice deliberately supports github.com only. Do not send another
    // host's locator to github.com or reinterpret it as the active repository.
    if repo.url != format!("https://github.com/{}", repo.name_with_owner) {
        return Err("Repository is not a canonical github.com repository".into());
    }
    Ok((repo.name_with_owner, repo.url))
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPr {
    id: String,
    number: u64,
    title: String,
    url: String,
    state: String,
    head_ref_oid: String,
    is_draft: bool,
    merged_at: Option<String>,
    status_check_rollup: Option<Vec<Check>>,
    review_decision: Option<String>,
    mergeable: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    #[serde(rename = "__typename")]
    kind: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    state: Option<String>,
    head_sha: Option<String>,
}

pub(super) fn rows(
    data: &[u8],
    repo_url: &str,
    limit: usize,
) -> Result<(Vec<PrListRow>, bool), String> {
    super::validate_limit(limit)?;
    let raw: Vec<RawPr> =
        serde_json::from_slice(data).map_err(|e| format!("Invalid GitHub PR list: {e}"))?;
    if raw.len() > limit + 1 {
        return Err("GitHub PR list exceeded requested row limit".into());
    }
    let truncated = raw.len() > limit;
    let mut ids = std::collections::HashSet::new();
    let mut numbers = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(raw.len().min(limit));
    for pr in raw.into_iter().take(limit + 1) {
        if !ids.insert(pr.id.clone()) || !numbers.insert(pr.number) {
            return Err("Duplicate GitHub PR identity".into());
        }
        let row = row(pr, repo_url)?;
        if result.len() < limit {
            result.push(row);
        }
    }
    Ok((result, truncated))
}

fn row(pr: RawPr, repo_url: &str) -> Result<PrListRow, String> {
    if pr.id.is_empty()
        || pr.id.len() > 256
        || pr.number == 0
        || pr.url != format!("{repo_url}/pull/{}", pr.number)
        || !matches!(pr.head_ref_oid.len(), 40 | 64)
        || !pr.head_ref_oid.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("GitHub returned an invalid or mismatched PR identity".into());
    }
    let title: String = pr
        .title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(1024)
        .collect();
    Ok(PrListRow {
        checks: checks(pr.status_check_rollup.as_deref(), &pr.head_ref_oid),
        review: review(pr.review_decision.as_deref()),
        merge: merge(pr.mergeable.as_deref()),
        id: pr.id,
        number: pr.number,
        title,
        url: pr.url,
        head_oid: pr.head_ref_oid,
        draft: pr.is_draft,
        lifecycle: lifecycle(&pr.state)?,
        merged_at: pr
            .merged_at
            .as_deref()
            .filter(|_| pr.state == "MERGED")
            .and_then(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .ok()
                    .map(|instant| instant.with_timezone(&chrono::Utc))
            }),
    })
}

fn lifecycle(value: &str) -> Result<PrLifecycle, String> {
    match value {
        "OPEN" => Ok(PrLifecycle::Open),
        "CLOSED" => Ok(PrLifecycle::Closed),
        "MERGED" => Ok(PrLifecycle::Merged),
        _ => Err("GitHub returned an unknown PR lifecycle state".into()),
    }
}

fn review(value: Option<&str>) -> PrReview {
    match value {
        Some("REVIEW_REQUIRED") => PrReview::Required,
        Some("APPROVED") => PrReview::Approved,
        Some("CHANGES_REQUESTED") => PrReview::ChangesRequested,
        _ => PrReview::Unknown,
    }
}

fn merge(value: Option<&str>) -> PrMerge {
    match value {
        Some("MERGEABLE") => PrMerge::Mergeable,
        Some("CONFLICTING") => PrMerge::Conflicting,
        _ => PrMerge::Unknown,
    }
}

fn checks(items: Option<&[Check]>, head: &str) -> PrChecks {
    let Some(items) = items else {
        return PrChecks::Unknown;
    };
    if items.is_empty() {
        return PrChecks::NoneObserved;
    }
    // gh fetches contexts from commits(last:1) in the same PR query. Its JSON
    // projection drops pageInfo, so 100 contexts may be truncated: never pass.
    // https://github.com/cli/cli/blob/trunk/api/query_builder.go
    let mut summary = if items.len() >= 100 {
        PrChecks::Unknown
    } else {
        PrChecks::Passing
    };
    for check in items.iter().take(100) {
        let state = check_state(check, head);
        summary = match (summary, state) {
            (PrChecks::Failed, _) | (_, PrChecks::Failed) => PrChecks::Failed,
            (PrChecks::Unknown, _) | (_, PrChecks::Unknown) => PrChecks::Unknown,
            (PrChecks::Running, _) | (_, PrChecks::Running) => PrChecks::Running,
            _ => PrChecks::Passing,
        };
    }
    summary
}

fn check_state(check: &Check, head: &str) -> PrChecks {
    if check.head_sha.as_deref().is_some_and(|sha| sha != head) {
        return PrChecks::Unknown;
    }
    match check.kind.as_deref() {
        Some("StatusContext") => match check.state.as_deref() {
            Some("SUCCESS") => PrChecks::Passing,
            Some("ERROR" | "FAILURE") => PrChecks::Failed,
            Some("PENDING" | "EXPECTED") => PrChecks::Running,
            _ => PrChecks::Unknown,
        },
        Some("CheckRun") => check_run(check),
        _ => PrChecks::Unknown,
    }
}

fn check_run(check: &Check) -> PrChecks {
    match (check.status.as_deref(), check.conclusion.as_deref()) {
        (Some("COMPLETED"), Some("SUCCESS")) => PrChecks::Passing,
        (
            Some("COMPLETED"),
            Some(
                "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE"
                | "STALE",
            ),
        ) => PrChecks::Failed,
        (Some("QUEUED" | "IN_PROGRESS" | "WAITING" | "PENDING" | "REQUESTED"), _) => {
            PrChecks::Running
        }
        _ => PrChecks::Unknown,
    }
}

#[cfg(test)]
mod tests;
