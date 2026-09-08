use super::*;
use observe::{Observation, Transport};
use serde_json::{json, Value};
use std::collections::VecDeque;

fn fixture() -> Value {
    json!({"viewer":{"login":"maintainer"},"repository":{"id":"R_1","nameWithOwner":"owner/repo",
        "viewerPermission":"ADMIN","mergeCommitAllowed":true,"squashMergeAllowed":true,"rebaseMergeAllowed":false,
        "pullRequest":{"id":"PR_1","number":1,"title":"A change","url":"https://github.com/owner/repo/pull/1",
            "state":"OPEN","isDraft":false,"headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "baseRefName":"main","baseRefOid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","reviewDecision":null,
            "isMergeQueueEnabled":false,"mergeCommit":null}}})
}
fn observation(value: Value) -> Observation {
    serde_json::from_value(value).expect("typed fixture")
}
fn prepared() -> PreparedPrMerge {
    PreparedPrMerge {
        observation: Box::new(observation(fixture())),
        host: "github.com".into(),
        operation_id: receipt::operation_id().expect("clock"),
    }
}
struct Fake {
    observations: VecDeque<Result<Observation, String>>,
    submission: Result<Vec<u8>, String>,
    dispatched: Vec<(String, String, PrMergeMethod)>,
    receipt_directory: std::path::PathBuf,
}
impl Transport for Fake {
    fn observe(&mut self, _: &str, _: &str, _: u64) -> Result<Observation, String> {
        self.observations
            .pop_front()
            .expect("bounded expected observation")
    }
    fn merge(
        &mut self,
        _: &str,
        id: &str,
        head: &str,
        method: PrMergeMethod,
    ) -> Result<Vec<u8>, String> {
        assert_eq!(
            std::fs::read_dir(&self.receipt_directory)
                .expect("durable intent exists")
                .count(),
            1
        );
        self.dispatched.push((id.into(), head.into(), method));
        self.submission.clone()
    }
}
fn fake(directory: &Path) -> Fake {
    let mut merged = fixture();
    merged["repository"]["pullRequest"]["state"] = json!("MERGED");
    merged["repository"]["pullRequest"]["mergeCommit"] =
        json!({"oid":"cccccccccccccccccccccccccccccccccccccccc"});
    Fake { observations: VecDeque::from([Ok(observation(fixture())), Ok(observation(merged.clone()))]),
        submission: Ok(serde_json::to_vec(&json!({"data":{"mergePullRequest":{"pullRequest":merged["repository"]["pullRequest"]}}})).expect("response")),
        dispatched: vec![], receipt_directory: directory.into() }
}
#[test]
fn policy_blocks_incomplete_and_unsafe_states_but_allows_clean_admin_without_required_review() {
    assert!(observation(fixture()).eligible().is_ok());
    for (field, value) in [
        ("state", json!("CLOSED")),
        ("isDraft", json!(true)),
        ("mergeable", json!("UNKNOWN")),
        ("mergeStateStatus", json!("UNSTABLE")),
        ("mergeStateStatus", json!("BLOCKED")),
        ("mergeStateStatus", json!("BEHIND")),
        ("reviewDecision", json!("REVIEW_REQUIRED")),
        ("reviewDecision", json!("CHANGES_REQUESTED")),
        ("reviewDecision", json!("NEW_UNKNOWN_VALUE")),
        ("isMergeQueueEnabled", json!(true)),
    ] {
        let mut value_fixture = fixture();
        value_fixture["repository"]["pullRequest"][field] = value;
        assert!(observation(value_fixture).eligible().is_err(), "{field}");
    }
    let mut value = fixture();
    value["repository"]["viewerPermission"] = json!("READ");
    assert!(observation(value).eligible().is_err());
}
#[test]
fn successful_dispatch_binds_head_and_persists_intent_before_write_and_confirmed_readback() {
    let directory = tempfile::tempdir().expect("directory");
    let mut transport = fake(directory.path());
    let result = submit::execute(
        &mut transport,
        prepared(),
        PrMergeMethod::Squash,
        directory.path(),
    );
    assert_eq!(result.outcome, PrMergeOutcome::Confirmed);
    assert_eq!(
        transport.dispatched,
        vec![("PR_1".into(), "a".repeat(40), PrMergeMethod::Squash)]
    );
    let receipt = std::fs::read_to_string(result.receipt_path.expect("path")).expect("receipt");
    assert!(receipt.contains("dispatch_intent"));
    assert!(receipt.contains("Confirmed"));
}
#[test]
fn disallowed_method_and_changed_identity_never_dispatch() {
    let directory = tempfile::tempdir().expect("directory");
    let mut transport = fake(directory.path());
    assert_eq!(
        submit::execute(
            &mut transport,
            prepared(),
            PrMergeMethod::Rebase,
            directory.path()
        )
        .outcome,
        PrMergeOutcome::Rejected
    );
    for (field, value) in [
        ("headRefOid", json!("d".repeat(40))),
        ("baseRefOid", json!("d".repeat(40))),
        ("baseRefName", json!("release")),
        ("id", json!("PR_2")),
    ] {
        let mut changed = fixture();
        changed["repository"]["pullRequest"][field] = value;
        transport.observations = VecDeque::from([Ok(observation(changed))]);
        assert_eq!(
            submit::execute(
                &mut transport,
                prepared(),
                PrMergeMethod::Merge,
                directory.path()
            )
            .outcome,
            PrMergeOutcome::Rejected
        );
    }
    let mut changed = fixture();
    changed["viewer"]["login"] = json!("someone-else");
    transport.observations = VecDeque::from([Ok(observation(changed))]);
    assert_eq!(
        submit::execute(
            &mut transport,
            prepared(),
            PrMergeMethod::Merge,
            directory.path()
        )
        .outcome,
        PrMergeOutcome::Rejected
    );
    assert!(transport.dispatched.is_empty());
}
#[test]
fn receipt_failure_and_duplicate_preparation_never_dispatch_again() {
    let directory = tempfile::tempdir().expect("directory");
    let invalid = directory.path().join("file");
    std::fs::write(&invalid, "occupied").expect("file");
    let mut transport = fake(directory.path());
    assert_eq!(
        submit::execute(&mut transport, prepared(), PrMergeMethod::Merge, &invalid).outcome,
        PrMergeOutcome::Rejected
    );
    assert!(transport.dispatched.is_empty());
    std::fs::remove_file(invalid).expect("remove fixture");
    let preparation = prepared();
    let duplicate = preparation.clone();
    let mut transport = fake(directory.path());
    assert_eq!(
        submit::execute(
            &mut transport,
            preparation,
            PrMergeMethod::Merge,
            directory.path()
        )
        .outcome,
        PrMergeOutcome::Confirmed
    );
    transport.observations = VecDeque::from([Ok(observation(fixture()))]);
    assert_eq!(
        submit::execute(
            &mut transport,
            duplicate,
            PrMergeMethod::Merge,
            directory.path()
        )
        .outcome,
        PrMergeOutcome::Rejected
    );
    assert_eq!(transport.dispatched.len(), 1);
}
#[test]
fn uncertain_write_readback_or_different_merged_revision_never_claim_success() {
    for mode in 0..4 {
        let directory = tempfile::tempdir().expect("directory");
        let mut transport = fake(directory.path());
        match mode {
            0 => transport.submission = Err("timeout".into()),
            1 => transport.observations[1] = Err("offline".into()),
            2 => transport.observations[1] = Ok(observation(fixture())),
            _ => {
                let mut other = fixture();
                other["repository"]["pullRequest"]["headRefOid"] = json!("d".repeat(40));
                other["repository"]["pullRequest"]["state"] = json!("MERGED");
                transport.observations[1] = Ok(observation(other));
            }
        }
        let result = submit::execute(
            &mut transport,
            prepared(),
            PrMergeMethod::Merge,
            directory.path(),
        );
        assert_eq!(result.outcome, PrMergeOutcome::OutcomeUnknown);
        assert_eq!(transport.dispatched.len(), 1);
    }
}

