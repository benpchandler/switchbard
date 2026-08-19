---
id: TASK-31
title: Tombstone filename collides on same-second consecutive wipes
status: To Do
assignee: []
created_date: '2026-08-05 16:39'
labels:
  - bug
  - hardening
dependencies: []
priority: low
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Verifier finding (007711f pass, non-blocking): config.tombstone-<ts>.toml uses second-precision %Y%m%d-%H%M%S; two wipes within the same second silently lose the earlier tombstone. Same bug class as TASK-30's loaded_at_unix fix — apply the same millisecond (or counter-suffix) pattern. Lane B surface (crates/switchbard-core/src/config.rs).
<!-- SECTION:DESCRIPTION:END -->
