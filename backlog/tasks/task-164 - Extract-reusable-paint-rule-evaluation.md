---
id: TASK-164
title: Extract reusable paint rule evaluation
status: To Do
assignee: []
created_date: '2026-09-07 10:05'
labels:
  - abstraction
  - refactor
  - paint
dependencies:
  - TASK-162
  - TASK-163
priority: medium
project: Abstraction
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers extending list styling must couple new features to task fields and Ratatui colors or duplicate precedence logic, risking inconsistent highlights for users. Priority is medium because the gap affects reuse and future consistency.

Evidence: crates/switchbard-tui/src/paint.rs:8 imports BacklogTask, rule evaluation at :137 and field_value at :171 bind rules to tasks; :179 also accepts BacklogTask. Verified with rg -n "BacklogTask|field_value" on paint.rs. TASK-141.4 owns PR paint parity, and TASK-152 owns palette-token persistence; coordinate rather than duplicate those changes.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Rule matching and precedence operate on the shared field/column contract for Tasks and an existing non-task list.
- [ ] #2 Document and enforce the boundary between semantic paint decisions and frontend color/style conversion; do not introduce UI dependencies into core.
- [ ] #3 Tests prove category, row and column rule precedence, multiple matches, no match, missing fields and invalid rule handling, preserving existing results.
- [ ] #4 Current paint configuration remains compatible and palette-token behavior respects TASK-152; no second matcher is introduced.
<!-- AC:END -->
