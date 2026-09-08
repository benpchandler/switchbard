---
id: TASK-170
title: Add task creation to the TUI
status: Done
assignee: []
created_date: '2026-09-08 11:18'
updated_date: '2026-09-08 11:35'
labels:
  - tui
  - enhancement
  - ball:me
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: TUI users cannot capture an ordinary task without leaving the interface. Evidence: inspected switchbard-tui browse actions and report commands on 2026-09-08; only bug/idea filing exists. Implement a discoverable title-entry flow through the shared core writer, with cancellation and recoverable failure.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A discoverable configurable shortcut creates a task in the current repo and shows its result
- [x] #2 Blank input, cancellation, failed writes, retry, filters and narrow layouts are covered by real-key E2E tests
- [x] #3 TUI formatting, clippy and E2E gates pass and sbt is installed
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented configurable n/new_task title capture through create_backlog_task. Ten real-key E2E tests pass, covering blank/cancel/retry, hidden filters/settings, duplicates/custom prefix, bounded Unicode/resize, external additions, and held Enter. Independent review found the held-Enter issue, now fixed and confirmed resolved. State matrix and evidence: docs/verification/tui-add-task.md. Final install gate in progress.

Reopened after user reported missing PR tab. Cargo install provenance confirmed prior installation had PR-enabled feat/tui-pr-merge, while initial capture branch lacked PR features. Correcting by integrating onto 1d14a58, preserving existing n notification shortcut and using a for task capture; combined page/capture E2E added.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Corrected the initial installation regression by combining task capture with the previously installed PR-enabled revision 1d14a58. a adds a task; existing n notification dismissal and Tab PR navigation are preserved. Isolated mise run tui-install passes fmt, clippy, 102 tests and rustdoc (17 existing opt-in tests skipped), and installed binary provenance is verified. Independent review clear. Branch fix/tui-add-task-pr; no push or merge to primary.
<!-- SECTION:FINAL_SUMMARY:END -->
