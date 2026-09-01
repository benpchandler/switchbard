---
id: TASK-98
title: 'IA V2: Dispatches view + Command fleet console'
status: To Do
assignee: []
created_date: '2026-09-01 02:24'
updated_date: '2026-09-01 02:57'
labels:
  - ia
  - gui
  - dispatch-failed
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dispatch's two axes (trajectory: IA V2). Under Tasks: the built-in Dispatches view - per-run status, live activity line, elapsed, watch/kill/retry/log. Command as its own place: the agent-scoped fleet console - agents, missions, worktree leases, SITREP age, support requests (NEEDS_DECISION etc.) with respond affordance and blast-radius note. Footer lamp deep-links.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dispatches view lists runs with kill/retry/log wired to existing dispatch_kill/reaper paths
- [ ] #2 Command place renders the fleet with support requests surfaced and respondable
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Dispatch failed: claude exited with 1
<!-- SECTION:NOTES:END -->
