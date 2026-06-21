---
id: TASK-6
title: Harden validate_refish against leading-dash refnames
status: Done
assignee:
  - '@removal-fixes'
created_date: '2026-06-21 01:49'
updated_date: '2026-06-21 02:07'
labels:
  - mutation-arch
  - audit
  - validation
dependencies: []
priority: low
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit finding #6a. validate_refish (crates/switchbard-core/src/worktree_create.rs:62-67) only rejects empty/whitespace. A branch/base beginning with '-' could be misparsed as a git flag (e.g. 'git worktree add -b -x ...'). NOT an injection risk — these are Command args, never shell-interpreted — but a latent arg-parsing edge worth closing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 validate_refish rejects values beginning with '-'
- [x] #2 Tests cover empty, whitespace, leading-dash, and a valid refname
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
validate_refish is private; tests added as a #[cfg(test)] mod directly in worktree_create.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added leading-dash guard to validate_refish in crates/switchbard-core/src/worktree_create.rs: trims whitespace then rejects values starting with '-' with a clear error message. Added 5 unit tests in the same file covering empty, whitespace-only, leading-dash, leading-dash-with-whitespace, and valid refnames. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
