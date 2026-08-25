---
id: TASK-47
title: 'Board: per-column task creation + collapsible detail rail'
status: In Progress
assignee: []
created_date: '2026-08-25 00:19'
labels: []
dependencies: []
priority: medium
ordinal: 47000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Creating a task in a particular column meant opening the global composer and re-picking the status the click had already implied. Board columns now carry their own '+ Add task' affordance that preselects that column's status, rendered full-height in an empty column so a column with nothing in it still offers a target. All creation entry points route through create::open_new_task, which owns project targeting and clears any retained subtask parent - the global '+ Task' control and the per-column affordances could otherwise drift on either, and a retained parent would silently make a top-level task a subtask. Also collapses the detail rail to a narrow edge toggle (session-only state; egui panel memory still owns the dragged width). Recorded after the fact: the work landed as a local commit that was never PR'd.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Per-column + Add task preselects that column's status
- [ ] #2 Empty column renders the affordance as a full-height target
- [ ] #3 All creation entry points route through create::open_new_task
- [ ] #4 Detail rail collapses to an edge toggle; dragged width persists
<!-- AC:END -->
