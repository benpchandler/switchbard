---
id: TASK-141.3
title: Bring PR sorting to parity with Tasks
status: In Progress
assignee: []
created_date: '2026-09-07 01:32'
updated_date: '2026-09-07 01:37'
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
- [ ] #1 The shared s picker and numbered column menu offer applicable ascending/descending and semantic PR sorts.
- [ ] #2 Sorting composes with filtering and refresh, preserves selected PR identity when still visible, and persists independently of Tasks.
- [ ] #3 Real-key E2E journeys prove numeric PR ordering, titles, lifecycle/check semantics, empty results and page/restart isolation.
<!-- AC:END -->
