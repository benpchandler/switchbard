# Architecture: Task Queue authority and identity model

## Vocabulary and identities

| Concept | Stable identity | Mutable labels or coordinates |
|---|---|---|
| `LocalTaskKey` | `(repo_root, task_id)`, matching the existing GUI key | title, path-derived display name, project, status |
| `GitHubProjectBindingId` | Switchbard-minted opaque id stored in config | Project URL locator, repo scope, source-band position |
| `GitHubNodeKey` | `(host, opaque new-format GraphQL node id)` | owner/repo, issue or PR number, SHA, tag, URL, title |
| `GitHubProjectMembershipKey` | `(project node id, ProjectV2Item node id)` | item position, archive state, content coordinates |
| `LocalGitHubLinkKey` | `(LocalTaskKey, GitHubNodeKey)` | original URL locator retained in `references:` |
| `QueueProjectionKey` | local key for a linked/native row; highest-ranked membership key for an unlinked remote row | freshness, attention state, badges, display coordinates |

Global node ids are opaque. The adapter requests GitHub's new global-id format and never decodes ids to infer type. `__typename` is observed explicitly. Host is part of identity even though v1 accepts only `github.com`.

The Project source binding is not the GitHub Project. The binding is durable Switchbard intent to observe one locator in one repo scope; the Project node is a GitHub observation that may be unavailable before the first successful refresh.

## Definitions, instances, and revisions

`GitHubProjectBinding` is a persisted Switchbard definition with `binding_id`, `host`, `project_url`, `repo_root`, and implicit `source_order` from its position in the config list. Adding, removing, or moving a binding is an explicit user mutation.

Each refresh attempt creates a monotonically increasing in-memory generation for one binding. A successful generation produces one immutable `DeliverySnapshot` containing the Project node, ordered memberships, content artifacts, delivery relationships, per-field facts, `observed_at_ms`, `attempted_at_ms`, and observer identity when available. A later generation replaces the cached display revision only if it is still current.

The cache retains the latest successful snapshot per binding, not a historical ledger. Refresh failure wraps that snapshot as Stale without changing its original observation time. Correction is a successor snapshot; cached bytes are never edited as if they were GitHub authority.

## Ownership and authorization

### Switchbard-authoritative durable fields

| Field | Authority and write boundary |
|---|---|
| Native task title, description, status, priority, assignees, labels, dependencies, project, acceptance criteria, plan, notes, and outcome acceptance | Native task file through `switchbard-core::backlog` only |
| Explicit sibling rank and expedite lane | `backlog/ranking.yml` through `backlog::ranking` |
| Source bindings and source-band order | `Config.github_project_bindings` through config mutation methods |
| Queue membership and claim token | dispatch label ladder through the native dispatch command boundary |
| Generic local-to-GitHub references | canonical GitHub URL strings in native task `references:` |
| Dispatch success | atomic `RecordDispatchSuccess` native mutation |

Native task `priority` is not stack rank. Stack rank is not queue order. Queue order is derived from native task facts and ranking. No code may persist the derived order.

### GitHub-authoritative observed fields

Project identity and title; Project membership, archive state, and within-Project item order; issue identity, repository, title, body, state, labels, assignees, and milestones; PR identity, state, reviews, checks, merge queue, merge commit, and commits; releases, deployments, environments, and repository automation state.

The adapter is read-only by construction: its public port contains fetch/resolve functions but no mutation verb. Current `gh` authentication and scopes authorize a read. Missing auth, insufficient scope, unsupported host, rate limit, network failure, and missing-or-inaccessible resources remain distinct reasons.

## Relationships and temporal meaning

`GitHubProjectMembership` is an observed directional edge `Project -> ProjectV2Item -> content node`. Position and archive state belong to the membership, not the issue. The same issue may have several memberships without becoming several GitHub artifacts.

Delivery relationships are observed directional edges: issue closing references point to PRs; PRs point to reviews, checks, commits, and merge-queue entries; releases and deployments point to their reported commit or environment. An empty edge list is known-empty only when the enclosing connection is complete. An omitted, truncated, unsupported, or failed subquery is Unknown.