#[test]
fn malformed_identity_and_changed_policy_never_dispatch() {
    let directory = tempfile::tempdir().expect("directory");
    for (field, value) in [
        ("headRefOid", json!("short")),
        ("baseRefOid", json!("z".repeat(40))),
        ("mergeStateStatus", json!("BLOCKED")),
        ("isMergeQueueEnabled", json!(true)),
    ] {
        let mut changed = fixture();
        changed["repository"]["pullRequest"][field] = value;
        let mut transport = fake(directory.path());
        transport.observations = VecDeque::from([Ok(observation(changed))]);
        assert_eq!(
            submit::execute(
                &mut transport,
                prepared(),
                PrMergeMethod::Merge,
                directory.path()
            )
            .outcome,
            PrMergeOutcome::Rejected
        );
        assert!(transport.dispatched.is_empty());
    }
}

#[test]
#[ignore = "read-only authenticated GitHub observation; requires SBT_PR_REPO and SBT_PR_MERGE_NUMBER (a closed or merged PR)"]
fn live_historical_pr_merge_is_disabled() {
    let repo = std::env::var("SBT_PR_REPO").expect("explicit repo directory");
    let number = std::env::var("SBT_PR_MERGE_NUMBER")
        .expect("explicit historical PR number")
        .parse()
        .expect("PR number");
    let mut transport = observe::Github {
        repo: Path::new(&repo),
    };
    let observation = transport
        .observe("github.com", "benpchandler/switchbard", number)
        .expect("live observation");
    assert!(matches!(
        observation.pr().state.as_str(),
        "CLOSED" | "MERGED"
    ));
    assert!(observation
        .eligible()
        .expect_err("historical PR cannot merge")
        .contains("Only open"));
}

