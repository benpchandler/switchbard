---
id: TASK-7
title: Document/standardize HiveApp command-method naming convention
status: Done
assignee:
  - '@removal-fixes'
created_date: '2026-06-21 01:49'
updated_date: '2026-06-21 02:11'
labels:
  - mutation-arch
  - audit
  - consistency
dependencies: []
priority: low
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit finding #6b. Mutation method names on HiveApp are freehand: open_/execute_/cancel_ for create+remove modals, spawn_* for start/stop/kill, add_/remove_/move_ for repos, with the '_confirm' suffix only on remove-worktree. All threaded mutators (execute_*, spawn_*) do the same thing. Harmless but undocumented. Either add a one-line convention note in app.rs's header doc-comment, or rename for consistency if touching them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A short naming convention for mutation methods is documented in app.rs (or methods are renamed consistently)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added a markdown table to the module-level doc-comment in app.rs documenting the open_/cancel_/execute_, add_/remove_/move_, and spawn_ prefixes. No renames — all existing names already follow the convention.

Orchestrator follow-up: corrected the spawn_ row's examples in the app.rs naming-convention doc — they cited non-existent methods (spawn_service / spawn_kill_pgid). Now cites the real methods: spawn_start, spawn_stop_run, spawn_kill.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a ## Mutation-method naming convention table to the top-of-file doc-comment in crates/switchbard-gui/src/app.rs documenting the open_/cancel_/execute_ modal lifecycle triad, add_/remove_/move_ repo CRUD, and spawn_ fire-and-forget pattern. No renames; all existing names already conform. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
