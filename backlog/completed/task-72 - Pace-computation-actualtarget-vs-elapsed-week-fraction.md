---
id: TASK-72
title: 'Pace computation: actual/target vs elapsed week fraction'
status: Done
assignee: []
created_date: '2026-08-31 17:02'
updated_date: '2026-08-31 17:53'
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
- [x] #1 Verdict tests for all four states incl. week boundaries
- [x] #2 tasks-measure actual matches project/label scope and excludes archived
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
compute_goal_statuses in backlog_stats: manual actual = latest check-in (ties -> last entry), tasks actual = done non-archived tasks matching scope by project or label with updated_date in the goal week; verdict Met/Missed terminal, OnTrack/Behind by integer cross-multiplication of actual/target vs days/7; week_monday_of helper; progress/week fraction methods for the GUI bar+tick. Tests cover all four verdicts, Sunday-vs-Monday boundary, absence-not-zero for unset weeks, and the scope matrix. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
