---
id: TASK-178
title: Consolidate TUI footer menus into shared pickers
status: In Progress
assignee: []
created_date: '2026-09-08 11:58'
updated_date: '2026-09-08 13:31'
labels:
  - tui
  - ux
  - ball:me
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: TUI users cannot see or navigate choices that are packed into clipped footer sentences. Evidence: owner screenshot of task chord footer on 2026-09-08; inventory finds task and view open/save/global chord menus plus action hints in existing pickers. Scope: expose existing menu actions as shared picker choices, preserve keyboard shortcuts and PR features, consolidate duplicate menu handling.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Task and view chord menus use shared pickers with all existing actions reachable
- [x] #2 Other footer menu actions are inventoried and consolidated into picker rows; editing/navigation hints remain concise
- [x] #3 Real-key E2E verifies keyboard navigation, cancellation, empty and narrow states, shortcuts and PR parity
- [x] #4 TUI gate passes and corrected installed build preserves PR-enabled revision
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner visual annotation requests replacing Mark Done with s Status and a subsequent status picker. Implementing before closing the review.

Addressed all five owner annotations: configured Status picker, project linking, arrow/Vim parent navigation, and separate Top list membership from view display. Fixed full-suite filter/paint navigation regressions before installation.

PR140 Linux CI exposed a native task writer defect: a long Unicode title produced a 258-byte filename. Added executable writer regression, reproduced before fix, cap slug at180 UTF-8 bytes without truncating persisted title;34 writer tests pass. Cancelled malfunctioning pipeline after it accidentally committed target-ci build artifacts; remote stayed at clean05ec96b6, rejected commit preserved by guarded recovery, local synchronized to remote. Completing direct preflight and GitHub CI before merge.

Final writer fix budgets the complete basename including configured prefix/id and extension, caps slug bytes on UTF-8 boundaries, and covers original Unicode, mid-character truncation, and100-byte prefix cases. All three regression scenarios pass; independent review clear.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Installed consolidated task/view/column/settings/paint pickers, including tn New, ts Status, tp Project, tr Top list, vp top-list display. Left/Right and Vim navigation supported with documented legacy filter typeahead precedence. Isolated tui-install passed fmt, clippy, 117 tests and rustdoc; nine fixture renders refreshed; all five Visual Review annotations resolved. Evidence docs/verification/tui-menu-pickers.md. Branch feat/tui-menu-pickers; no push or primary merge.
<!-- SECTION:FINAL_SUMMARY:END -->
