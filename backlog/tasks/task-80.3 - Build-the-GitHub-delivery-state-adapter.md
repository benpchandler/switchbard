---
id: TASK-80.3
title: Build the GitHub delivery-state adapter
status: To Do
assignee: []
created_date: '2026-08-31 21:45'
updated_date: '2026-08-31 21:45'
labels:
  - backend
  - github
  - task-queue
dependencies:
  - TASK-80.2
priority: high
parent_task_id: TASK-80
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a read-oriented GitHub adapter that resolves issue, pull request, required-check, merge-queue, commit, release, and deployment state for configured repositories. Reuse the authenticated gh boundary and preserve Unknown separately from negative answers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The adapter returns typed, source-attributed delivery observations without mutating GitHub.
- [ ] #2 Authentication, scope, rate-limit, network, missing-resource, and unsupported-repository failures remain distinguishable.
- [ ] #3 Caching and refresh behavior are bounded and expose observation freshness.
- [ ] #4 Synthetic tests cover complete, partial, stale, transferred, deleted, and inaccessible delivery chains.
<!-- AC:END -->
