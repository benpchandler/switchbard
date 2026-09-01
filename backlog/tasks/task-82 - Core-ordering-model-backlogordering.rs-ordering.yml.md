---
id: TASK-82
title: 'Core ordering model: backlog/ordering.rs + ordering.yml'
status: Done
assignee: []
created_date: '2026-08-31 22:01'
updated_date: '2026-08-31 23:48'
labels:
  - backlog
  - backend
dependencies: []
priority: high
project: Stack Ranking
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The domain half of stack ranking (trajectory: 'Stack ranking'). A new switchbard-core module backlog/ordering.rs owns backlog/ordering.yml: an expedite list of task ids, a ranked project-name list, per-project ranked task-id lists, and per-parent ranked sub-issue lists. Reads are tolerant (malformed file warns and loads empty, never fails repo load; entries naming done/archived/missing ids are ignored). Writes are line-surgical YAML edits through the shared write layer (goals.rs precedent) and prune stale ids from the scope they touch. The repo-wide next-up order is computed, never stored: expedite lane first, then flatten top-ranked project's task stack downward; sparse fallback everywhere to the existing compare_tasks comparator. Rank must NOT be written into task frontmatter, and ordinal stays unwritten.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 backlog/ordering.rs loads, validates, and line-surgically writes backlog/ordering.yml; a malformed file warns and loads empty
- [x] #2 compute_ranked_order (name TBD) yields expedite-first flattened repo order with sparse fallback to compare_tasks, covered by unit tests including empty/partial/stale-id fixtures
- [x] #3 Stale ids are ignored on read and pruned on the next write to their scope, proven by a test
- [x] #4 Ordering rides load_backlog_repo into snapshots with no new IO or worker
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Shipped as backlog/ranking.rs + backlog/ranking.yml (renamed from the planned ordering.rs/ordering.yml: the hub's cross-repo triage overlay is already a root-level ordering.yml/OrderingOverlay, and two files sharing one name would be a findability trap - trajectory entry updated). Flatten refinement found by a failing test: sibling ranks are never compared across scopes; the comparator walks the two tasks' ancestor chains and compares true siblings only (rank, then today's comparator), so a parent always precedes its sub-issues and a partially ranked repo groups families. Fully unranked repos keep today's comparator byte-for-byte. Rank applies within the source/status tiers: expedite leads To Do but never floats above In Progress. Writes prune stale ids from the touched scope and fail closed on a malformed or restyled file.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
backlog/ranking.rs owns backlog/ranking.yml: tolerant reads (missing=empty, malformed warns+empty, load never fails), line-surgical scope-block writes that prune stale ids and fail closed on unrecognizable structure, RankPlacement (Top/Before/After) write ops for tasks and projects plus expedite/unexpedite, and sort_tasks - the computed repo-wide flatten (expedite lane, project rank, ancestor-chain sibling walk) applied inside load_backlog_repo so every snapshot rides it with no new worker. BacklogRepo carries the loaded RepoRanking for surfaces. 16 unit tests incl. round-trip, stale-prune, surgical-diff, fail-closed, and hierarchy-flatten fixtures; mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
