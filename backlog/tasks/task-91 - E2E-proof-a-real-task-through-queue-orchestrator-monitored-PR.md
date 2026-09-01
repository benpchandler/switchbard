---
id: TASK-91
title: 'E2E proof: a real task through queue -> orchestrator -> monitored PR'
status: To Do
assignee: []
created_date: '2026-09-01 01:56'
labels:
  - task-queue
  - dogfood
  - verification
dependencies:
  - TASK-89
  - TASK-90
priority: high
project: Task Queue
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dogfood the whole loop on this repo: send a small real task to the queue (rank order visible), orchestrator claims and works it in an isolated worktree, live progress renders in the GUI, run ends in an open PR and a released claim, and the task's notes carry the PR link. This is the goal's definition of done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real task flows queue -> claim -> agent run -> PR with no hand-edits
- [ ] #2 Progress was observable live during the run (events sidecar populated, GUI phase moved)
- [ ] #3 Reordering the queue before pickup changes which task the orchestrator takes next
<!-- AC:END -->
