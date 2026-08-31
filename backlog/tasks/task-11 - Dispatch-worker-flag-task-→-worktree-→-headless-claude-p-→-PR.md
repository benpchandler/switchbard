---
id: TASK-11
title: 'Dispatch worker: flag task → worktree → headless claude -p → PR'
status: Done
assignee: []
created_date: '2026-08-05 02:30'
updated_date: '2026-08-31 11:10'
labels:
  - hub
  - slice-2
dependencies: []
priority: medium
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Slice 2 (trajectory doc). Fifth worker thread following workers.rs pattern: task flagged for dispatch in UI → create worktree via existing lifecycle → spawn_in_session runs headless claude -p with the task file as prompt → on success gh pr create → append PR link to task notes via backlog CLI. Concurrency-capped; logs to $TMPDIR/switchbard-logs/.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed as already shipped. The dispatch worker described here exists on main: workers.rs spawn_dispatch (fifth worker, 90s cadence) drives switchbard_core::dispatch (flag -> worktree -> headless claude -p -> gh pr create -> notes append through the native write layer since TASK-65). Landed across the dispatch arc (TASK-43/44/46/53); this card predates them and was never updated.
<!-- SECTION:FINAL_SUMMARY:END -->
