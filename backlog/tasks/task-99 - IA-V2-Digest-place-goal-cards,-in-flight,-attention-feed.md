---
id: TASK-99
title: 'IA V2: Digest place - goal cards, in-flight, attention feed'
status: To Do
assignee: []
created_date: '2026-09-01 02:24'
labels:
  - ia
  - gui
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The landing place (trajectory: IA V2). Goal cards lead (existing goal statuses), then in-flight tasks, then the attention feed: rows computed from owning objects (PR probe, run reaper, server watch, port scan, removal_safety) with inline icon actions (review/merge, open mock, retry, restart, remove, kill) that reuse those surfaces' command verbs. Nothing stored on tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Feed rows computed live from existing probes; each deep-links to its owning surface
- [ ] #2 Inline actions invoke the same verbs as the owning surfaces (no second implementation)
<!-- AC:END -->
