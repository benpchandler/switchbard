---
id: TASK-80.2
title: Lock the Task Queue authority and identity model
status: In Progress
assignee: []
created_date: '2026-08-31 21:45'
updated_date: '2026-09-01 01:44'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner clarification (2026-09-01): the Task Queue is the surface where the user tees up tasks for dispatch and sees what dispatch is working on - not a backlog-browsing lens. Design input for the authority model: 'Switchbard owns priority' now has a concrete authority - the stack rank in backlog/ranking.yml (TASK-82..85, shipped) and its computed flatten (expedite lane, project rank, sibling walk). The queue's tee-up order should consume that computed order rather than inventing a second priority representation; cross-repo composition goes through the hub ordering.yml overlay (trajectory: unified task hub slice 1), which remains deferred. Dispatch visibility side ('what's being worked on') should read the same run/claim state the Dispatches view already cross-checks (dispatch_inspect / looks_orphaned), not a new store.

Authority-model draft written for owner review: docs/task-queue-authority-model.md. Three record kinds with unmergeable identities (native task = (repo root, task id); GitHub records keyed by node_id with coordinates as provenance-stamped derived fields; links as canonical URLs in references:). One authority per field (Switchbard: rank/status/claims/acceptance/links; GitHub: delivery state, read-only observations with observed_at provenance). Probe-and-derive reconciliation per landing.rs's PushState/PrState precedent; disposable observation cache outside backlog/; per-field Unknown(reason) degradation with the removal_safety spine (an unanswered question never counts as answered; absence never implies Done/merged). ACs stay unchecked until the owner approves the model - locking is an owner decision.
<!-- SECTION:NOTES:END -->
