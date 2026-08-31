---
id: TASK-74
title: 'GUI: current-week goal cards on the Digest lens'
status: To Do
assignee: []
created_date: '2026-08-31 17:02'
labels:
  - goals
  - gui
dependencies:
  - TASK-72
priority: medium
project: Weekly goals
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
One card per current-week goal: progress bar, actual/target with unit, pace verdict in existing pill colors, days remaining. Invoke design-state before implementing; goals data rides the snapshot (no per-frame IO); perf smokes green. History-by-week view on the Statistics lens is a follow-up, not this task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Design-state matrix bound to tests (zero goals, met, behind, long names)
- [ ] #2 No per-frame goal scans outside the worker snapshot
<!-- AC:END -->
