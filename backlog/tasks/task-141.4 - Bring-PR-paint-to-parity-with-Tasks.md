---
id: TASK-141.4
title: Bring PR paint to parity with Tasks
status: Done
assignee: []
created_date: '2026-09-07 01:32'
updated_date: '2026-09-07 13:27'
labels:
  - tui
  - github
  - parity
dependencies:
  - '141.1'
  - '141.2'
priority: medium
parent_task_id: '141'
references:
  - https://github.com/benpchandler/switchbard/pull/136
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: PR users cannot visually distinguish rows or columns with the same paint workflow used for Tasks.

Evidence: Current installed TUI source at /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui/src/page.rs::Page::allows excludes Paint; pr_view.rs::draw_row applies only Selected/Text surfaces.

Owner requested these as tracked tasks before live claiming and implementation. This is a scoped child of TASK-141, not a replacement for its broader PR actions/notifications objective.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The shared p picker and column menu support categorical values, selected/filtered rows, whole columns, palette auto-paint, rule ordering and clearing for PRs.
- [x] #2 Paint stays legible under selection, composes with filter/sort, and persists in PR settings without changing Tasks paint.
- [x] #3 Real-key E2E rendered-cell assertions cover paint application/removal, precedence, page/restart isolation and unknown observations.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in feat/tui-pr-pane (source ea277a1). Full TUI fmt/clippy/tests/install passed. All 20 page/PR/control E2E journeys passed with actual GitHub reads; title/check sorts and review/merge facets also passed on budget PR observations. Evidence: docs/tui-pr-controls-ledger.md and docs/tui-pr-controls-evidence.md. Independent source review has no remaining verified blocker. Delivery pipeline/PR/CI and human visual review remain pending; not merged.

Verified PR #136 merged with all eight checks passing; scoped implementation criteria complete. Parent TASK-141 remains open.
<!-- SECTION:NOTES:END -->
