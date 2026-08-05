---
id: TASK-16
title: Cross-repo Statistics dashboard + burndown
status: To Do
assignee: []
created_date: '2026-08-05 03:10'
updated_date: '2026-08-05 03:13'
labels:
  - hub
  - parity
dependencies:
  - TASK-15
priority: medium
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Tier-2 parity item pulled into the active workstream (owner 2026-08-04): totals, completion %, status and priority distributions, computed ACROSS all repos with per-repo breakdown, PLUS burndown info: completed-over-time vs remaining, overall and per-milestone. Data constraint: derive from existing task metadata (created/updated timestamps, status, completed/ source, milestone field) — no new stores, no schema changes. Note: Backlog v1.47 has no due-date field, so burndown is completion-trend, not due-date-based. Design per TASK-14 direction B (instrument-gauge header language).
<!-- SECTION:DESCRIPTION:END -->
