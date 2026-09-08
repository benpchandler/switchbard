use super::*;
use observe::{Observation, Transport};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::time::Duration;

/// The live retry policy with the waiting taken out: same attempt count, no
/// settle, and a budget no test can spend.
fn instant_retry() -> MergeabilityRetry {
    MergeabilityRetry {
        settle: Duration::ZERO,
        budget: Duration::from_secs(3600),
        ..MergeabilityRetry::LIVE
    }
}

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
    /// How long each observation takes, standing in for a slow round trip.
    observe_delay: Duration,
    submission: Result<Vec<u8>, String>,
    dispatched: Vec<(String, String, PrMergeMethod)>,
    receipt_directory: std::path::PathBuf,
}
impl Transport for Fake {
    fn observe(&mut self, _: &str, _: &str, _: u64) -> Result<Observation, String> {
        std::thread::sleep(self.observe_delay);
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
        observe_delay: Duration::ZERO,
        dispatched: vec![], receipt_directory: directory.into() }
}
#[test]
fn policy_blocks_incomplete_and_unsafe_states_but_allows_clean_admin_without_required_review() {
    assert!(observation(fixture()).eligible().is_ok());
    for (field, value) in [
        ("state", json!("CLOSED")),
        ("isDraft", json!(true)),
        ("mergeable", json!("UNKNOWN")),
        ("mergeStateStatus", json!("BLOCKED")),
        ("mergeStateStatus", json!("DIRTY")),
        ("mergeStateStatus", json!("DRAFT")),
        ("mergeStateStatus", json!("UNKNOWN")),
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

/// Reads one PR through the real transport. `SBT_PR_SLUG` names the
/// repository (`owner/name`, default this one) so the check can be pointed at
/// whichever PR is being investigated, not only at switchbard's own.
fn live_observation() -> Observation {
    let repo = std::env::var("SBT_PR_REPO").expect("explicit repo directory");
    let slug =
        std::env::var("SBT_PR_SLUG").unwrap_or_else(|_| "benpchandler/switchbard".to_string());
    let number = std::env::var("SBT_PR_MERGE_NUMBER")
        .expect("explicit PR number")
        .parse()
        .expect("PR number");
    observe::Github {
        repo: Path::new(&repo),
    }
    .observe("github.com", &slug, number)
    .expect("live observation")
}

#[test]
#[ignore = "read-only authenticated GitHub observation; requires SBT_PR_REPO and SBT_PR_MERGE_NUMBER (a closed or merged PR)"]
fn live_historical_pr_merge_is_disabled() {
    let observation = live_observation();
    assert!(matches!(
        observation.pr().state.as_str(),
        "CLOSED" | "MERGED"
    ));
    assert!(observation
        .eligible()
        .expect_err("historical PR cannot merge")
        .contains("Only open"));
}

/// TASK-171's shape against the real API: whatever GitHub says about an open
/// PR, sbt's verdict agrees with GitHub's own - eligible exactly when
/// `mergeable` is MERGEABLE and the state is one GitHub merges on.
#[test]
#[ignore = "read-only authenticated GitHub observation; requires SBT_PR_REPO and SBT_PR_MERGE_NUMBER (an open PR)"]
fn live_open_pr_verdict_matches_what_github_permits() {
    let observation = live_observation();
    assert_eq!(observation.pr().state, "OPEN", "point this at an open PR");
    let (mergeable, state) = observation.merge_readiness();
    // GitHub's rule, spelled out rather than read from MERGE_READY: the point
    // is to check our constant against the real API, not against itself.
    let github_would_merge =
        mergeable == "MERGEABLE" && matches!(state, "CLEAN" | "HAS_HOOKS" | "UNSTABLE");
    let verdict = observation.eligible();
    eprintln!(
        "live: {mergeable}/{state} eligible={} caveat={:?}",
        verdict.is_ok(),
        observation.readiness_caveat()
    );
    assert_eq!(
        verdict.is_ok(),
        github_would_merge,
        "{mergeable}/{state} -> {verdict:?}"
    );
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
        open_count: Ok(1),
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
    assert!(prepare(&mut transport, &snapshot, &row, instant_retry()).is_err());
    snapshot.rows.push(row.clone());
    snapshot.repository_url = "https://github.com/other/repo".into();
    assert!(prepare(&mut transport, &snapshot, &row, instant_retry()).is_err());
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
                prepare(&mut transport, &snapshot, &row, instant_retry()).expect("valid response"),
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
        prepare(&mut transport, &snapshot, &row, instant_retry())
            .expect("fresh complete observation")
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

/// TASK-171: GitHub's merge button stays live at `UNSTABLE` - mergeable, with
/// checks failing or still running, none of them required. Gating on `CLEAN`
/// refused a merge GitHub permitted; the guard is now the confirmation naming
/// the state, not a refusal that calls a mergeable PR unmergeable.
#[test]
fn states_github_itself_merges_on_are_allowed_and_each_names_its_caveat() {
    for (status, caveat) in [
        ("CLEAN", None),
        ("UNSTABLE", Some("UNSTABLE")),
        ("HAS_HOOKS", Some("HAS_HOOKS")),
    ] {
        let mut value = fixture();
        value["repository"]["pullRequest"]["mergeStateStatus"] = json!(status);
        let observed = observation(value);
        assert!(observed.eligible().is_ok(), "{status}");
        match (observed.readiness_caveat(), caveat) {
            (None, None) => {}
            (Some(line), Some(needle)) => assert!(line.contains(needle), "{line}"),
            (line, expected) => panic!("{status}: {line:?} vs {expected:?}"),
        }
    }
}

/// The same fact, at the surface the user actually confirms on: an UNSTABLE
/// merge prepares, and the confirmation says why it is not plainly green.
#[test]
fn an_unstable_pr_prepares_and_its_confirmation_carries_the_reason() {
    let directory = tempfile::tempdir().expect("directory");
    let (snapshot, row) = selection();
    let mut unstable = fixture();
    unstable["repository"]["pullRequest"]["mergeStateStatus"] = json!("UNSTABLE");
    let mut transport = fake(directory.path());
    transport.observations = VecDeque::from([Ok(observation(unstable))]);
    let PrMergePreparation::Ready(prepared) =
        prepare(&mut transport, &snapshot, &row, instant_retry()).expect("valid response")
    else {
        panic!("GitHub merges UNSTABLE PRs, so sbt must be able to prepare one");
    };
    let caveat = prepared
        .readiness_caveat()
        .expect("a non-clean state is named");
    assert!(caveat.contains("UNSTABLE"), "{caveat}");
    assert!(caveat.contains("required"), "{caveat}");
}

/// GitHub answers `UNKNOWN` on the first read of a PR it has not looked at
/// lately and computes mergeability behind that answer. One look is not an
/// answer: ask again before calling a mergeable PR unmergeable (TASK-171).
#[test]
fn pending_mergeability_is_asked_again_rather_than_reported_as_a_refusal() {
    let directory = tempfile::tempdir().expect("directory");
    let (snapshot, row) = selection();
    let mut pending = fixture();
    pending["repository"]["pullRequest"]["mergeable"] = json!("UNKNOWN");
    pending["repository"]["pullRequest"]["mergeStateStatus"] = json!("UNKNOWN");

    let mut transport = fake(directory.path());
    transport.observations =
        VecDeque::from([Ok(observation(pending.clone())), Ok(observation(fixture()))]);
    assert!(
        matches!(
            prepare(&mut transport, &snapshot, &row, instant_retry()).expect("valid response"),
            PrMergePreparation::Ready(_)
        ),
        "a settled MERGEABLE answer wins over the first UNKNOWN"
    );
    assert!(transport.observations.is_empty(), "the retry was used");

    // Still pending after every attempt is a refusal, and a bounded one.
    let mut transport = fake(directory.path());
    transport.observations = VecDeque::from(vec![
        Ok(observation(pending));
        MergeabilityRetry::LIVE.attempts
    ]);
    assert!(matches!(
        prepare(&mut transport, &snapshot, &row, instant_retry()).expect("valid response"),
        PrMergePreparation::Disabled(_)
    ));
    assert!(
        transport.observations.is_empty(),
        "no attempt beyond the bound"
    );
}

/// The retry must not be able to turn `m` into a minute and a half of
/// "Preparing merge" ending in the same refusal. `gh` bounds one call at 30
/// seconds, so the attempt count alone is not a bound a person would accept;
/// the budget is. When the round trip itself is what is slow, asking again
/// buys nothing, so no second look is taken at all.
#[test]
fn a_slow_first_look_is_not_asked_again() {
    let directory = tempfile::tempdir().expect("directory");
    let (snapshot, row) = selection();
    let mut pending = fixture();
    pending["repository"]["pullRequest"]["mergeable"] = json!("UNKNOWN");
    pending["repository"]["pullRequest"]["mergeStateStatus"] = json!("UNKNOWN");

    let retry = MergeabilityRetry {
        attempts: 3,
        settle: Duration::ZERO,
        budget: Duration::from_millis(20),
    };
    let mut transport = fake(directory.path());
    transport.observations = VecDeque::from(vec![Ok(observation(pending.clone())); 3]);
    // One look already outlasts the whole budget.
    transport.observe_delay = Duration::from_millis(40);
    assert!(matches!(
        prepare(&mut transport, &snapshot, &row, retry).expect("valid response"),
        PrMergePreparation::Disabled(_)
    ));
    assert_eq!(
        transport.observations.len(),
        2,
        "a slow round trip must not buy another look"
    );

    // The same UNKNOWN, answered fast, still gets its retries.
    let mut transport = fake(directory.path());
    transport.observations = VecDeque::from(vec![Ok(observation(pending)); 3]);
    assert!(matches!(
        prepare(&mut transport, &snapshot, &row, retry).expect("valid response"),
        PrMergePreparation::Disabled(_)
    ));
    assert!(
        transport.observations.is_empty(),
        "a fast UNKNOWN is worth asking about again"
    );
}

/// The bound is on elapsed time, not on the clock being generous: the live
/// policy's own numbers cannot exceed what a person will wait at the
/// keyboard. Three 30s `gh` calls would; the budget is what stops it.
#[test]
fn the_live_retry_policy_cannot_outlast_a_single_request() {
    let live = MergeabilityRetry::LIVE;
    assert!(live.attempts >= 2, "one look is not an answer");
    assert!(
        live.budget < Duration::from_secs(30),
        "the budget must expire well inside one bounded gh call"
    );
    let started = std::time::Instant::now();
    assert!(live.may_retry(1, started), "a fast UNKNOWN retries");
    assert!(
        !live.may_retry(live.attempts, started),
        "the attempt count still caps a run of fast UNKNOWNs"
    );
}
