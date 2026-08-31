---
id: TASK-74
title: 'GUI: current-week goal cards on the Digest lens'
status: Done
assignee: []
created_date: '2026-08-31 17:02'
updated_date: '2026-08-31 18:00'
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
- [x] #1 Design-state matrix bound to tests (zero goals, met, behind, long names)
- [x] #2 No per-frame goal scans outside the worker snapshot
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Digest lens leads with 'This week's goals': per-repo cards with pace pill (Good/Warn/Danger kinds), actual/target + unit, progress bar with a today-tick at the elapsed-week fraction (skipped on terminal verdicts), 'auto - scope' badge for task-derived goals, inline DragValue + Check in for manual goals via a Pending intent and spawn_goal_checkin. Section absent entirely with zero goals. State matrix bound to ui_views tests (section present/absent, met pill, check-in affordance); no per-frame IO (goals ride the snapshot); release perf smokes green; CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
