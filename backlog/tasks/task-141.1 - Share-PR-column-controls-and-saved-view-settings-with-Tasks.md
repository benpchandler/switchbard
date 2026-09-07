---
id: TASK-141.1
title: Share PR column controls and saved view settings with Tasks
status: To Do
assignee: []
created_date: '2026-09-07 01:32'
labels:
  - tui
  - github
  - parity
dependencies: []
priority: medium
parent_task_id: '141'
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: PR users cannot access numbered column actions, choose visible fields, or retain page-specific presentation settings. This blocks the same workflow already available on Tasks.

Evidence: Current installed TUI source at /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui/src/page.rs::Page::allows excludes Columns/View; src/app/mod.rs::handle_browse_key limits numbered column actions to Tasks; pr_view.rs hard-codes columns.

Owner requested these as tracked tasks before live claiming and implementation. This is a scoped child of TASK-141, not a replacement for its broader PR actions/notifications objective.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PR header digits open the shared column-action picker; show/hide/reorder operates on PR columns.
- [ ] #2 PR settings and saved view slots persist independently of Tasks across page switches and restart; existing task views remain compatible.
- [ ] #3 Help and footer accurately advertise available PR controls; real-key E2E journeys cover narrow/empty layouts and page isolation.
<!-- AC:END -->
