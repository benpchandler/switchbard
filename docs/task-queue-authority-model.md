# Task Queue: authority and identity model (TASK-80.2)

Status: **draft for owner review** - the model is not locked until this doc is
owner-approved; the approved decision then gets a trajectory-doc entry and this
file becomes its detail record.

Scope: the *data* half of TASK-80.2. Where the Task Queue surface lives and
what it looks like is deliberately out of scope here - that belongs to the IA
V2 decision record (TASK-77, per the owner's 2026-09-01 note).

The Task Queue is the surface where the user tees up tasks for dispatch and
sees what dispatch is working on. Switchbard owns priority, orchestration
context, dependencies, assignments, leases, and outcome acceptance. GitHub
remains the authoritative delivery ledger. This doc pins down what that
sentence means field by field, so persistence and UI work cannot quietly
invent a second authority.

## 1. Record kinds and identities (AC #1)

Three kinds. Identities never merge; a "linked" pair is an edge between two
records, not one record with two faces.

| Kind | Identity | Storage |
|---|---|---|
| **Switchbard-native task** | `(repo root, task id)` - the exact key every existing surface uses (`BacklogTaskKey`, `HiveApp::backlog_repos`) | Backlog markdown, unchanged (AC #4: no migration) |
| **GitHub delivery record** (issue, PR, commit, check run, release, deployment) | GitHub `node_id` (GraphQL global id) as the primary key; `owner/repo` + number/SHA/tag as *display coordinates* | Observation cache only (see §4) - never task files |
| **Link** | the ordered pair (task identity, GitHub identity) plus a link kind (`delivers`, `tracks`, `mentions`) | On the native task: canonical GitHub URLs in the `references:` frontmatter list |

Decisions inside that table:

- **`node_id`, not `owner/repo#number`, is the GitHub identity.** Numbers and
  repo names are not stable: a repository transfer or rename changes the
  coordinates while `node_id` survives. Coordinates are derived fields with
  provenance (§3), re-resolved from the id, never the reverse.
- **Links live in `references:`.** It already exists in the Backlog format,
  already holds URLs, and already round-trips through the write layer - so
  the edge store is migration-free. Dispatch's `Dispatch PR: <url>` notes
  line remains as a human breadcrumb; when dispatch releases a run it also
  appends the URL to `references:`, which becomes the one machine-read edge.
  (Additive: existing tasks whose PR link lives only in notes gain the
  reference the next time anything touches them; a missing edge degrades to
  "unlinked", never to wrong data.)
- **A GitHub-backed queue item with no local task is a projection, not a
  task.** It renders in the queue from the observation cache and its GitHub
  identity; no shadow task file is created. Creating a local task for it is
  an explicit user action that also writes the link. This is what "without
  requiring duplicate task creation" (TASK-80 AC #1) means concretely.

## 2. Field authority (AC #2)

Exactly one authority per mutable field. Switchbard never writes
GitHub-authoritative fields in v1 (write-back is a non-goal, §7), and nothing
derived is ever written into a task file.

**Switchbard-authoritative** (mutated only through the native write layer):

| Field | Authority |
|---|---|
| Priority / queue order | `backlog/ranking.yml` computed flatten (expedite lane, project rank, sibling walk) - the one order the queue tees up from; no second priority representation |
| Task status, title, description, ACs, DoD, plan, notes | task frontmatter/sections |
| Dispatch claim + lease | dispatch label ladder (`dispatch` / `dispatching` / `dispatched` / `dispatch-failed`) cross-checked by `dispatch_inspect` liveness (`looks_orphaned`) - the queue *reads* this, it adds no run store |
| Assignments, dependencies, project membership | task frontmatter |
| Outcome acceptance | a human moving the task to Done; a merged PR never auto-completes a task |
| Links (`references:`) | the native task file |

**GitHub-authoritative** (read-only observations, always carrying provenance):

issue state, PR state / merge state / review state, check conclusions,
commit reachability, release and deployment state, and - for GitHub-backed
items sourced from a GitHub Project - that Project's *internal* item order.

**The composition rule for order:** where GitHub-backed items slot relative
to Switchbard work is Switchbard-authoritative (project rank / expedite);
the order *among* items inside one linked GitHub Project is
GitHub-authoritative. Neither side ever rewrites the other. Cross-repo
composition stays with the deferred hub `ordering.yml` overlay.

## 3. Provenance on every derived field (AC #2)

Every GitHub-derived value is an observation:
`(value, source, observed_at_unix)` - millisecond precision, same rationale
as `BacklogRepo::loaded_at_unix` (two observations of the same thing easily
land in the same second exactly when the ordering matters). "Last refresh"
in the UI (TASK-80 AC #2) renders `observed_at`, never a guess.

Precedent to copy, not reinvent: `landing.rs` already splits the cheap local
fact (`PushState`, a `rev-parse`) from the networked fact (`PrState`, via
`gh`) and derives the display state as a pure function of both, degrading to
"unknown" instead of blocking or guessing. Every adapter observation class
follows that shape: probe on its own cadence, cache with `observed_at`,
derive purely.

## 4. Reconciliation and freshness

- **Probe-and-derive, never store merged truth.** The observation cache
  (identity-keyed, in-memory plus an on-disk cache file outside `backlog/`)
  is disposable; deleting it loses nothing but freshness. Task files are
  never written by reconciliation - the one-writer invariant stands: only
  user actions and the dispatch lifecycle write tasks.
- **Staleness is displayed, not hidden.** An old observation renders with
  its age; it is never silently dropped (an empty queue lies harder than a
  stale one) and never silently trusted as current.
- **Cadence per class**, mirroring the workers table's discipline: local git
  facts on the cheap tick; `gh` facts on a slow tick with backoff, and
  rate-limit responses respected via their reset time.

## 5. Degraded and hostile states (AC #3)

Per-observation `Unknown` carries a reason; a record-level failure never
erases field-level facts that did resolve (partial observation is normal,
not an error).

| State | Behavior |
|---|---|
| No `gh` / not authenticated / insufficient scope | `Unknown(reason)` on every GitHub field; queue still renders Switchbard-native facts fully |
| Rate-limited | `Unknown(rate-limited until T)`; no retry before T |
| Stale | value shown with `observed_at` age |
| Repository transferred / renamed | `node_id` re-resolves; coordinates update with new provenance; links (URLs) are re-canonicalized on next touch, old URLs still resolve via the id |
| Record deleted / inaccessible (404/410) | the link's state becomes `Gone(observed_at)` - the edge is never silently dropped, and absence **never** implies Done/merged/released |
| Partially observed (some sub-queries failed) | per-field `Unknown`, resolved fields keep their values |

The spine, inherited from `removal_safety`: **an unanswered question never
counts as an answered one.** No `Unknown` state may ever produce a false
Done, merged, released, or deployed claim (TASK-80 AC #4).

## 6. Conflicts

By construction there are none at the field level - no field has two
authorities. What remains are *disagreements between facts* (task marked
Done while its PR is open; task In Progress while its dispatch claim looks
orphaned). These are surfaced side by side and left to the human; nothing
auto-resolves, because each fact is already true within its own authority.

## 7. Non-goals (v1)

Write-back to GitHub (issue edits, PR actions), storage migration of any
kind, cross-repo order composition (hub overlay, deferred), and two-way
GitHub Projects sync. The Lucella GitHub Project 3 dogfood (TASK-80 AC #5)
is read-only consumption under §2's composition rule.

## AC mapping

- AC #1 -> §1 (three kinds, edges not merges, projections for unlinked items)
- AC #2 -> §2 (one authority per field) + §3 (provenance and observation time)
- AC #3 -> §5 (explicit behavior per degraded state)
- AC #4 -> §1 + §4 (references-based links, observation cache outside
  `backlog/`, no task-file writes from reconciliation)
