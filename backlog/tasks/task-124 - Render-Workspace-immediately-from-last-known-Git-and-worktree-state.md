---
id: TASK-124
title: Render Workspace immediately from last-known Git and worktree state
status: To Do
assignee: []
created_date: '2026-09-01 17:44'
labels:
  - cold-start
  - workspace
  - git
  - github
  - performance
dependencies:
  - TASK-123
priority: high
project: Instant Cold Start
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Worktree dirt, drift, staleness, size, and landing state can take seconds or many bounded worker ticks to repopulate, leaving the Workspace incomplete after every cold launch.

Evidence: The Git probe costs roughly 6 to 8 seconds at the recorded 84-worktree scale, size probing averages about 0.65 seconds per worktree, and landing probes include network-backed GitHub observations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Hydrate Git metadata, worktree size, and landing observations on the first frame with their original capture times and provenance.
- [ ] #2 Recompute derived counts such as retired worktrees from hydrated inputs rather than persisting a second authority.
- [ ] #3 Cached metadata never authorizes single or bulk worktree removal; the existing fresh preflight remains mandatory.
- [ ] #4 Unknown, failed, confirmed-empty, and refreshed Git or GitHub results remain distinguishable, and a failed refresh preserves the last-known display without presenting it as current.
- [ ] #5 The real 143-worktree render fixture stays within the existing frame budget with fully hydrated cached state.
<!-- AC:END -->
