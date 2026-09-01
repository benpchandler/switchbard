---
id: TASK-110
title: Goal check-in drafts survive week rollover with stale values
status: To Do
assignee: []
created_date: '2026-09-01 08:20'
labels:
  - goals
  - gui
  - correctness
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
goal_checkin_drafts is keyed (repo, goal) without week and seeded via or_insert (runtime/mod.rs ~306; consumers ui/places/goals.rs ~262, ui/backlog/digest.rs ~376): across a calendar rollover or a Roll action, a stale draft silently submits as the NEW week's cumulative total.

Impact: a user's weekly goal actuals get silently corrupted - a manual check-in can submit a stale prior-week value as this week's cumulative total, undermining pace tracking for that goal and anyone relying on its reported status.

Evidence: runtime/mod.rs ~306 (goal_checkin_drafts keyed (repo, goal), or_insert seeding with no week component); ui/places/goals.rs ~262 and ui/backlog/digest.rs ~376 (both consumers read the draft without a week key).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Draft keyed by (repo, goal, week), or explicitly cleared on roll/rollover
- [ ] #2 Regression test proving a draft from week N does not leak into week N+1's check-in
<!-- AC:END -->
