---
id: TASK-178
title: Consolidate TUI footer menus into shared pickers
status: Done
assignee: []
created_date: '2026-09-08 11:58'
updated_date: '2026-09-08 12:15'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Installed shared pickers for Task/rank, Views/save/global, column/settings actions and paint-rule actions. Preserved keyboard chords and PR features, shortened footers, added clipped-menu navigation and stale-task cancellation. Isolated tui-install passed fmt, clippy, 112 tests and rustdoc. Nine new behavioral menu tests and six fixture renders; independent review clear. Evidence docs/verification/tui-menu-pickers.md. Branch feat/tui-menu-pickers; no push or primary merge.
<!-- SECTION:FINAL_SUMMARY:END -->
