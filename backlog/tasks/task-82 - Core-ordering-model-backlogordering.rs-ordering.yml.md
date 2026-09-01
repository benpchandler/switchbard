---
id: TASK-82
title: 'Core ordering model: backlog/ordering.rs + ordering.yml'
status: To Do
assignee: []
created_date: '2026-08-31 22:01'
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
- [ ] #1 backlog/ordering.rs loads, validates, and line-surgically writes backlog/ordering.yml; a malformed file warns and loads empty
- [ ] #2 compute_ranked_order (name TBD) yields expedite-first flattened repo order with sparse fallback to compare_tasks, covered by unit tests including empty/partial/stale-id fixtures
- [ ] #3 Stale ids are ignored on read and pruned on the next write to their scope, proven by a test
- [ ] #4 Ordering rides load_backlog_repo into snapshots with no new IO or worker
<!-- AC:END -->
