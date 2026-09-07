---
id: TASK-141.1
title: Share PR column controls and saved view settings with Tasks
status: In Review
assignee: []
created_date: '2026-09-07 01:32'
updated_date: '2026-09-07 02:00'
labels:
  - tui
  - github
  - parity
  - ball:agent
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
- [x] #1 PR header digits open the shared column-action picker; show/hide/reorder operates on PR columns.
- [x] #2 PR settings and saved view slots persist independently of Tasks across page switches and restart; existing task views remain compatible.
- [x] #3 Help and footer accurately advertise available PR controls; real-key E2E journeys cover narrow/empty layouts and page isolation.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in feat/tui-pr-pane (source ea277a1). Full TUI fmt/clippy/tests/install passed. All 20 page/PR/control E2E journeys passed with actual GitHub reads; title/check sorts and review/merge facets also passed on budget PR observations. Evidence: docs/tui-pr-controls-ledger.md and docs/tui-pr-controls-evidence.md. Independent source review has no remaining verified blocker. Delivery pipeline/PR/CI and human visual review remain pending; not merged.

Delivery run 01M1X6S8BVE6H8F1BVR3A5H2CK: rebase onto current main completed and code review passed without findings; pipeline targeted tests passed. Pipeline is paused at ask-user finding missing-visual-evidence (native terminal screenshot/GIF). Installed build and all 20 originating live/page/control tests are proved; CUA refused WezTerm capture. Owner decision requested before proceeding. No PR or CI result yet. Live claim retained for resumption.

Owner approved the missing-visual-evidence gate and instructed continue. Run 01M1X6S8BVE6H8F1BVR3A5H2CK resumed: test gate approved using the installed build and recorded real E2E evidence; documentation/lint/push/PR/CI pending. This is not a claim of a captured screenshot or human appearance approval.
<!-- SECTION:NOTES:END -->
