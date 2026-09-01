# Decision: Task Queue authority and identity model

Status: owner-approved data contract on 2026-09-01; production implementation remains unapproved and unstarted.

## Objective

Lock a migration-free, read-only GitHub delivery model that preserves one authority per field and produces one deterministic Task Queue without duplicate task records.

## Context

Switchbard already owns repo-local Backlog task files, their native write layer, stack ranking, dispatch claims, run inspection, and human outcome acceptance. GitHub owns repository-controlled delivery facts such as issues, pull requests, reviews, checks, commits, releases, and deployments. The missing contract is how both authorities appear in one Task Queue without creating shadow task files, allowing two writers, or letting an unavailable GitHub answer become false completion.

This decision is consequential because identity, source configuration, ordering, cache semantics, and dispatch-link writes become compatibility promises as soon as production code persists them. The previous draft left four contradictions: GitHub Project order had no Project membership identity; remote-only items could not participate in `backlog/ranking.yml`; typed link kinds could not fit the existing string-only `references:` field; and a successful dispatch could update its label before recording the machine-readable PR link.

The Task Queue is the task-delivery surface where users tee up local tasks for dispatch, see dispatch custody and progress, and inspect configured GitHub Project work. IA V2 separately owns its placement under Tasks / Dispatches and the Command fleet console. This package locks the data contract only.

## Chosen model

1. **Keep identities separate.** A native task, configured GitHub Project source, GitHub Project membership, GitHub artifact, and local-to-GitHub link remain separate records or edges. A queue projection may compose them but never merges their identities.
2. **Configure sources explicitly.** A user action adds a `GitHubProjectBinding` to Switchbard config. The binding has a Switchbard-minted stable id, GitHub host, Project URL locator, tracked repo scope, and list position. Source selection and source-band order are Switchbard-authoritative intent; Project identity and contents are GitHub observations.
3. **Use deterministic source bands, not synthetic cross-authority rank.** The local dispatch band contains dispatch-labeled native tasks in the one stack-rank order owned by `load_backlog_repo`. Configured GitHub Project bands follow in binding-list order; items within each band retain GitHub Project item order. History is inspectable but excluded from next work. Remote-only items are not individually expedited or ranked in v1. Adopting one creates a linked local task, after which Switchbard rank applies and the standalone remote projection disappears.
4. **Keep links generic in v1.** A link is the ordered pair of a native task identity and a resolved GitHub artifact identity. Its durable locator remains a canonical GitHub URL string in the existing `references:` list. V1 does not persist `delivers`, `tracks`, or `mentions`; semantics that cannot be represented are not invented.
5. **Observe GitHub, never write it.** The adapter exposes reads only. Every remote field is a typed observation with source, observation time, and explicit Unknown reason. GitHub failure cannot mutate task bytes, mark a task Done, or produce merged, released, or deployed claims.
6. **Persist only rebuildable snapshots.** A versioned, bounded, per-binding cache outside `backlog/` keeps the last successful observation across restarts. It is an optimization and stale-evidence carrier, never authority. Removing it loses only freshness.
7. **Record dispatch success atomically and idempotently.** One native write-layer command accepts the task id, canonical PR URL, and optional caller note; changes `dispatching` to `dispatched`; sets `In Review`; appends one canonical note entry containing the PR line plus the optional caller text; and adds the PR URL to `references:` in one task-file replacement. If the write fails, the task remains claimed and retryable. Replaying the same PR URL and normalized optional note is a no-op; a different PR URL or different optional note is a conflict.
8. **Derive presentation state.** Linked local tasks remain local queue entries with nested delivery evidence. A GitHub item with no resolved local link is a remote projection. If the same artifact occurs in multiple configured Projects, one projection uses the highest source-band position while retaining every observed membership as provenance.
9. **Keep next work separate from dispatch history.** The source-band Task Queue owns tee-up order. The existing Dispatches activity monitor retains Active, Queued, Finished, and Failed facets and newest-run-first history as a subordinate progress/history section; its history order is not presented as queue priority.

## Invariants