`LocalGitHubLink` is a generic association. V1 stores no semantic link kind. A reference URL that has not resolved is an `UnresolvedReference` and never joins records by guessed coordinates. The original task URL is a user-authored locator and is not rewritten by refresh; the UI may display the current derived URL from the observation beside it.

If a visible local task resolves to a Project item's content node or one of its delivery descendants, the local task owns the queue row and the standalone remote projection is suppressed. Multiple local tasks remain multiple local rows because their outcome identities differ. If the same unlinked artifact appears in several bindings, one remote row uses the earliest binding-list position and retains all memberships as provenance.

### Observation state

`Fact<T>` is either `Known { value, source_updated_at_ms? }` or `Unknown { reason }`. It is nested inside a source-level state:

- `NeverObserved { attempted_at_ms?, reason? }`: no successful snapshot exists.
- `Refreshing { started_at_ms, prior? }`: a generation is in flight; prior data remains visible when present.
- `Fresh { observed_at_ms, fields }`: the latest bounded refresh succeeded; individual fields may still be Unknown.
- `Stale { observed_at_ms, attempted_at_ms, fields, reason }`: a prior successful snapshot remains visible after a later refresh failed.
- `Unavailable { attempted_at_ms, reason }`: no prior successful snapshot and the refresh failed.

`MissingOrInaccessible` is a reason, not a deletion verdict. `RateLimited { reset_at_ms }` suppresses retries until the reset. GitHub's own `updatedAt` is source time and never substitutes for Switchbard's observation or attempt time.

## Queue composition

The data contract yields ordered bands rather than inventing a single cross-authority priority scale:

1. The local dispatch band contains active tasks carrying `dispatch` or `dispatching`, in the exact order already produced by `load_backlog_repo` after `RepoRanking` is applied. All local consumers call one core queue selector and must not re-sort.
2. GitHub source bands follow in `Config.github_project_bindings` order for the selected repo scope.
3. Active unlinked items within a source band follow the Project membership order from the latest visible snapshot. Closed or archived items appear only in history.
4. An adopted or otherwise linked item is removed from the standalone remote band and shown through its local task row.

Remote-only items have no Switchbard priority, dependencies, claim, or outcome state. The UI must not render empty placeholders that imply otherwise. Adoption is the explicit boundary that creates those local facts.

The existing Dispatches activity monitor is conserved as a separate progress/history section within the same Tasks / Dispatches destination. Its Active, Queued, Finished, and Failed facets, run actions, and newest-run-first history answer what dispatch is doing or did; they do not reorder the next-work bands or claim to be queue priority.

## Commands and transaction boundary

### Source commands

- `AddGitHubProjectBinding`: validate `github.com` Project URL, mint binding id, reject duplicate binding locator in the same repo scope, append config atomically.
- `MoveGitHubProjectBinding`: reorder one binding within the same repo scope; reject unknown ids and cross-scope targets.
- `RemoveGitHubProjectBinding`: remove durable observation intent and evict its disposable cache file; never touch tasks or GitHub.
- `RefreshGitHubProject`: read GitHub, parse a complete bounded snapshot, and publish only if the generation is current. It never calls a local task writer.

### Task commands

- `AdoptGitHubItem { repo_root, node_key, canonical_url, title }`: require `node_key` to be resolved in the currently visible snapshot, then acquire a git-common-dir reservation named from `git hash-object --stdin` over `host`, a NUL separator, and the opaque node id. The git common directory already scopes the reservation to the repository. While holding it, resolve every native task reference through that snapshot. One matching task is an idempotent success, more than one is a conflict, and none creates one native task whose initial `references:` contains the canonical artifact URL. `NewBacklogTask.references` defaults empty for ordinary creation, while adoption supplies exactly one URL. The reservation closes cross-process races and the durable generic link is the derivable idempotency record. Unresolved identity, reservation failure, native create failure, or conflicting links have zero task-file effect. The GUI remote-row Adopt action is the v1 caller; the orchestration queue CLI does not adopt work.
- `RecordDispatchSuccess { task_id, pr_url, optional_note }`: require a canonical GitHub PR URL; normalize absent/blank caller text to none; read the claimed task once; construct labels, `In Review` status, one note entry containing `Dispatch PR: <url>` plus nonblank optional text, and the de-duplicated reference; replace the task file once. Replay of the same URL and normalized note is success/no-op. A different URL or different normalized note is a conflict. The Rust fallback supplies no optional text, `sb queue release --outcome dispatched --note` forwards its text into this command, and the LangGraph protocol supplies no optional text. No caller appends a post-success note. Validation or write failure leaves prior task bytes and claim unchanged.
- Existing native edits continue through `switchbard-core::backlog`; no GitHub adapter function accepts them.

