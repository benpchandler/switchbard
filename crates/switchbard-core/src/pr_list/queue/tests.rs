use super::*;
use serde_json::json;

fn metadata(number: u64, state: &str) -> PrListRow {
    let data = json!([{"id":format!("PR_{number}"), "number":number, "title":"Queue",
        "url":format!("https://github.com/owner/repo/pull/{number}"), "state":state,
        "headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "isDraft":false}]);
    crate::pr_list::parse::rows(
        &serde_json::to_vec(&data).expect("fixture"),
        "https://github.com/owner/repo",
        100,
    )
    .expect("valid metadata")
    .0
    .remove(0)
}

fn response(entry: serde_json::Value) -> serde_json::Value {
    json!({"data":{"nodes":[{"id":"PR_1", "number":1,
        "url":"https://github.com/owner/repo/pull/1", "state":"OPEN",
        "headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "mergeQueueEntry":entry}]}})
}

fn decoded(data: serde_json::Value) -> Result<Vec<Option<Observation>>, String> {
    parse(&serde_json::to_vec(&data).expect("fixture"))
}

#[test]
fn queue_membership_refines_status_but_never_lifecycle_checks_or_merged_state() {
    let mut rows = vec![metadata(1, "OPEN")];
    assert!(apply(&mut rows, decoded(response(json!({"id":"MQ_1"})))).is_none());
    assert_eq!(rows[0].status_label(), "Queued");
    assert_eq!(rows[0].lifecycle, PrLifecycle::Open);
    assert_eq!(rows[0].checks, crate::PrChecks::Unknown);
    assert!(apply(&mut rows, decoded(response(json!(null)))).is_none());
    assert_eq!(rows[0].merge_queue, PrMergeQueue::NotQueued);
    assert_eq!(rows[0].status_label(), "Open");
    for state in [PrLifecycle::Closed, PrLifecycle::Merged] {
        rows[0].lifecycle = state;
        rows[0].merge_queue = PrMergeQueue::Queued;
        assert_eq!(rows[0].status_label(), state.label());
        assert!(apply(&mut rows, decoded(response(json!({"id":"MQ_1"})))).is_none());
        assert_eq!(rows[0].merge_queue, PrMergeQueue::Unknown);
    }
}

#[test]
fn changed_identity_revision_state_or_missing_node_is_unknown() {
    for (field, value) in [
        ("id", json!("OTHER")),
        ("number", json!(2)),
        ("url", json!("https://github.com/other/repo/pull/1")),
        (
            "headRefOid",
            json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        ),
        ("state", json!("MERGED")),
    ] {
        let mut data = response(json!({"id":"MQ_1"}));
        data["data"]["nodes"][0][field] = value;
        let mut rows = vec![metadata(1, "OPEN")];
        assert!(apply(&mut rows, decoded(data)).is_some());
        assert_eq!(rows[0].merge_queue, PrMergeQueue::Unknown);
    }
    let mut rows = vec![metadata(1, "OPEN")];
    assert!(apply(&mut rows, decoded(json!({"data":{"nodes":[null]}}))).is_some());
}

#[test]
fn failure_partial_errors_missing_field_and_overflow_never_claim_dequeued() {
    let mut missing = response(json!(null));
    missing["data"]["nodes"][0]
        .as_object_mut()
        .expect("node")
        .remove("mergeQueueEntry");
    let mut errors = response(json!(null));
    errors["errors"] = json!([{"message":"Permission denied"}]);
    let mut duplicate = response(json!(null));
    duplicate["data"]["nodes"] =
        json!([duplicate["data"]["nodes"][0], duplicate["data"]["nodes"][0]]);
    for data in [
        missing,
        errors,
        duplicate,
        json!({"data":null}),
        response(json!({"id":""})),
        json!({"data":{"nodes":vec![serde_json::Value::Null; LIMIT+1]}}),
    ] {
        let mut rows = vec![metadata(1, "OPEN")];
        rows[0].merge_queue = PrMergeQueue::Queued;
        assert!(apply(&mut rows, decoded(data)).is_some());
        assert_eq!(rows[0].merge_queue, PrMergeQueue::Unknown);
    }
    let mut rows = vec![metadata(1, "OPEN")];
    assert!(apply(&mut rows, Err("timeout".into()))
        .expect("warning")
        .contains("timeout"));
}

#[test]
fn partial_queue_coverage_preserves_check_observations_and_open_status() {
    let mut rows: Vec<_> = (1..=101).map(|number| metadata(number, "OPEN")).collect();
    for row in &mut rows {
        row.checks = crate::PrChecks::Running;
    }
    let warning =
        apply(&mut rows, decoded(response(json!({"id":"MQ_1"})))).expect("partial warning");
    assert!(warning.contains("100 unmatched"));
    assert_eq!(rows[0].status_label(), "Queued");
    assert!(rows[1..]
        .iter()
        .all(|row| row.status_label() == "Open" && row.merge_queue == PrMergeQueue::Unknown));
    assert!(rows
        .iter()
        .all(|row| row.checks == crate::PrChecks::Running));
    rows[1].checks = crate::PrChecks::Passing;
    assert_eq!(rows[1].status_label(), "Open");
}
