---
id: TASK-141.1
title: Share PR column controls and saved view settings with Tasks
status: Done
assignee: []
created_date: '2026-09-07 01:32'
updated_date: '2026-09-07 13:27'
labels:
  - tui
  - github
  - parity
dependencies: []
priority: medium
parent_task_id: '141'
references:
  - https://github.com/benpchandler/switchbard/pull/136
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: PR users cannot access numbered column actions, choose visible fields, or retain page-specific presentation settings. This blocks the same workflow already available on Tasks.

Evidence: Current installed TUI source at /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui/src/page.rs::Page::allows excludes Columns/View; src/app/mod.rs::handle_browse_key limits numbered column actions to Tasks; pr_view.rs hard-codes columns.

Owner requested these as tracked tasks before live claiming and implementation. This is a scoped child of TASK-141, not a replacement for its broader PR actions/notifications objective.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PR header digits open the shared column-action picker; show/hide/reorder operates on PR columns.
- [x] #2 PR settings and saved view slots persist independently of Tasks across page switches and restart; existing task views remain compatible.
- [x] #3 Help and footer accurately advertise available PR controls; real-key E2E journeys cover narrow/empty layouts and page isolation.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in feat/tui-pr-pane (source ea277a1). Full TUI fmt/clippy/tests/install passed. All 20 page/PR/control E2E journeys passed with actual GitHub reads; title/check sorts and review/merge facets also passed on budget PR observations. Evidence: docs/tui-pr-controls-ledger.md and docs/tui-pr-controls-evidence.md. Independent source review has no remaining verified blocker. Delivery pipeline/PR/CI and human visual review remain pending; not merged.

Verified 2026-09-07: PR #136 merged at be9428d, all eight GitHub checks passed. This child implementation contract is complete; parent TASK-141 still tracks actions, cross-page alerts, and reporter confirmation.
<!-- SECTION:NOTES:END -->
