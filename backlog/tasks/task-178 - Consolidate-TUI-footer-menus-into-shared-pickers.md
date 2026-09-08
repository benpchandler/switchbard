---
id: TASK-178
title: Consolidate TUI footer menus into shared pickers
status: Done
assignee: []
created_date: '2026-09-08 11:58'
updated_date: '2026-09-08 18:06'
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

Closing bookkeeping only - this work was authored by a different session, not here.

Shipped in PR #140 (merged 2026-09-08 17:39Z, main f5c5f61c), which carried the shared-picker consolidation, app/task_project.rs and app/task_status.rs, the ~200 lines app/mod.rs sheds into pickers.rs, tests/menu_pickers.rs, and the nine verification screenshots under docs/verification/tui-menu-pickers/. All four acceptance criteria were already checked by the authoring session; the status was simply never moved off In Progress.

Note for the record: PR #142 merged the same branch a second time at 20:43Z. It was a no-op - `git diff ba639c1d 1703a7d6` is empty - so main carries one empty merge commit and no duplicated content. Left in place at the owner's direction; reverting it would add noise without removing any.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Delivered in PR #140 by a separate session, with all four ACs checked and verification screenshots committed. Closed here as bookkeeping only - the status had been left at In Progress after the work merged. No code was written for this task in this session.
<!-- SECTION:FINAL_SUMMARY:END -->
