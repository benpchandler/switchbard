---
id: TASK-166
title: Generalize saved views across list features
status: To Do
assignee: []
created_date: '2026-09-07 10:05'
labels:
  - abstraction
  - refactor
  - views
  - persistence
dependencies:
  - TASK-162
  - TASK-163
  - TASK-164
priority: medium
project: Abstraction
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers adding list features must reproduce task-specific view persistence, risking lost settings or cross-page leakage for users. Priority is medium because current task persistence exists but does not offer an entity-neutral contract.

Evidence: crates/switchbard-tui/src/views.rs:19 ViewState bundles filter, sort, columns, glyphs, paint and grouping but imports task Column/Sort/Grouping types. Verified with sed -n "1,75p" on views.rs. Extend the single ViewState authority from TASK-130 and reconcile TASK-141.1; TASK-153 and TASK-154 retain startup-resume and history outcomes.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Represent reusable view settings against explicit list/column identities with feature capabilities and separate per-feature scope.
- [ ] #2 Tasks and an existing non-task list round-trip applicable filter, sort, columns, glyphs, paint and grouping settings without cross-page leakage.
- [ ] #3 Existing saved task views load compatibly; missing/renamed/unsupported columns and malformed or older records have a documented, tested non-destructive policy.
- [ ] #4 Slot save/load and restart tests prove compatibility and feature isolation; extend the existing serialization authority and coordinate with TASK-153/TASK-154 rather than creating a second persistence store.
<!-- AC:END -->
