---
id: TASK-73
title: 'CLI goal verb family: create/list/view/check-in/roll'
status: To Do
assignee: []
created_date: '2026-08-31 17:02'
updated_date: '2026-08-31 17:38'
labels:
  - goals
  - cli
dependencies:
  - TASK-72
priority: high
project: Weekly goals
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Mirror the project/initiative families (payload-only stdout, one-line stderr errors): goal create --week --target --unit [--measure --scope], goal check-in <NAME> <N> (appends dated observation), goal list (name, week, actual/target, pct, pace), goal view, goal roll (clone last week's goals into the new week). Help text is the output contract.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Integration tests through the real binary incl. check-in append and roll
- [ ] #2 clap debug_assert stays green
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
goal roll now means 'add a week key with last week's targets' in goals.yml, not cloning files.
<!-- SECTION:NOTES:END -->
