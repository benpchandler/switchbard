---
id: TASK-157
title: 'sbt: mark selected task Done'
status: Done
assignee: []
created_date: '2026-09-04 13:21'
updated_date: '2026-09-04 13:27'
labels:
  - tui
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a keyboard action in sbt that marks the selected task Done through Switchbard's native backlog mutation boundary.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Pressing the documented Done key on an open task changes only its status to Done through the native write layer
- [x] #2 The task remains selected when it is still visible, and no-op or write failures are reported without stale UI state
- [x] #3 The behavior is covered by a real TUI harness test and the TUI test suite is green
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added sbt's documented d action. It marks the selected task Done through edit_backlog_task and the native validation/write boundary, reloads the projection, preserves selection when visible, and makes repeat presses an explicit no-op. A real terminal harness test proves the transition and byte-identical repeat.
<!-- SECTION:FINAL_SUMMARY:END -->
