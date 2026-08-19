---
id: TASK-25
title: 'Board: add the Icebox column (union of statuses across tracked repos)'
status: Done
assignee: []
created_date: '2026-08-05 14:02'
updated_date: '2026-08-05 15:56'
labels:
  - board
  - ux
dependencies: []
priority: medium
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner-requested UX (2026-08-05): Board columns today are BACKLOG_STATUSES (To Do/In Progress/Done) plus any nonstandard status present in the CURRENT scope's own tasks. Budget's Backlog.md config uses an Icebox status that should show as a column even when the currently-scoped project has zero Icebox tasks, if ANY tracked repo's backlog config defines it. Need: column set becomes the union of statuses across every tracked repo's backlog config (not just tasks currently present), sane fixed order (Icebox, To Do, In Progress, In Review, Done, then any other nonstandard statuses alphabetically).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (commit babd342): column_order unions BACKLOG_STATUSES with every scoped project's configured_statuses (parsed from backlog/config.yml) plus any status actually present on a task, ordered via CANONICAL_STATUS_ORDER. Confirmed no CLI flag exists for setting statuses (hand-edit config.yml in fixtures is acceptable per repo convention, since it's not task data). Unaffected by TASK-29's click/drag defect.
<!-- SECTION:NOTES:END -->
