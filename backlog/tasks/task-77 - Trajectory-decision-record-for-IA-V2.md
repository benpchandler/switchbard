---
id: TASK-77
title: Trajectory decision record for IA V2
status: Done
assignee:
  - '@claude'
created_date: '2026-08-31 21:20'
updated_date: '2026-09-01 02:24'
labels:
  - ia
  - docs
dependencies:
  - TASK-76
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After the mockup settles direction: record the IA decision in docs/product-trajectory.md (what changes, what is deliberately kept, migration of persisted lens/saved-view state) and define the implementation tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Decision recorded; implementation tasks defined under this project
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner (2026-09-01): the Task Queue's meaning and placement get worked in this redesign - it is the surface where the user tees up tasks for dispatch and sees what dispatch is working on (see TASK-80.2 notes for the priority-authority link to backlog/ranking.yml). The decision record should say where that surface lives in the new IA, alongside the existing 'where Dispatches lands as a facet' question from TASK-76.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decision record written into docs/product-trajectory.md under Planned: 'Information architecture V2 - places and objects (owner-approved 2026-09-01)' - places/scope model, Tasks-primacy with generic grouping and the project-page cut, Dispatches view + Command fleet console, Digest attention feed, Ops merge/rename, Linear-style favorites, implementation obligations (stroke selection/TASK-38, AccessKit-from-verb-names, TASK-13 virtualization, TASK-78 dark-warn, TASK-92 Inputs card), rejected alternatives, and UiConfig.filters key migration. Weekly-goals entry amended for input goals (TASK-92) and CLAUDE.md's goals paragraph synced. Implementation tasks defined under the project: TASK-96 sidebar shell + scope + favorites, TASK-97 Tasks place, TASK-98 Dispatches + Command, TASK-99 Digest, TASK-100 Ops, TASK-101 Goals surfaces.
<!-- SECTION:FINAL_SUMMARY:END -->
