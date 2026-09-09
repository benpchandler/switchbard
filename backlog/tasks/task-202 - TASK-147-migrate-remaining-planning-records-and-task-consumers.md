---
id: TASK-202
title: 'TASK-147: migrate remaining planning records and task consumers'
status: To Do
assignee: []
created_date: '2026-09-08 19:06'
labels:
  - task-147
  - storage
  - migration
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: partial storage migration leaves ordinary task and planning changes requiring PR synchronization. Progress after proven low-impact cutover through config, ranking, goals, all task lifecycles and every GUI/TUI/CLI/dispatch/refine consumer. Evidence: user asks gradual migration through everything with same-or-intentional-change verification; TASK-147 contract identifies existing consumers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each migrated kind passes before/after content and command parity with unknown fields preserved and originals recoverable.
- [ ] #2 All native task-domain reads/writes use shared central authority across linked worktrees without routine Git changes.
<!-- AC:END -->
