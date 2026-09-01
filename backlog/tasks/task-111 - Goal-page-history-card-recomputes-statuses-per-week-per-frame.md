---
id: TASK-111
title: Goal page history card recomputes statuses per week per frame
status: To Do
assignee: []
created_date: '2026-09-01 08:20'
labels:
  - goals
  - perf
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
places/goals.rs ~738-753 calls compute_goal_statuses (O(goals x tasks), backlog_stats.rs:303) once per recorded week, every frame.

Impact: goal page frame cost grows unbounded with a repo's goal-tracking history length, degrading render responsiveness the longer a team has been running weekly goals.

Evidence: places/goals.rs ~738-753 (per-week compute_goal_statuses call in the history card); backlog_stats.rs:303 (compute_goal_statuses's O(goals x tasks) complexity).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 History-card status computation cached or scoped to avoid O(weeks x goals x tasks) work per frame
- [ ] #2 Perf smoke added, or a reasoned complexity bound documented if a smoke test isn't warranted
<!-- AC:END -->