fn selection() -> (PrSnapshot, PrListRow) {
    let row = PrListRow {
        id: "PR_1".into(),
        number: 1,
        title: "A change".into(),
        url: "https://github.com/owner/repo/pull/1".into(),
        head_oid: "a".repeat(40),
        draft: false,
        lifecycle: crate::PrLifecycle::Open,
        merged_at: None,
        checks: crate::PrChecks::Unknown,
        review: crate::PrReview::Unknown,
        merge: crate::PrMerge::Unknown,
    };
    let snapshot = PrSnapshot {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        observed_at: std::time::SystemTime::now(),
        rows: vec![row.clone()],
        truncated: false,
        limit: 100,
        enrichment_warning: None,
    };
    (snapshot, row)
}

#[test]
fn prepare_rejects_missing_selection_and_repository_url_mismatch_before_reading() {
    let directory = tempfile::tempdir().expect("directory");
    let (mut snapshot, row) = selection();
    let mut transport = fake(directory.path());
    transport.observations.clear();
    snapshot.rows.clear();
    assert!(prepare(&mut transport, &snapshot, &row).is_err());
    snapshot.rows.push(row.clone());
    snapshot.repository_url = "https://github.com/other/repo".into();
    assert!(prepare(&mut transport, &snapshot, &row).is_err());
    assert!(transport.dispatched.is_empty());
}

#[test]
fn prepare_disables_changed_remote_head_id_number_url_or_repository() {
    let directory = tempfile::tempdir().expect("directory");
    let (snapshot, row) = selection();
    for (pointer, replacement) in [
        ("/repository/pullRequest/headRefOid", json!("d".repeat(40))),
        ("/repository/pullRequest/id", json!("PR_2")),
        ("/repository/pullRequest/number", json!(2)),
        (
            "/repository/pullRequest/url",
            json!("https://github.com/owner/repo/pull/2"),
        ),
        ("/repository/nameWithOwner", json!("other/repo")),
    ] {
        let mut changed = fixture();
        *changed
            .pointer_mut(pointer)
            .expect("fixture identity field") = replacement;
        let mut transport = fake(directory.path());
        transport.observations = VecDeque::from([Ok(observation(changed))]);
        assert!(
            matches!(
                prepare(&mut transport, &snapshot, &row).expect("valid response"),
                PrMergePreparation::Disabled(_)
            ),
            "{pointer}"
        );
        assert!(transport.dispatched.is_empty());
    }
}

#[test]
fn prepare_binds_fresh_identity_and_methods_without_trusting_cached_delivery() {
    let directory = tempfile::tempdir().expect("directory");
    let (snapshot, row) = selection();
    let mut transport = fake(directory.path());
    let PrMergePreparation::Ready(prepared) =
        prepare(&mut transport, &snapshot, &row).expect("fresh complete observation")
    else {
        panic!("fresh eligible observation must prepare");
    };
    assert_eq!(prepared.repository(), snapshot.repository);
    assert_eq!(prepared.number(), row.number);
    assert_eq!(prepared.head_oid(), row.head_oid);
    assert_eq!(prepared.base_ref(), "main");
    assert_eq!(prepared.base_oid(), "b".repeat(40));
    assert_eq!(prepared.viewer(), "maintainer");
    assert_eq!(
        prepared.methods(),
        vec![PrMergeMethod::Merge, PrMergeMethod::Squash]
    );
    assert!(transport.dispatched.is_empty());
}

#[test]
fn enabled_merge_methods_cover_zero_each_single_and_all() {
    for (enabled, expected) in [
        ([false, false, false], vec![]),
        ([true, false, false], vec![PrMergeMethod::Merge]),
        ([false, true, false], vec![PrMergeMethod::Squash]),
        ([false, false, true], vec![PrMergeMethod::Rebase]),
        (
            [true, true, true],
            vec![
                PrMergeMethod::Merge,
                PrMergeMethod::Squash,
                PrMergeMethod::Rebase,
            ],
        ),
    ] {
        let mut value = fixture();
        for (field, allowed) in [
            "mergeCommitAllowed",
            "squashMergeAllowed",
            "rebaseMergeAllowed",
        ]
        .into_iter()
        .zip(enabled)
        {
            value["repository"][field] = json!(allowed);
        }
        let observed = observation(value);
        assert_eq!(observed.methods(), expected);
        assert_eq!(observed.eligible().is_ok(), !expected.is_empty());
    }
}
