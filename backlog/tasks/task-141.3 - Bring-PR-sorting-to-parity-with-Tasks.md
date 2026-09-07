---
id: TASK-141.3
title: Bring PR sorting to parity with Tasks
status: In Review
assignee: []
created_date: '2026-09-07 01:32'
updated_date: '2026-09-07 01:51'
labels:
  - tui
  - github
  - parity
  - ball:agent
dependencies:
  - '141.1'
priority: medium
parent_task_id: '141'
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: PR users cannot order their list by the fields they inspect; fixed attention ordering prevents their preferred triage flow.

Evidence: Current installed TUI source at /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui/src/page.rs::Page::allows excludes SortColumn; pull_requests.rs::accept fixes attention rank and descending PR number.

Owner requested these as tracked tasks before live claiming and implementation. This is a scoped child of TASK-141, not a replacement for its broader PR actions/notifications objective.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The shared s picker and numbered column menu offer applicable ascending/descending and semantic PR sorts.
- [x] #2 Sorting composes with filtering and refresh, preserves selected PR identity when still visible, and persists independently of Tasks.
- [x] #3 Real-key E2E journeys prove numeric PR ordering, titles, lifecycle/check semantics, empty results and page/restart isolation.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in feat/tui-pr-pane (source ea277a1). Full TUI fmt/clippy/tests/install passed. All 20 page/PR/control E2E journeys passed with actual GitHub reads; title/check sorts and review/merge facets also passed on budget PR observations. Evidence: docs/tui-pr-controls-ledger.md and docs/tui-pr-controls-evidence.md. Independent source review has no remaining verified blocker. Delivery pipeline/PR/CI and human visual review remain pending; not merged.
<!-- SECTION:NOTES:END -->
