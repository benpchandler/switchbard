---
id: TASK-54
title: Sort control available on every lens that shows the filter row
status: Done
assignee: []
created_date: '2026-08-26 00:40'
updated_date: '2026-08-26 00:42'
labels: []
dependencies: []
priority: medium
ordinal: 54000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Board and Milestones already drew from the same sorted visible_task_rows, so the ordering applied everywhere - only the control was List-only, leaving those lenses sorted by a key their user could neither see nor change. board.rs even carried a comment explaining Board had no toolbar row to attach it to; the shared toolbar container from the tidy-up made that moot. Moved into the shared toolbar next to the filters.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Sort control renders on Board
- [x] #2 Stale comment in board.rs refreshed
<!-- AC:END -->
