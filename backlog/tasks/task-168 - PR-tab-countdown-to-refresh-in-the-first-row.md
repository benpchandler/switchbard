---
id: TASK-168
title: 'PR tab: countdown to refresh in the first row'
status: In Progress
assignee: []
created_date: '2026-09-07 13:02'
updated_date: '2026-09-07 13:02'
labels:
  - tui
  - prs
  - refresh
  - ball:agent
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: PR-tab users cannot tell when the next refresh will occur from the static 60s header and a separate refreshing note. Medium priority: unclear refresh timing, without blocked PR operations.

Evidence: owner request 2026-09-07. At origin/main be9428d, PR observation in crates/switchbard-tui/src/pr_view.rs appends refreshing as a separate note; PullRequests::tick in pull_requests.rs schedules from request start. Replace the fixed header interval with the remaining time and restart it from completion.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The first-row refresh interval counts down to 0 and shows refreshing while the request is pending.
- [ ] #2 The countdown restarts from the configured interval when refresh completes, including failure, and manual refresh continues to work.
- [ ] #3 The separate refreshing note is removed; errors and stale-data warnings remain visible.
- [ ] #4 Deterministic lifecycle and terminal-render tests verify ticking without input, completion, retry, and narrow/wide layouts.
<!-- AC:END -->
