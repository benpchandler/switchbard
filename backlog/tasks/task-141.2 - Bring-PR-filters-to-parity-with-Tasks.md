---
id: TASK-141.2
title: Bring PR filters to parity with Tasks
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
Impact: PR users can only pick status, ID and title filters and cannot narrow by linked task, checks, review or merge observations.

Evidence: Current installed TUI source at /Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui/src/app/pickers.rs::open_column_chooser restricts PR columns to Status/Id/Title; pull_requests.rs::refilter exposes only status and id fields.

Owner requested these as tracked tasks before live claiming and implementation. This is a scoped child of TASK-141, not a replacement for its broader PR actions/notifications objective.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The shared filter picker exposes every applicable PR column, including linked tasks, checks, review and merge observations.
- [ ] #2 Text and categorical filters compose, clear, and persist in PR settings without modifying the task filter or GitHub data.
- [ ] #3 Real-key E2E journeys prove positive/negative/empty results, unknown/historical observations and page/restart isolation.
<!-- AC:END -->
