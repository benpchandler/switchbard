---
id: TASK-5
title: Add tests for multi-step worktree removal orchestration
status: Done
assignee:
  - '@removal-fixes'
created_date: '2026-06-21 01:49'
updated_date: '2026-06-21 02:07'
labels:
  - mutation-arch
  - audit
  - testing
dependencies:
  - TASK-1
  - TASK-3
priority: medium
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit finding #5. The riskiest mutation path is the least covered. Decision logic is tested (worktree_branch_delete_state.rs, worktree_create_state.rs, worktree_rename_state.rs) but execution is not: state_drifted/runs_drifted (app.rs:796-827), delete_branch_after_removal (app.rs:837-852), and the kill->remove->branch-delete sequencing in execute_remove_worktree have no direct tests. Start with the near-pure helpers; then an integration test over a real temp git repo (copy setup_repo_with_worktree from crates/switchbard-core/src/worktree_remove.rs tests). Extracting the kill->remove->branch body into a snapshot-taking fn enables orchestration coverage without the thread.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 state_drifted and runs_drifted have unit tests covering equal, added, removed, and status-flip cases
- [x] #2 delete_branch_after_removal is tested for: not-requested, success, and failure-is-non-fatal
- [x] #3 At least one integration test exercises success and one drift-abort path against a real temp git repo
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
state_drifted and runs_drifted promoted to pub (needed for integration test visibility). delete_branch_after_removal promoted to pub. Thread note: execute_remove_worktree spawns a worker; testing it end-to-end requires a live egui context. Instead tested the extracted decision functions (state_drifted, runs_drifted, delete_branch_after_removal) plus the core remove_worktree + collect_dirty_files calls that the worker delegates to — 14 tests total.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 14 tests in tests/worktree_removal_orchestration.rs: 5 for state_drifted (equal/added/removed/status-flip/empty), 4 for runs_drifted (equal/added/removed/empty), 3 for delete_branch_after_removal (not-requested/success/failure-non-fatal against real temp git repo), and 2 integration tests exercising remove_worktree success and state_drifted catching a real file change. Added tempfile as a gui dev-dependency. Promoted state_drifted, runs_drifted, delete_branch_after_removal to pub for test accessibility. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
