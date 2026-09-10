//! Exact repository total, independent of the bounded PR history and delivery reads.
use super::process;
use serde::Deserialize;
use std::path::Path;

const QUERY: &str = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { url pullRequests(states: OPEN) { totalCount } } }";

pub(super) fn fetch(repo: &Path, repository: &str, url: &str) -> Result<u64, String> {
    let args = arguments(repository);
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    process::run_gh(repo, &args).and_then(|data| parse(&data, url))
}

fn arguments(repository: &str) -> Vec<String> {
    let (owner, name) = repository
        .split_once('/')
        .expect("repository identity was validated before count query");
    // Raw fields retain numeric-looking repository names as GraphQL strings.
    // Explicit host pins the same github.com identity as the PR list read.
    vec![
        "api".into(),
        "graphql".into(),
        "--hostname".into(),
        "github.com".into(),
        "--raw-field".into(),
        format!("query={QUERY}"),
        "--raw-field".into(),
        format!("owner={owner}"),
        "--raw-field".into(),
        format!("name={name}"),
    ]
}

#[derive(Deserialize)]
struct Response {
    data: Option<Data>,
    errors: Option<Vec<serde::de::IgnoredAny>>,
}

#[derive(Deserialize)]
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    url: String,
    pull_requests: Count,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Count {
    total_count: u64,
}

fn parse(data: &[u8], expected_url: &str) -> Result<u64, String> {
    let response: Response = serde_json::from_slice(data)
        .map_err(|error| format!("Invalid GitHub open PR count: {error}"))?;
    if response.errors.is_some_and(|errors| !errors.is_empty()) {
        return Err("GitHub open PR count query returned errors".into());
    }
    let repository = response
        .data
        .and_then(|data| data.repository)
        .ok_or("GitHub open PR count repository unavailable")?;
    if repository.url != expected_url {
        return Err("GitHub open PR count repository identity changed".into());
    }
    Ok(repository.pull_requests.total_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const URL: &str = "https://github.com/owner/repo";

    fn response(count: serde_json::Value) -> serde_json::Value {
        json!({"data":{"repository":{"url":URL,"pullRequests":{"totalCount":count}}}})
    }

    #[test]
    fn exact_total_includes_values_beyond_the_loaded_row_cap_and_known_zero() {
        for count in [0, 1, 1001, 100_000] {
            let bytes = serde_json::to_vec(&response(json!(count))).expect("fixture");
            assert_eq!(parse(&bytes, URL), Ok(count));
        }
    }

    #[test]
    fn malformed_missing_negative_and_partial_error_counts_are_unknown() {
        for value in [
            json!({}),
            json!({"data":null}),
            json!({"data":{"repository":null}}),
            response(json!(-1)),
            response(json!(null)),
            response(json!("2")),
            response(json!(1.5)),
        ] {
            assert!(parse(&serde_json::to_vec(&value).expect("fixture"), URL).is_err());
        }
        let mut partial = response(json!(2));
        partial["errors"] = json!([{"message":"partial result"}]);
        assert!(parse(&serde_json::to_vec(&partial).expect("fixture"), URL).is_err());
        assert!(parse(
            &serde_json::to_vec(&response(json!(2))).expect("fixture"),
            "https://github.com/other/repo"
        )
        .is_err());
    }

    #[test]
    fn query_pins_host_and_uses_string_variables_without_paging() {
        let args = arguments("123/456");
        assert_eq!(&args[..4], ["api", "graphql", "--hostname", "github.com"]);
        assert_eq!(
            &args[6..],
            ["--raw-field", "owner=123", "--raw-field", "name=456"]
        );
        assert_eq!(args[5], format!("query={QUERY}"));
    }
}