1. A GitHub observation can never mutate GitHub or a native task.
2. A merged PR, successful deployment, empty result, resolved annotation, or silent refresh never implies local outcome acceptance.
3. Native task identity is `(repo root, task id)`; a GitHub artifact identity is `(host, opaque new-format node id)`; mutable coordinates are labels, not identity.
4. A GitHub Project membership is an observed edge from a Project item node to its content node and owns the item position within that Project.
5. Source binding selection and source-band order are Switchbard-authoritative and durable; Project contents and within-band order are GitHub-authoritative observations.
6. Native task priority, explicit rank, and derived queue order are distinct facts with distinct authorities.
7. The local queue order has one core implementation consumed by the GUI, `sb queue`, and the Rust fallback dispatcher.
8. Existing task files remain readable and writable without migration; no shadow task is created by refresh.
9. A successful local dispatch release includes its PR reference in the same atomic task-file revision as its terminal dispatch label.
10. Unknown, stale, missing-or-inaccessible, partial, and rate-limited states never collapse into a negative answer or success claim.
11. A prior successful observation remains Stale after a failed refresh; a source with no successful observation is Unavailable. Per-field omissions remain Unknown.
12. A 404, null node, or permission-masked response is `MissingOrInaccessible`, never proof of deletion.
13. Repository transfers and renames update derived coordinates when a node lookup succeeds; stored task URLs are not automatically rewritten.
14. A successful empty connection means known-empty only when the adapter completed every bounded page. Truncation or omitted connections remain Unknown.
15. Cache loss, corruption, source removal, or viewer change cannot lose native work or fabricate GitHub state.
16. Adoption is serialized per `(repo root, GitHub node key)` through the git-common-dir reservation mechanism, rechecks all resolvable native references under that reservation, and creates at most one native task when identity is available. An unresolved node cannot be adopted.
17. One binding refresh has a hard total request budget. Items or descendant connections not fetched within that budget remain Unknown with an explicit reason; a bounded partial graph never becomes a complete or negative claim.

## Alternatives considered

### Mirror GitHub issues into task markdown

Rejected because refresh would become a second task writer and issue changes would conflict with local planning fields.

### Merge a local task and GitHub issue into one polymorphic record

Rejected because one record would carry two status, title, assignment, and completion authorities. A link preserves composition without identity collapse.

### Add typed link kinds to `references:`

Rejected for v1 because `references:` stores URL strings only. Adding meaning that cannot round-trip would create a cosmetic contract. A future structured link field requires its own compatibility decision.

### Interleave remote-only items directly into `backlog/ranking.yml`

Rejected for v1 because the existing ranking aggregate is hierarchy-shaped around local task and project identities. Extending it would create a new durable queue-key schema before per-item remote reordering has demonstrated value. Source bands preserve deterministic order without a second priority representation.

### Store merged local-plus-GitHub truth

Rejected because it can drift and creates conflict resolution where none is necessary. Persist observations and derive composition instead.

### Treat 404 as Gone

Rejected because GitHub may hide an inaccessible resource as missing. The honest state is `MissingOrInaccessible` unless a future API supplies positive deletion evidence.

### Keep the dispatch release as several best-effort writes

Rejected because a terminal label without the machine-readable PR edge is an invalid partial success. One intent-level native mutation is the smallest coherent boundary.

## Deferred questions

- GitHub write-back, issue creation, PR actions, Project field updates, and two-way synchronization.
- Per-item ranking or expedite for remote-only work. Reopen only after source-band dogfood demonstrates a real need that explicit local adoption does not cover.
- Cross-repo interleaving through the hub `ordering.yml` overlay. The existing overlay remains deferred and unchanged.
- GitHub Enterprise Server and other forges. V1 accepts `github.com` only but includes host in identity so the domain model does not conflate servers.
- Webhooks, historical observation revisions, and durable audit history. V1 keeps only the latest successful snapshot per binding.
- Typed link kinds. Reopen only with a user-visible behavior that generic association cannot support.
- Exact visual composition. IA V2, the design-state matrix, and TASK-80.4 own rendering.

## Approval boundary

The data contract is approved. The first production implementation, first persisted `GitHubProjectBinding` written to a real `~/.switchbard/config.toml`, first on-disk delivery-cache write, and any GitHub mutation remain outside this decision task. TASK-80.3 and TASK-80.4 require separate implementation authorization and must satisfy this package's verifier rather than silently revising the contract.