The external PR may already exist when `RecordDispatchSuccess` runs. Therefore local failure is not compensated by closing the PR. The task remains `dispatching`, the run reports the local recording failure, and an idempotent retry repairs the local record. The terminal `dispatched` label is written only with the PR edge.

## Persistence and reads

| Store | Contents | Authority |
|---|---|---|
| repo `backlog/` | native task files, ranking, hierarchy, goals | authoritative local work |
| `~/.switchbard/config.toml` | additive `github_project_bindings` list | authoritative observation intent and source-band order |
| `~/.switchbard/cache/github-delivery-v1/<binding-id>.json` | one versioned latest-success snapshot, maximum 500 Project items | disposable projection |
| in-memory `DeliveryCache` | current generation, refresh state, parsed snapshot | disposable projection |
| GitHub | Project and delivery state | authoritative remote delivery ledger |

Each cache file is written atomically with mode 0600 where supported. A malformed, wrong-version, over-bound, wrong-host, wrong-binding, or wrong-viewer cache entry is ignored with an explicit warning. One corrupt binding file cannot erase other bindings. Removing a binding may remove its cache because the cache holds no intent or work.

One binding refresh has `MAX_PROJECT_PAGES = 5`, `PROJECT_ITEMS_PER_PAGE = 100`, `MAX_ENRICHED_ARTIFACTS = 20`, `MAX_DETAIL_REQUESTS_PER_ARTIFACT = 3`, and `MAX_REQUESTS_PER_BINDING = 65`. The five Project membership requests establish at most 500 ordered memberships and content identities. The detail budget then enriches linked local artifacts first and remaining artifacts in Project order. Each enriched artifact may spend at most three requests: issue/PR and up to 10 closing PRs; up to 50 reviews, 100 commits, and 100 check runs; then up to 20 releases and 20 deployments reported for observed commits. No descendant connection paginates beyond those caps. A nonempty `hasNextPage`, unsupported relationship, exhausted artifact/detail/request budget, or failed detail request preserves observed edges but marks connection completeness `Unknown` with `ConnectionTruncated`, `UnsupportedRelationship`, `BudgetDeferred`, or the transport reason. Only a successful fully bounded empty connection is known-empty.

The worker refreshes on the existing slow-probe shape, records its cadence in the workers policy table, respects rate-limit reset, and never performs network work on the render path. Rendering exposes at most 100 remote rows per source page before an explicit “Show 100 more” action.

## Provenance and immutability

Every snapshot records binding id, source URL, host, observer login when available, Project node id, generation, `observed_at_ms`, `attempted_at_ms`, page completeness, and per-item artifact and membership node ids. Every displayed remote field inherits that snapshot provenance and may also carry GitHub source-updated time.

Global node ids use GitHub's new format and remain opaque. Repository coordinates and URLs are re-resolved labels. A successful transfer or rename may update displayed coordinates, but the original task reference remains unchanged. Redirects are convenience, not identity or permanence evidence.

## Failure, migration, and rollback

There is no task migration. Missing config fields default to no GitHub sources. The source-binding config addition and cache are additive; an older build ignores the unknown config field and continues reading task files.

Rollout order is domain types and synthetic cases, read-only adapter, config/source commands, cache and worker, queue projection, atomic dispatch-success mutation, IA V2 consumers, live Lucella proof. No stage writes GitHub.

Before the first production write, rollback is deletion of unshipped code and decision artifacts. After source bindings exist, code rollback leaves an additive ignored config field. Cache rollback deletes disposable cache files. After a task gains a PR reference, rollback must preserve that reference because it is valid user-visible evidence, not implementation debris.

Illegal states and rejection boundaries are enumerated in `synthetic-invalid-cases.json`; their zero-effect expectations are mapped in `testing-matrix.md` and `acceptance.md`.
