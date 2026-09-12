---
id: TASK-201
title: 'TASK-147: central document store and initiative/project strangler cutover'
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
Impact: Switchbard users need routine definition updates shared across worktrees without PRs. Implement flexible central SQLite authority per repository/kind and migrate lower-impact initiative/project definitions first. Evidence: TASK-147 owner authorization and second-opinion/schema-flexibility contract in codex/task-147-storage-contract; hierarchy.rs currently reads/writes definition Markdown directly.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One database stores full custom content with transactional updates and per-kind authority; migrated kinds never fall back to files on error.
- [ ] #2 Temporary-store parity, unchanged original files, concurrent writes and linked-worktree visibility pass before real cutover.
<!-- AC:END -->
