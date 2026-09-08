//! Confirmation revalidation and explicit ambiguous-outcome reconciliation.
use super::{
    observe::{Observation, Transport},
    receipt::Receipt,
    PrMergeMethod, PrMergeOutcome, PrMergeResult, PreparedPrMerge,
};
use std::path::Path;

pub(super) fn rejected(message: impl Into<String>) -> PrMergeResult {
    PrMergeResult {
        outcome: PrMergeOutcome::Rejected,
        message: message.into(),
        receipt_path: None,
    }
}
pub(super) fn execute(
    transport: &mut impl Transport,
    prepared: PreparedPrMerge,
    method: PrMergeMethod,
    directory: &Path,
) -> PrMergeResult {
    if let Err(reason) = revalidate(transport, &prepared, method) {
        return rejected(reason);
    }
    let mut receipt = match Receipt::create(directory, &prepared, method) {
        Ok(receipt) => receipt,
        Err(reason) => return rejected(reason),
    };
    let submission = transport.merge(
        &prepared.host,
        &prepared.observation.pr().id,
        prepared.head_oid(),
        method,
    );
    let readback = transport.observe(&prepared.host, prepared.repository(), prepared.number());
    let mut result = reconcile(&prepared, submission, readback);
    result.receipt_path = Some(receipt.path.clone());
    if let Err(error) = receipt.finish(&result) {
        result.message = format!(
            "{}; result receipt failed: {error}. Original intent retained",
            result.message
        );
    }
    result
}
fn revalidate(
    transport: &mut impl Transport,
    prepared: &PreparedPrMerge,
    method: PrMergeMethod,
) -> Result<(), String> {
    if !prepared.methods().contains(&method) {
        return Err("Selected merge method was not offered".into());
    }
    let current = transport.observe(&prepared.host, prepared.repository(), prepared.number())?;
    current.eligible()?;
    let before = &prepared.observation;
    let (a, b) = (before.pr(), current.pr());
    if before.viewer.login != current.viewer.login
        || before.repository.id != current.repository.id
        || before.repository.name_with_owner != current.repository.name_with_owner
        || a.id != b.id
        || a.number != b.number
        || a.url != b.url
        || a.head_ref_oid != b.head_ref_oid
        || a.base_ref_name != b.base_ref_name
        || a.base_ref_oid != b.base_ref_oid
    {
        return Err("PR head, base, repository or account changed; confirm again".into());
    }
    if !current.methods().contains(&method) {
        return Err("Selected merge method is no longer enabled".into());
    }
    Ok(())
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MutationData {
    merge_pull_request: MutationPayload,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MutationPayload {
    pull_request: MergedPullRequest,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergedPullRequest {
    id: String,
    state: String,
    head_ref_oid: String,
    merge_commit: Option<super::observe::Commit>,
}
fn accepted_commit(bytes: &[u8], prepared: &PreparedPrMerge) -> Option<String> {
    let data: MutationData = super::observe::decode(bytes).ok()?;
    let pr = data.merge_pull_request.pull_request;
    if pr.id != prepared.observation.pr().id
        || pr.state != "MERGED"
        || pr.head_ref_oid != prepared.head_oid()
    {
        return None;
    }
    pr.merge_commit
        .map(|commit| commit.oid)
        .filter(|oid| !oid.is_empty())
}

fn reconcile(
    prepared: &PreparedPrMerge,
    submission: Result<Vec<u8>, String>,
    readback: Result<Observation, String>,
) -> PrMergeResult {
    let accepted = submission
        .as_ref()
        .ok()
        .and_then(|bytes| accepted_commit(bytes, prepared));
    if let (Some(commit), Ok(current)) = (accepted, &readback) {
        let pr = current.pr();
        if current.repository.id == prepared.observation.repository.id
            && pr.id == prepared.observation.pr().id
            && pr.state == "MERGED"
            && pr.head_ref_oid == prepared.head_oid()
            && pr
                .merge_commit
                .as_ref()
                .is_some_and(|merged| merged.oid == commit)
        {
            return PrMergeResult {
                outcome: PrMergeOutcome::Confirmed,
                message: format!(
                    "Merged {}#{}; GitHub readback confirmed {commit}",
                    prepared.repository(),
                    prepared.number()
                ),
                receipt_path: None,
            };
        }
    }
    let detail = match (submission, readback) {
        (Err(error), _) => error,
        (_, Err(error)) => format!("Readback failed: {error}"),
        _ => "Merge response and readback did not confirm the same revision".into(),
    };
    PrMergeResult {
        outcome: PrMergeOutcome::OutcomeUnknown,
        message: format!("Merge outcome unknown: {detail}. Check GitHub before trying again"),
        receipt_path: None,
    }
}
