---
id: TASK-11
title: 'Dispatch worker: flag task → worktree → headless claude -p → PR'
status: To Do
assignee: []
created_date: '2026-08-05 02:30'
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
