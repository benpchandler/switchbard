---
id: TASK-73
title: 'CLI goal verb family: create/list/view/check-in/roll'
status: Done
assignee: []
created_date: '2026-08-31 17:02'
updated_date: '2026-08-31 17:55'
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
- [x] #1 Integration tests through the real binary incl. check-in append and roll
- [x] #2 clap debug_assert stays green
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
goal roll now means 'add a week key with last week's targets' in goals.yml, not cloning files.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
goal create/check-in/list/view/roll family (goals_cmd.rs) mirroring the sibling families' output contract; --week accepts any date and normalizes to its Monday; check-in refuses tasks-measured goals and echoes actual/target; help text carries the GOALS contract. Integration tests use terminal past weeks and actual==target cases for clock-independent verdicts. clap debug_assert green; CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
