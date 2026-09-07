---
id: TASK-160
title: 'sbt: show read-only repository pull requests with task links'
status: In Progress
assignee: []
created_date: '2026-09-06 23:57'
updated_date: '2026-09-07 00:09'
labels:
  - tui
  - github
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: the owner cannot inspect delivery from the new PR page; it is currently only a shell. Evidence: user confirmed navigation worked and authorized the next read-only PR-list slice. Scope: current repo open PRs, explicit links from task references, checks/review/merge facts, bounded asynchronous refresh and honest unknown states. No GitHub writes or task status changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PR page asynchronously loads current repo open PRs with identity, linked tasks and delivery observations.
- [x] #2 Loading, empty, errors, partial and stale states remain explicit; task context and bytes are preserved.
- [ ] #3 Rendered TUI journeys and crate checks pass; checked binary installed for owner review.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation and evidence tracked in docs/implementation/tui-pr-list.md. Real budget PR #958 read successfully; isolated local canonical task link and stale refresh proven without changing task bytes. Core 10 tests, core Clippy and TUI gate passed. Final install pending.
<!-- SECTION:NOTES:END -->
