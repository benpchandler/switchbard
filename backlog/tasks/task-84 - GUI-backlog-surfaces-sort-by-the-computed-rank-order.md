---
id: TASK-84
title: 'GUI: backlog surfaces sort by the computed rank order'
status: To Do
assignee: []
created_date: '2026-08-31 22:01'
labels:
  - backlog
  - gui
dependencies:
  - TASK-82
priority: high
project: Stack Ranking
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Read-only GUI half first (trajectory: 'Stack ranking'). Backlog views (project list, task lists within a project, sub-issue lists) sort by the computed next-up order from backlog/ordering.rs: expedite lane first where a flat task view exists, ranked items in rank order, unranked items by the existing comparator. Expedited tasks get a visible marker (theme.rs semantic color/glyph). No reorder controls in this slice. Respect render-path perf discipline: ordering comes from the snapshot, no per-frame file reads.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Projects and tasks render in computed rank order across backlog surfaces; unranked items keep today's relative order
- [ ] #2 Expedited tasks carry a visible marker sourced from theme.rs
- [ ] #3 SWITCHBARD_PERF smoke shows no p95 regression vs previous build on the touched surfaces
<!-- AC:END -->
