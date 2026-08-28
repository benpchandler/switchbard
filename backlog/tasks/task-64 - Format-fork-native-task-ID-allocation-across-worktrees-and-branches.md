---
id: TASK-64
title: 'Format fork: native task ID allocation across worktrees and branches'
status: To Do
assignee: []
created_date: '2026-08-28 18:40'
labels:
  - format-fork
dependencies:
  - TASK-62
priority: medium
ordinal: 63000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the backlog CLI check_active_branches collision scan with our own allocator. Next id = max over task ids found in every worktree of the repo (enumerate_worktrees already exists) plus, per decided policy, task filenames on active branches (git for-each-ref + ls-tree). All real work in this fleet happens in worktrees, so worktrees-only may suffice - decide and document the policy in the module doc rather than silently narrowing. Must be race-conscious: two dispatchers creating tasks simultaneously must never overwrite each other.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 next_task_id considers task files in all worktrees of the repo
- [ ] #2 Policy for branch-only task ids (scan or explicitly out of scope) is decided and documented in the module doc
- [ ] #3 Concurrent-create behavior is defined and tested: create fails cleanly or retries on collision, never overwrites an existing file
<!-- AC:END -->
