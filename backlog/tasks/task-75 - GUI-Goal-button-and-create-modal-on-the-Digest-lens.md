---
id: TASK-75
title: 'GUI: + Goal button and create modal on the Digest lens'
status: Done
assignee: []
created_date: '2026-08-31 20:33'
updated_date: '2026-08-31 20:38'
labels:
  - goals
  - gui
dependencies: []
priority: medium
project: Weekly goals
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Entry point for creating a goal without the CLI. '+ Goal' button on the goals section header when goals exist; a muted '+ Goal for this week' affordance at the top of Digest when none do (the zero-state section itself stays absent). Opens a New Goal modal: target repo (seeded from the selected repo, picker in All-repos scope, same as the task modal), name, target, unit, measure manual/tasks, scope combo (known projects + free text) required for tasks-measure. Creates for the current week via core create_goal through a Pending intent; errors surface via backlog_status.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Modal opens from both entry points; created goal appears as a card after refresh
- [x] #2 tasks-measure cannot submit without a scope; duplicate-name error surfaces in the status line
- [x] #3 State-matrix tests; no per-frame IO; CI + perf smokes green
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
New Goal modal (goal_create.rs, same shell as the task composer: fixed repo in single-repo scope, picker in All-repos), opened from the goals-section header's + Goal button or the zero-goals '+ Goal for this week' doorway (the empty section itself still never renders). Create disabled without name/unit or without scope on tasks-measure; creates for the current week via Pending::goal_create -> spawn_goal_create -> core create_goal; duplicate-name and other errors surface via backlog_status. Entry-point harness tests; CI + release perf smokes green.
<!-- SECTION:FINAL_SUMMARY:END -->
