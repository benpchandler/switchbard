---
id: TASK-72
title: 'Pace computation: actual/target vs elapsed week fraction'
status: To Do
assignee: []
created_date: '2026-08-31 17:02'
labels:
  - goals
  - core
dependencies:
  - TASK-71
priority: high
project: Weekly goals
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
compute_goal_status in backlog_stats: actual from last check-in (manual) or done-in-week tasks matching scope (tasks measure, updated_date within the goal week); verdict on-track/behind/met/missed from actual/target vs elapsed_days/7. Computed, never stored.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Verdict tests for all four states incl. week boundaries
- [ ] #2 tasks-measure actual matches project/label scope and excludes archived
<!-- AC:END -->
