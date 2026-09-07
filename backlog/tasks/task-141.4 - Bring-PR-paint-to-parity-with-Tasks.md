---
id: TASK-141.4
title: Bring PR paint to parity with Tasks
status: To Do
assignee: []
created_date: '2026-09-07 01:32'
labels:
  - tui
  - github
  - parity
dependencies:
  - '141.1'
  - '141.2'
priority: medium
parent_task_id: '141'
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: PR users cannot visually distinguish rows or columns with the same paint workflow used for Tasks.

Evidence: Current installed TUI source at /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui/src/page.rs::Page::allows excludes Paint; pr_view.rs::draw_row applies only Selected/Text surfaces.

Owner requested these as tracked tasks before live claiming and implementation. This is a scoped child of TASK-141, not a replacement for its broader PR actions/notifications objective.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The shared p picker and column menu support categorical values, selected/filtered rows, whole columns, palette auto-paint, rule ordering and clearing for PRs.
- [ ] #2 Paint stays legible under selection, composes with filter/sort, and persists in PR settings without changing Tasks paint.
- [ ] #3 Real-key E2E rendered-cell assertions cover paint application/removal, precedence, page/restart isolation and unknown observations.
<!-- AC:END -->
