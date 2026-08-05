---
id: TASK-25
title: 'Board: add the Icebox column (union of statuses across tracked repos)'
status: To Do
assignee: []
created_date: '2026-08-05 14:02'
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
