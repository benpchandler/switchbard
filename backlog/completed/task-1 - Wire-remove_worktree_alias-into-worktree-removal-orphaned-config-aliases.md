---
id: TASK-1
title: Wire remove_worktree_alias into worktree removal (orphaned config aliases)
status: Done
assignee:
  - '@removal-fixes'
created_date: '2026-06-21 01:49'
updated_date: '2026-06-21 01:56'
labels:
  - mutation-arch
  - audit
  - correctness
dependencies: []
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit finding #1. remove_worktree_alias (crates/switchbard-gui/src/runtime/worktree_names.rs:116) is dead code — zero callers. execute_remove_worktree (crates/switchbard-gui/src/app.rs:416-547) removes the worktree from disk and the runtime worktrees mutex (app.rs:506-509) but never prunes config.worktrees, so alias entries accumulate pointing at deleted paths. Note: config is owned directly (not Arc<Mutex>) and execute_remove_worktree runs on a worker thread, so the prune must happen on the UI thread — mirror the create flow's outcomes-queue (create_worktree_outcomes + drain_create_worktree_outcomes) rather than mutating config from the worker.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 On successful worktree removal, the matching alias in config.worktrees is removed and config is persisted
- [x] #2 Alias prune happens on the UI thread (config is not behind a Mutex); worker-thread code does not touch config
- [x] #3 A test proves config.worktrees no longer contains the removed worktree's alias after removal
- [x] #4 remove_worktree_alias is no longer dead code (has a caller) or is removed if a better path is chosen
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented via outcomes-queue pattern mirroring create flow. Added RemovedWorktree struct and removal_error_message fn to worktree_actions.rs; added remove_worktree_outcomes: Arc<Mutex<Vec<RemovedWorktree>>> field to HiveApp; drain_remove_worktree_outcomes() runs on UI thread calling remove_worktree_alias + save_config; wired into render_ui next to drain_create_worktree_outcomes. Worker pushes to queue on Ok. Test: remove_worktree_alias_prunes_matching_entry_and_leaves_others in tests/worktree_names.rs.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Wired remove_worktree_alias into the worktree removal flow via a new remove_worktree_outcomes queue (Arc<Mutex<Vec<RemovedWorktree>>>) mirroring the existing create flow. On successful git worktree remove, the worker thread pushes a RemovedWorktree payload; drain_remove_worktree_outcomes() runs each frame on the UI thread, calls remove_worktree_alias, and persists config. remove_worktree_alias is no longer dead code. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
