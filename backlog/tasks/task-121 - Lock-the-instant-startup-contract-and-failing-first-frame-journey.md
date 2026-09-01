---
id: TASK-121
title: Lock the instant-startup contract and failing first-frame journey
status: To Do
assignee: []
created_date: '2026-09-01 17:44'
labels:
  - cold-start
  - design
  - testing
  - performance
dependencies: []
priority: high
project: Instant Cold Start
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: People opening Switchbard on a real multi-worktree machine currently see empty or misleading surfaces and can wait through worker staggering and cold probes before useful information appears.

Evidence: Read-only startup trace on 2026-09-01 found that main.rs loads config and synchronously expands worktrees before opening the window, app.rs initializes every derived display store empty except agent context, and workers.rs delays the first Backlog and Landing refreshes to roughly 8 and 14 seconds.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Record the owner-approved stale-while-revalidate outcome and authority boundary in the product trajectory and a focused decision record.
- [ ] #2 Inventory every field rendered on Servers, Workspace, Agents, Tasks, and Dispatch, classifying each as display-only cached data or live authority required for an action.
- [ ] #3 Add a deterministic failing journey that blocks live probes and proves whether representative last-known content is present on the first rendered frame.
- [ ] #4 Cover valid cache, no cache, partial cache, stale cache, refresh success, refresh failure, corrupt or incompatible cache, removed repos, and newly configured repos in the state and stress matrix.
- [ ] #5 Measure current real-scale serialized data and startup costs, then ratify explicit cache byte limits and first-frame performance budgets.
<!-- AC:END -->
