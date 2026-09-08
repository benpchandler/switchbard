---
id: TASK-185
title: Preserve live work claims when reparenting changes a task ID
status: To Do
assignee: []
created_date: '2026-09-08 13:55'
labels:
  - tui
  - hierarchy
  - bug
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: users moving an actively claimed task lose its live-work marker on the new ID, while the agent session still holds the old ID. Evidence: parent-picker integration review in feat/tui-parent-picker; move_backlog_task in crates/switchbard-core/src/backlog/mutations.rs renames dependency, ranking and goal references but never work claims; work_sessions.rs::holds and sessions_working compare the old task_id exactly. The existing sb edit --parent path has the same gap. Decide whether to migrate claims through the owning work-session write layer or prevent moves while claimed; never silently orphan claims.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A disk-backed journey claims a task, reparents it, and proves there is no invisible or stranded claim on the old ID.
- [ ] #2 The chosen behavior is shared by CLI and TUI and handles multiple sessions plus mutation failure without dropping live work.
<!-- AC:END -->
