//! Optional merge queue observations, bounded independently of check enrichment.
use super::{process, PrLifecycle, PrListRow, PrMergeQueue, MAX_PULL_REQUESTS};
use serde::Deserialize;
use std::path::Path;

const LIMIT: usize = 100;
const QUERY: &str = "query($ids: [ID!]!) { nodes(ids: $ids) { ... on PullRequest { id number url headRefOid state mergeQueueEntry { id } } } }";

pub(super) fn fetch(repo: &Path, rows: &mut [PrListRow]) -> Option<String> {
    let ids: Vec<_> = rows
        .iter()
        .take(MAX_PULL_REQUESTS)
        .filter(|row| row.lifecycle == PrLifecycle::Open)
        .take(LIMIT)
        .map(|row| row.id.as_str())
        .collect();
    if ids.is_empty() {
        return None;
    }
    let mut args = vec![
        "api".into(),
        "graphql".into(),
        "--hostname".into(),
        "github.com".into(),
        "--raw-field".into(),
        format!("query={QUERY}"),
    ];
    for id in ids {
        args.extend(["--raw-field".into(), format!("ids[]={id}")]);
    }
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let result = process::run_gh(repo, &args).and_then(|data| parse(&data));
    apply(rows, result)
}

#[derive(Deserialize)]
struct Response {
    data: Option<Data>,
    errors: Option<Vec<serde::de::IgnoredAny>>,
}
#[derive(Deserialize)]
struct Data {
    nodes: Vec<Option<Observation>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Observation {
    id: String,
    number: u64,
    url: String,
    head_ref_oid: String,
    state: String,
    merge_queue_entry: Entry,
}

// A required nullable field distinguishes explicit null from an omitted observation.
#[derive(Deserialize)]
#[serde(untagged)]
enum Entry {
    Queued { id: String },
    NotQueued(()),
}

fn parse(data: &[u8]) -> Result<Vec<Option<Observation>>, String> {
    let response: Response = serde_json::from_slice(data)
        .map_err(|error| format!("Invalid merge queue response: {error}"))?;
    if response.errors.is_some_and(|errors| !errors.is_empty()) {
        return Err("GitHub merge queue query returned errors".into());
    }
    let nodes = response
        .data
        .ok_or("GitHub merge queue data unavailable")?
        .nodes;
    if nodes.len() > LIMIT {
        return Err("Merge queue response exceeded row limit".into());
    }
    let mut ids = std::collections::HashSet::new();
    for node in nodes.iter().flatten() {
        if !ids.insert(&node.id)
            || matches!(&node.merge_queue_entry, Entry::Queued { id } if id.is_empty())
        {
            return Err("Invalid or duplicate merge queue identity".into());
        }
    }
    Ok(nodes)
}

fn apply(
    rows: &mut [PrListRow],
    result: Result<Vec<Option<Observation>>, String>,
) -> Option<String> {
    for row in rows.iter_mut().take(MAX_PULL_REQUESTS) {
        row.merge_queue = PrMergeQueue::Unknown;
    }
    let observations = match result {
        Ok(value) => value,
        Err(error) => return Some(format!("Merge queue unavailable: {error}")),
    };
    let missing = join(rows, &observations);
    (missing > 0).then(|| format!("Merge queue coverage incomplete (up to {LIMIT} loaded open PRs); {missing} unmatched or changed"))
}

fn join(rows: &mut [PrListRow], observations: &[Option<Observation>]) -> usize {
    let mut missing = 0;
    for row in rows
        .iter_mut()
        .take(MAX_PULL_REQUESTS)
        .filter(|row| row.lifecycle == PrLifecycle::Open)
    {
        let found = observations.iter().take(LIMIT).flatten().find(|observed| {
            observed.id == row.id
                && observed.number == row.number
                && observed.url == row.url
                && observed.head_ref_oid == row.head_oid
                && observed.state == "OPEN"
        });
        match found {
            Some(observed) => {
                row.merge_queue = match observed.merge_queue_entry {
                    Entry::Queued { .. } => PrMergeQueue::Queued,
                    Entry::NotQueued(()) => PrMergeQueue::NotQueued,
                }
            }
            None => missing += 1,
        }
    }
    missing
}

#[cfg(test)]
mod tests;
