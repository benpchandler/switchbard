---
id: TASK-162
title: Extract reusable column definitions and sorting
status: To Do
assignee: []
created_date: '2026-09-07 10:05'
labels:
  - abstraction
  - refactor
  - sorting
  - columns
dependencies: []
priority: medium
project: Abstraction
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers adding non-task lists must duplicate task-specific column access and ordering, allowing users to see inconsistent sorting and grouping across features. Priority is medium because this is an extensibility and consistency gap, not a demonstrated outage.

Evidence: crates/switchbard-tui/src/columns.rs:28 defines ColumnSpec, but values/cell_text/display_text at lines 226/249/264 take BacklogTask; src/sort.rs owns task ordering. Verified using rg -n "BacklogTask|pub struct" on columns.rs. Extend the TASK-132 catalog rather than replacing it with a competing authority.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define explicit column identities, value accessors and capabilities usable by Tasks and one existing non-task list, with feature-specific labels and semantic order retained.
- [ ] #2 Both consumers use shared sorting logic with deterministic ties, ascending/descending behavior, missing values, and categorical/numeric semantics covered by tests.
- [ ] #3 Column display, filtering, sorting and grouping consult the same capability definitions; multi-valued/non-groupable columns remain explicit.
- [ ] #4 Existing task column configuration, rank ordering and grouping remain compatible; document how to add one new column without duplicating shared algorithms.
<!-- AC:END -->
