use super::*;
use serde_json::{json, Value};
const REPO: &str = "https://github.com/owner/repo";
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture() -> Value {
    json!({"id":"PR_1", "number":1, "title":"A PR", "url":format!("{REPO}/pull/1"),
        "state":"OPEN", "headRefOid":HEAD,"isDraft":false,"statusCheckRollup":[],
        "reviewDecision":"", "mergeable":"UNKNOWN"})
}

fn parse(value: Value) -> Result<(Vec<PrListRow>, bool), String> {
    rows(
        &serde_json::to_vec(&value).expect("fixture serializes"),
        REPO,
        super::super::DEFAULT_PULL_REQUEST_LIMIT,
    )
}

#[test]
fn empty_is_distinct_from_unavailable_and_malformed() {
    assert_eq!(parse(json!([])).expect("empty response"), (vec![], false));
    for value in [json!(null), json!({}), json!([{}]), json!([fixture(), {}])] {
        assert!(parse(value).is_err());
    }
    assert!(rows(b"not json", REPO, 100).is_err());
}

#[test]
fn no_checks_unknown_and_observed_pass_stay_separate() {
    let mut pr = fixture();
    let cases = [
        (Value::Null, PrChecks::Unknown),
        (json!([]), PrChecks::NoneObserved),
        (
            json!([{"__typename":"CheckRun","status":"COMPLETED","conclusion":"SUCCESS"}]),
            PrChecks::Passing,
        ),
        (
            json!([{"__typename":"CheckRun","status":"COMPLETED","conclusion":"SKIPPED"}]),
            PrChecks::Unknown,
        ),
        (
            json!([{"__typename":"CheckRun","status":"IN_PROGRESS"}]),
            PrChecks::Running,
        ),
        (
            json!([{"__typename":"CheckRun","status":"COMPLETED","conclusion":"FAILURE"}]),
            PrChecks::Failed,
        ),
        (
            json!([{"__typename":"StatusContext","state":"SUCCESS"}]),
            PrChecks::Passing,
        ),
        (
            json!([{"__typename":"FutureType","state":"SUCCESS"}]),
            PrChecks::Unknown,
        ),
    ];
    for (rollup, expected) in cases {
        pr["statusCheckRollup"] = rollup;
        let result = parse(json!([pr])).expect("valid response");
        assert_eq!(result.0[0].checks, expected);
        assert_eq!(result.0[0].review, PrReview::Unknown);
    }
}

#[test]
fn mismatched_head_and_incomplete_contexts_cannot_pass() {
    let mut pr = fixture();
    pr["statusCheckRollup"] = json!([{"__typename":"CheckRun", "status":"COMPLETED",
        "conclusion":"SUCCESS", "headSha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]);
    assert_eq!(
        parse(json!([pr])).expect("valid PR").0[0].checks,
        PrChecks::Unknown
    );
    pr["statusCheckRollup"] = json!(vec![
        json!({"__typename":"StatusContext","state":"SUCCESS"});
        100
    ]);
    assert_eq!(
        parse(json!([pr])).expect("valid PR").0[0].checks,
        PrChecks::Unknown
    );
}

#[test]
fn failure_is_visible_even_with_unknown_contexts() {
    let mut pr = fixture();
    pr["statusCheckRollup"] = json!([{}, {"__typename":"StatusContext","state":"FAILURE"}]);
    assert_eq!(
        parse(json!([pr])).expect("valid PR").0[0].checks,
        PrChecks::Failed
    );
}

#[test]
fn identity_is_scoped_and_never_guessed() {
    for (key, value) in [
        ("url", json!("https://github.com/other/repo/pull/1")),
        ("number", json!(0)),
        ("id", json!("")),
        ("state", json!("FUTURE_STATE")),
        ("headRefOid", json!("not-a-revision")),
    ] {
        let mut pr = fixture();
        pr[key] = value;
        assert!(parse(json!([pr])).is_err(), "{key}");
    }
    assert!(parse(json!([fixture(), fixture()])).is_err());
}

#[test]
fn truncation_preserves_first_hundred_and_says_partial() {
    let entries: Vec<Value> = (1..=101)
        .map(|n| {
            let mut pr = fixture();
            pr["id"] = json!(format!("PR_{n}"));
            pr["number"] = json!(n);
            pr["url"] = json!(format!("{REPO}/pull/{n}"));
            pr
        })
        .collect();
    let (rows, partial) = parse(json!(entries)).expect("bounded list");
    assert!(partial);
    assert_eq!(rows.len(), 100);
    assert!(!parse(json!(&entries[..100])).expect("full limit").1);
    assert!(parse(json!(vec![fixture(); 102])).is_err());
}

#[test]
fn repository_identity_refuses_host_or_path_ambiguity() {
    assert_eq!(
        repository(br#"{"nameWithOwner":"owner/repo","url":"https://github.com/owner/repo"}"#)
            .expect("canonical repo"),
        ("owner/repo".into(), REPO.into())
    );
    for value in [
        json!({"nameWithOwner":"owner/repo","url":"https://elsewhere.test/owner/repo"}),
        json!({"nameWithOwner":"owner/repo","url":"https://github.com/other/repo"}),
        json!({"nameWithOwner":"owner/../repo","url":"https://github.com/owner/../repo"}),
    ] {
        assert!(repository(&serde_json::to_vec(&value).expect("fixture")).is_err());
    }
}

#[test]
fn every_lifecycle_is_observed_and_history_sorts_after_active_work() {
    let mut pr = fixture();
    for (state, lifecycle, rank) in [
        ("OPEN", PrLifecycle::Open, 1),
        ("CLOSED", PrLifecycle::Closed, 4),
        ("MERGED", PrLifecycle::Merged, 4),
    ] {
        pr["state"] = json!(state);
        let (rows, partial) = parse(json!([pr])).expect("known lifecycle");
        assert_eq!(rows[0].lifecycle, lifecycle);
        assert_eq!(rows[0].attention_rank(), rank);
        assert!(!partial);
    }
    pr["statusCheckRollup"] = json!([{"__typename":"StatusContext","state":"FAILURE"}]);
    assert_eq!(
        parse(json!([pr])).expect("historical failed check").0[0].attention_rank(),
        4
    );
}

#[test]
fn progressive_windows_and_hard_cap_retain_explicit_truncation() {
    let entries: Vec<Value> = (1..=1001)
        .map(|n| {
            let mut pr = fixture();
            pr["id"] = json!(format!("PR_{n}"));
            pr["number"] = json!(n);
            pr["url"] = json!(format!("{REPO}/pull/{n}"));
            pr["state"] = json!(if n % 2 == 0 { "MERGED" } else { "OPEN" });
            pr
        })
        .collect();
    for limit in [100, 200, 1000] {
        let bytes = serde_json::to_vec(&entries[..=limit]).expect("fixture");
        let (result, partial) = rows(&bytes, REPO, limit).expect("bounded mixed-lifecycle list");
        assert_eq!(result.len(), limit);
        assert!(partial);
        let exact = serde_json::to_vec(&entries[..limit]).expect("fixture");
        assert!(!rows(&exact, REPO, limit).expect("complete window").1);
    }
    for limit in [0, 1001, usize::MAX] {
        assert!(rows(b"[]", REPO, limit).is_err());
    }
}
