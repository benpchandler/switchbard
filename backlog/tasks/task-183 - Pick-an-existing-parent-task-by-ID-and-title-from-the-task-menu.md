---
id: TASK-183
title: Pick an existing parent task by ID and title from the task menu
status: In Progress
assignee: []
created_date: '2026-09-08 13:30'
updated_date: '2026-09-08 13:30'
labels:
  - tui
  - hierarchy
  - ball:agent
dependencies:
  - TASK-182
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: TUI users cannot assign a task parent from the task picker and must identify tasks by number outside the interaction flow. Evidence: owner request in this session; current app task chord offers rank, ball, complete, pin and goals without a parent picker. Scope: searchable existing eligible parent choices with ID and title, cancel and remove-parent paths, shared mutation validation and honest outcome feedback.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The task interaction menu opens a parent picker that shows eligible existing task IDs and titles and searches both.
- [ ] #2 Selecting a parent and removing a parent persist through the core mutation layer; cancellation changes nothing and failures remain visible.
- [ ] #3 Real-key TUI tests cover selection, title search, empty results, stale targets, narrow terminal and reopening after save.
<!-- AC:END -->
