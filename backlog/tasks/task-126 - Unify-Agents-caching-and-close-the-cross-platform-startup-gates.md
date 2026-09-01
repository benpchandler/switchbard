---
id: TASK-126
title: Unify Agents caching and close the cross-platform startup gates
status: To Do
assignee: []
created_date: '2026-09-01 17:44'
labels:
  - cold-start
  - agents
  - verification
  - docs
dependencies:
  - TASK-125
priority: medium
project: Instant Cold Start
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Leaving agent context on a separate persistence path and validating only the happy path would create two cache contracts and allow regressions in failure, scale, or platform behavior.

Evidence: Agent context is currently the only durable runtime cache, stored separately in agent-context-cache.json, while no existing test proves useful content on the first native frame with live probes blocked.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bring agent context and hooks under the bounded shard contract with a safe compatibility path for the existing cache.
- [ ] #2 With all live probes blocked, representative content for every major tab is visible on the first rendered frame; without cache, each surface shows an honest never-loaded or placeholder state.
- [ ] #3 Corrupting any single shard leaves other surfaces usable, and refresh failure always preserves last-known content with explicit age and failure evidence.
- [ ] #4 Run the full state and stress matrix, accessibility and layout checks, the cold-start integration harness, existing render performance smokes, and mise run ci.
- [ ] #5 Verify private cache permissions, byte bounds, atomic replacement, and startup behavior on both macOS and Linux CI.
- [ ] #6 Update architecture, backing-store, freshness, privacy, and troubleshooting documentation with exact verification commands and known limitations.
<!-- AC:END -->
