---
id: TASK-122
title: Build the bounded sharded startup snapshot kernel
status: To Do
assignee: []
created_date: '2026-09-01 17:44'
labels:
  - cold-start
  - cache
  - core
  - safety
dependencies:
  - TASK-121
priority: high
project: Instant Cold Start
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Without one safe cache contract, every surface could invent its own persistence, a corrupt or oversized file could make launch slower, and cached observations could be mistaken for live authority.

Evidence: The existing agent-context cache already demonstrates versioned atomic JSON persistence, while config.toml demonstrates same-directory temporary-file replacement. Every other runtime cache currently disappears at process exit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define typed per-domain shard envelopes with schema version, producer version, capture time, domain identity, configured repo identity, and validated payload.
- [ ] #2 Reject oversized, truncated, unknown-version, wrong-domain, and semantically invalid shards independently before unbounded allocation; one bad shard cannot invalidate healthy shards or delay launch.
- [ ] #3 Write each shard with same-directory temporary files and atomic replacement, preserving the prior good snapshot on failure and enforcing private directory and file permissions.
- [ ] #4 Keep logs, prompts, transient editor state, selections, confirmations, PID capabilities, and other non-read-model data out of the cache.
- [ ] #5 Prove round trips, crash points, corruption isolation, permission behavior, pruning, and the startup performance budget from the approved contract on macOS and Linux-compatible paths.
<!-- AC:END -->
