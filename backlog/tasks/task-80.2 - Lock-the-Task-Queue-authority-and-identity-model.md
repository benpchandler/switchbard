---
id: TASK-80.2
title: Lock the Task Queue authority and identity model
status: To Do
assignee: []
created_date: '2026-08-31 21:45'
labels:
  - architecture
  - github
  - task-queue
dependencies: []
priority: high
parent_task_id: TASK-80
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define stable identities and field authority for Switchbard-native tasks, GitHub issues, pull requests, commits, checks, releases, deployments, and their relationships. Specify reconciliation, freshness, conflict, deletion, repository-transfer, and partial-failure behavior before persistence or UI implementation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The model distinguishes a local task from a linked or GitHub-backed delivery record without conflating their identities.
- [ ] #2 Every mutable field has exactly one authority and every derived field identifies its provenance and observation time.
- [ ] #3 Invalid, stale, transferred, deleted, inaccessible, and partially observed GitHub states have explicit behavior.
- [ ] #4 Existing repo-local task storage remains valid and migration-free.
<!-- AC:END -->
