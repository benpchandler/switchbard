---
id: TASK-68
title: 'Format fork: diverge on named wins'
status: To Do
assignee: []
created_date: '2026-08-28 18:40'
labels:
  - format-fork
dependencies:
  - TASK-67
priority: low
ordinal: 67000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First format changes now that compatibility is no longer owed. Each is its own PR; split into subtasks when picked up. (1) Collapse the parent:/parent_task_id: dual key to one, migrating existing files. (2) Dispatch state as a first-class frontmatter field replacing the four-label convention (dispatch/dispatching/dispatched/dispatch-failed), updating the Dispatch view orphan cross-check. (3) Status validation moved into the write layer, validated against config.yml plus the standard-vocabulary offer, making the Invalid-status failure class structurally impossible.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 parent key collapsed to a single form with a one-shot migration across tracked repos
- [ ] #2 Dispatch pipeline state carried by a dedicated frontmatter field; label state machine retired
- [ ] #3 Write layer validates status against the repo config; the rejected-write class has a test proving it cannot recur
- [ ] #4 Each divergence recorded in the trajectory doc when it lands
<!-- AC:END -->
