---
id: TASK-85
title: 'GUI: reorder controls and expedite toggle'
status: To Do
assignee: []
created_date: '2026-08-31 22:01'
labels:
  - backlog
  - gui
  - design
dependencies:
  - TASK-82
  - TASK-84
priority: high
project: Stack Ranking
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Interactive GUI half (trajectory: 'Stack ranking'). Move up / move down controls on project rows and task rows in the backlog view, plus an expedite/unexpedite toggle on task rows; all mutations write through backlog/ordering.rs (one-writer invariant) and refresh via the existing rebuild path. v1 is buttons, not drag-and-drop, unless the design pass says otherwise. Run the design-state skill before building: enumerate states (ranked/unranked rows, first/last position, expedited, mixed sparse lists, empty ordering file) and bind each to evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 design-state matrix completed for the reorder controls before implementation, recorded in this task's notes
- [ ] #2 Move up/down on project and task rows reorders within the sibling scope and persists via backlog/ordering.rs
- [ ] #3 Expedite toggle adds/removes the task from the lane and the row marker updates in place
- [ ] #4 First/last rows disable the no-op direction; unranked rows can be pulled into the ranked set by moving them
<!-- AC:END -->
