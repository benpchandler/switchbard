---
id: TASK-115
title: Lock the GitHub Operations authority, placement, and command contract
status: To Do
assignee: []
created_date: '2026-09-01 17:12'
labels:
  - github
  - ops
  - architecture
dependencies: []
priority: high
project: GitHub Operations
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Switchbard users cannot safely operate pull requests if canonical ownership, authority, preconditions, and failure states remain implicit.

Evidence: The owner-reviewed GitHub Operations plan at /Users/bpc/.lavish/switchbard-github-ops-plan.html places the canonical surface at Ops > Pull requests. The current product trajectory defines Ops but does not yet define this surface or its mutation contract.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/product-trajectory.md records Ops > Pull requests as the canonical pull-request surface and permits Digest to mirror only objects already represented there
- [ ] #2 The contract preserves GitHub as delivery truth and Switchbard as local task outcome authority, with typed repository, pull-request, actor, permission, and expected-head identities
- [ ] #3 The contract defines Allowed, Denied, and Unknown capability states plus Pending, Confirmed, Rejected, and OutcomeUnknown operation outcomes
- [ ] #4 An executable acceptance and state-stress contract covers loading, empty, partial, stale, denied, conflict, offline, narrow-window, and high-volume states
<!-- AC:END -->
