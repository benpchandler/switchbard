---
id: TASK-163
title: Decouple filtering from task entities
status: To Do
assignee: []
created_date: '2026-09-07 10:05'
labels:
  - abstraction
  - refactor
  - filtering
dependencies:
  - TASK-162
priority: medium
project: Abstraction
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers adding non-task lists must reimplement matching, so users risk different search and facet semantics across otherwise similar lists. Priority is medium because current task filtering works but extension duplicates policy.

Evidence: crates/switchbard-tui/src/tasks.rs:176 FilterField::values_of and :239 Filter::matches accept BacklogTask; the filter vocabulary at :127 is task-specific. Verified with rg -n "BacklogTask|fn .*match" on tasks.rs. Existing TASK-141.2 owns PR filter parity; this task owns remaining shared matching architecture.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Separate parsing and predicate evaluation from entity-specific field access using the shared column contract.
- [ ] #2 Tasks and an existing non-task list use the same applicable matching engine through explicit adapters, with unsupported fields handled predictably.
- [ ] #3 Tests cover text plus categorical composition, negation where supported, multi-valued fields, missing values, invalid expressions, clear/reset and zero matches.
- [ ] #4 Search, filter pickers and paint predicates retain equivalent matching semantics; existing task queries and page isolation remain compatible.
<!-- AC:END -->
