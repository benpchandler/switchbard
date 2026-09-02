---
id: TASK-116
title: Extend GitHub delivery observations for repository pull-request operations
status: To Do
assignee: []
created_date: '2026-09-01 17:12'
labels:
  - github
  - ops
  - backend
dependencies:
  - TASK-80
  - TASK-115
priority: high
project: GitHub Operations
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Pull-request actions cannot be preflighted or verified without current repository, viewer, head revision, check, review, and mergeability facts.

Evidence: TASK-80 establishes project-scoped read-only delivery awareness. The approved GitHub Operations scope requires repository-scoped pull-request observations without allowing GitHub observations to mutate local task bytes or completion state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tracked repository remotes resolve to typed repository identities without heuristic string matching
- [ ] #2 The adapter observes pull-request summary and detail facts including head OID, checks, review decision, mergeability, merge queue or auto-merge state, viewer identity, and observed-at provenance
- [ ] #3 Permission and capability observations use explicit Allowed, Denied, or Unknown states and stale data remains visibly stale
- [ ] #4 Observation pages and caches are bounded, deterministic, and covered by fixture and live-probe verification
- [ ] #5 The observation path remains read-only and cannot change local task status, project membership, priority, dependencies, or task bytes
<!-- AC:END -->
