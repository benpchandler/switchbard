---
id: TASK-80.4
title: Render mixed local and GitHub-backed work in the Task Queue
status: To Do
assignee: []
created_date: '2026-08-31 21:45'
updated_date: '2026-08-31 21:45'
labels:
  - gui
  - github
  - task-queue
  - design
dependencies:
  - TASK-80.2
  - TASK-80.3
priority: high
parent_task_id: TASK-80
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Evolve the current task surface into the Task Queue. Show local planning and orchestration fields alongside clearly sourced GitHub delivery state without implying that a merged PR completes the user outcome.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Users can distinguish local, linked, and GitHub-backed items at a glance.
- [ ] #2 Issue, PR, checks, merge, release, deployment, freshness, and Unknown states use progressive disclosure.
- [ ] #3 Queue ordering can combine Switchbard priority and dependencies with GitHub delivery attention signals.
- [ ] #4 Empty, loading, partial, stale, error, mixed-source, narrow-window, long-title, and high-volume states are designed and verified.
- [ ] #5 Render-path performance remains within the existing Switchbard perf contract.
<!-- AC:END -->
