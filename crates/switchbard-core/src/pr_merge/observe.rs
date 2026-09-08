//! One bounded GraphQL observation owns identity, policy and allowed methods.
use super::PrMergeMethod;
use crate::{PrListRow, PrSnapshot};
use serde::Deserialize;
use std::path::Path;

const QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){viewer{login} repository(owner:$owner,name:$name){id nameWithOwner viewerPermission mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed pullRequest(number:$number){id number title url state isDraft headRefOid baseRefName baseRefOid mergeable mergeStateStatus reviewDecision isMergeQueueEnabled mergeCommit{oid}}}}";

#[derive(Debug, Clone, Deserialize)]
pub(super) struct Observation {
    pub viewer: Viewer,
    pub repository: Repository,
}
#[derive(Debug, Clone, Deserialize)]
pub(super) struct Viewer {
    pub login: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Repository {
    pub id: String,
    pub name_with_owner: String,
    pub viewer_permission: Option<String>,
    merge_commit_allowed: bool,
    squash_merge_allowed: bool,
    rebase_merge_allowed: bool,
    pull_request: PullRequest,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PullRequest {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    is_draft: bool,
    pub head_ref_oid: String,
    pub base_ref_name: String,
    pub base_ref_oid: String,
    mergeable: String,
    merge_state_status: String,
    review_decision: Option<String>,
    is_merge_queue_enabled: bool,
    pub merge_commit: Option<Commit>,
}
#[derive(Debug, Clone, Deserialize)]
pub(super) struct Commit {
    pub oid: String,
}
impl Observation {
    pub fn pr(&self) -> &PullRequest {
        &self.repository.pull_request
    }
    pub fn methods(&self) -> Vec<PrMergeMethod> {
        [
            (self.repository.merge_commit_allowed, PrMergeMethod::Merge),
            (self.repository.squash_merge_allowed, PrMergeMethod::Squash),
            (self.repository.rebase_merge_allowed, PrMergeMethod::Rebase),
        ]
        .into_iter()
        .filter_map(|(allowed, method)| allowed.then_some(method))
        .collect()
    }
    pub fn eligible(&self) -> Result<(), String> {
        let pr = self.pr();
        if pr.state != "OPEN" || pr.is_draft {
            return Err("Only open, ready-for-review PRs can merge".into());
        }
        if pr.is_merge_queue_enabled {
            return Err("Merge queue required; continue in GitHub".into());
        }
        if !matches!(
            self.repository.viewer_permission.as_deref(),
            Some("WRITE" | "MAINTAIN" | "ADMIN")
        ) {
            return Err("Current account lacks confirmed merge permission".into());
        }
        if pr.mergeable != "MERGEABLE" || pr.merge_state_status != "CLEAN" {
            return Err(format!(
                "GitHub has not confirmed merge readiness ({}/{})",
                pr.mergeable, pr.merge_state_status
            ));
        }
        if !matches!(pr.review_decision.as_deref(), None | Some("APPROVED")) {
            return Err("Required review is pending or changes were requested".into());
        }
        self.valid_identity()
    }
    fn valid_identity(&self) -> Result<(), String> {
        let pr = self.pr();
        if self.methods().is_empty() {
            return Err("No direct merge method is enabled".into());
        }
        if self.viewer.login.is_empty()
            || self.repository.id.is_empty()
            || !valid_oid(&pr.head_ref_oid)
            || !valid_oid(&pr.base_ref_oid)
        {
            return Err("GitHub returned incomplete merge identity".into());
        }
        Ok(())
    }
}

pub(super) trait Transport {
    fn observe(&mut self, host: &str, repository: &str, number: u64)
        -> Result<Observation, String>;
    fn merge(
        &mut self,
        host: &str,
        id: &str,
        head: &str,
        method: PrMergeMethod,
    ) -> Result<Vec<u8>, String>;
}
pub(super) struct Github<'a> {
    pub repo: &'a Path,
}
impl Transport for Github<'_> {
    fn observe(
        &mut self,
        host: &str,
        repository: &str,
        number: u64,
    ) -> Result<Observation, String> {
        let (owner, name) = repository.split_once('/').ok_or("Invalid repository")?;
        let bytes = crate::pr_list::process::run_gh(
            self.repo,
            &[
                "api",
                "graphql",
                "--hostname",
                host,
                "-f",
                &format!("query={QUERY}"),
                "-f",
                &format!("owner={owner}"),
                "-f",
                &format!("name={name}"),
                "-F",
                &format!("number={number}"),
            ],
        )?;
        decode(&bytes)
    }
    fn merge(
        &mut self,
        host: &str,
        id: &str,
        head: &str,
        method: PrMergeMethod,
    ) -> Result<Vec<u8>, String> {
        let query = "mutation($id:ID!,$head:GitObjectID!,$method:PullRequestMergeMethod!){mergePullRequest(input:{pullRequestId:$id,expectedHeadOid:$head,mergeMethod:$method}){pullRequest{id state headRefOid mergeCommit{oid}}}}";
        crate::pr_list::process::run_gh(
            self.repo,
            &[
                "api",
                "graphql",
                "--hostname",
                host,
                "-f",
                &format!("query={query}"),
                "-f",
                &format!("id={id}"),
                "-f",
                &format!("head={head}"),
                "-f",
                &format!("method={}", method.api()),
            ],
        )
    }
}
fn valid_oid(oid: &str) -> bool {
    oid.len() == 40 && oid.bytes().all(|c| c.is_ascii_hexdigit())
}
pub(super) fn validate_selection(snapshot: &PrSnapshot, row: &PrListRow) -> Result<String, String> {
    let url = snapshot
        .repository_url
        .strip_prefix("https://")
        .ok_or("Repository URL must use HTTPS")?;
    let (host, repository) = url.split_once('/').ok_or("Invalid repository URL")?;
    if host.is_empty()
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
        || repository != snapshot.repository
        || repository.split('/').count() != 2
        || repository.split('/').any(|s| {
            s.is_empty()
                || !s
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
        || row.number == 0
        || row.url != format!("{}/pull/{}", snapshot.repository_url, row.number)
        || !snapshot
            .rows
            .iter()
            .take(crate::pr_list::MAX_PULL_REQUESTS)
            .any(|candidate| candidate == row)
    {
        return Err("Selected PR does not match the repository snapshot".into());
    }
    Ok(host.into())
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
    errors: Option<serde::de::IgnoredAny>,
}
pub(super) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let response: Envelope<T> =
        serde_json::from_slice(bytes).map_err(|e| format!("Incomplete GitHub response: {e}"))?;
    if response.errors.is_some() {
        return Err("GitHub could not complete the request".into());
    }
    response
        .data
        .ok_or_else(|| "GitHub returned no response data".into())
}
