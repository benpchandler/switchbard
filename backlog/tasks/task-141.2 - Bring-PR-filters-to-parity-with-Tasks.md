---
id: TASK-141.2
title: Bring PR filters to parity with Tasks
status: In Review
assignee: []
created_date: '2026-09-07 01:32'
updated_date: '2026-09-07 02:00'
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
Impact: PR users can only pick status, ID and title filters and cannot narrow by linked task, checks, review or merge observations.

Evidence: Current installed TUI source at /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui/src/app/pickers.rs::open_column_chooser restricts PR columns to Status/Id/Title; pull_requests.rs::refilter exposes only status and id fields.

Owner requested these as tracked tasks before live claiming and implementation. This is a scoped child of TASK-141, not a replacement for its broader PR actions/notifications objective.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The shared filter picker exposes every applicable PR column, including linked tasks, checks, review and merge observations.
- [x] #2 Text and categorical filters compose, clear, and persist in PR settings without modifying the task filter or GitHub data.
- [x] #3 Real-key E2E journeys prove positive/negative/empty results, unknown/historical observations and page/restart isolation.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in feat/tui-pr-pane (source ea277a1). Full TUI fmt/clippy/tests/install passed. All 20 page/PR/control E2E journeys passed with actual GitHub reads; title/check sorts and review/merge facets also passed on budget PR observations. Evidence: docs/tui-pr-controls-ledger.md and docs/tui-pr-controls-evidence.md. Independent source review has no remaining verified blocker. Delivery pipeline/PR/CI and human visual review remain pending; not merged.

Delivery run 01M1X6S8BVE6H8F1BVR3A5H2CK: rebase onto current main completed and code review passed without findings; pipeline targeted tests passed. Pipeline is paused at ask-user finding missing-visual-evidence (native terminal screenshot/GIF). Installed build and all 20 originating live/page/control tests are proved; CUA refused WezTerm capture. Owner decision requested before proceeding. No PR or CI result yet. Live claim retained for resumption.

Owner approved the missing-visual-evidence gate and instructed continue. Run 01M1X6S8BVE6H8F1BVR3A5H2CK resumed: test gate approved using the installed build and recorded real E2E evidence; documentation/lint/push/PR/CI pending. This is not a claim of a captured screenshot or human appearance approval.
<!-- SECTION:NOTES:END -->
