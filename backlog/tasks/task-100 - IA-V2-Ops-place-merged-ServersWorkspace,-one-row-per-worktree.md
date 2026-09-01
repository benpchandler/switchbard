---
id: TASK-100
title: 'IA V2: Ops place - merged Servers/Workspace, one row per worktree'
status: To Do
assignee: []
created_date: '2026-09-01 02:24'
labels:
  - ia
  - gui
dependencies: []
priority: medium
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rename Repos->Ops and merge (trajectory: IA V2). One row per worktree: services start/stop, listeners with open-in-browser and logs, external squatters with kill, git state, agent sessions attributed per worktree, removal behind the removal_safety verdict. Retains every existing Servers/Workspace capability.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All current Servers + Workspace actions reachable from the merged rows
- [ ] #2 Removal still gated on RemovalVerdict (and RemovalAuthorization once TASK-81 lands)
<!-- AC:END -->
