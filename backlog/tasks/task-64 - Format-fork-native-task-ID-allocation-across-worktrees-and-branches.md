---
id: TASK-64
title: 'Format fork: native task ID allocation across worktrees and branches'
status: Done
assignee:
  - '@claude'
created_date: '2026-08-28 18:40'
updated_date: '2026-08-28 19:34'
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
- [x] #1 next_task_id considers task files in all worktrees of the repo
- [x] #2 Policy for branch-only task ids (scan or explicitly out of scope) is decided and documented in the module doc
- [x] #3 Concurrent-create behavior is defined and tested: create fails cleanly or retries on collision, never overwrites an existing file
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. backlog/allocate.rs: next_task_id(repo_root) = 1 + max task id found in (a) the four backlog dirs of every worktree of the repo (enumerate_worktrees; plain-dir fallback scans repo_root alone) and (b) task filenames on local branches with commit activity inside ACTIVE_BRANCH_DAYS=30 (git for-each-ref + ls-tree via git_cmd, branch count capped).
2. Id claims: a reservation file named by id alone, create_new in the repo git common dir (shared across worktrees; plain-dir fallback under backlog/.id-reservations). Stale reservations (>60s, or unparseable content) are stolen. Reservation removed on drop.
3. create_task_allocating_id: allocate -> reserve -> pre-check dir for the id -> write_new_task_file (create_new backstop) -> release; on any conflict, id+1 and retry, bounded attempts.
4. Policy documented in module doc: local worktrees + local active branches only; remote branches deliberately out of scope (single-machine fleet; cross-machine id collisions resolve at PR review).
5. Tests: filename/branch parsing pure fns; plain-dir allocation; git fixture with a task committed on an unmerged branch; a second worktree with an uncommitted task file; reservation mutual exclusion + staleness; two-thread concurrent create yielding distinct ids.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation notes:
- next_task_id = 1 + max over (a) the four backlog dirs in every worktree (uncommitted files count) and (b) task filenames on local branches with tip activity inside ACTIVE_BRANCH_DAYS=30, via git for-each-ref + ls-tree through git_cmd, capped at 100 branches.
- Branch policy documented in the module doc: local branches scanned (matching the CLI check_active_branches default it replaces); REMOTE branches deliberately out of scope - single-machine fleet, cross-machine collisions surface at PR review, and an allocator must not block on the network.
- Concurrency primitive: an id-named reservation file created with create_new in the repo git common dir, so a claim in one worktree excludes claimants in every other worktree; non-git projects fall back to backlog/.id-reservations. Reservations are released on drop and stolen after 60s (or when unreadable - refusing would brick the id forever).
- create_task_allocating_id: reserve -> dir pre-check -> write_new_task_file (create_new backstop), bumping the candidate id on any conflict, bounded at 20 attempts. Candidate ids are monotonic within one call, so a released reservation is never re-minted mid-call.
- Proved by a 4-thread concurrent create test (distinct ids, nothing overwritten) and real-git fixtures for the branch and sibling-worktree cases.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Landed backlog::allocate: native task ID allocation replacing the CLI's check_active_branches. next_task_id scans every worktree's four backlog dirs plus local branches active within 30 days (remote branches documented out of scope). create_task_allocating_id claims ids via create_new reservation files in the shared git common dir (cross-worktree mutual exclusion, stale-steal at 60s, drop-release), retries bounded, never overwrites. 10 new tests including real-git branch/worktree fixtures and a concurrent-create race. mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
