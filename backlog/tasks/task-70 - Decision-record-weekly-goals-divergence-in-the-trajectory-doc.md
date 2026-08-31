---
id: TASK-70
title: 'Decision record: weekly-goals divergence in the trajectory doc'
status: To Do
assignee: []
created_date: '2026-08-31 17:02'
updated_date: '2026-08-31 17:38'
labels:
  - goals
  - docs
dependencies: []
priority: high
project: Weekly goals
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Append the weekly-goals entry under Planned in docs/product-trajectory.md per the divergence-on-named-wins rule: goal def files, append-only check-in observations, computed pace, per-week instances, deferred recurrence templates.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Trajectory entry names the win and each divergence
- [ ] #2 Deferred items (recurring templates) recorded as ask-first
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Storage decision changed (owner, 2026-08-31): goals live in one structured backlog/goals.yml per repo, not per-week markdown def files - goals are records, not documents. The trajectory entry must record this shape and the reasoning.
<!-- SECTION:NOTES:END -->
