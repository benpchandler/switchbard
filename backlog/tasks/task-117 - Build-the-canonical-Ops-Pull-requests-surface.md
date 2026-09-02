---
id: TASK-117
title: Build the canonical Ops > Pull requests surface
status: To Do
assignee: []
created_date: '2026-09-01 17:12'
labels:
  - github
  - ops
  - gui
  - design
dependencies:
  - TASK-116
priority: high
project: GitHub Operations
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Users have no in-app place to scan pull-request status, understand blockers, or reach guarded actions across tracked repositories.

Evidence: Owner feedback removed Why Now and made Ops > Pull requests canonical. The approved plan uses compact PR, State, Date, and Actions columns and keeps Digest as a mirror only when the same item already exists in Ops.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Ops exposes a Pull requests subview as the canonical location for repository pull-request status and actions
- [ ] #2 The default table uses compact PR, State, Date, and Actions columns, with readable repository and pull-request identity
- [ ] #3 Loading, empty, partial, stale, denied, offline, narrow-window, keyboard, and high-volume states are implemented and verified
- [ ] #4 Digest deep-links to the canonical Ops item and does not create a second pull-request object model
- [ ] #5 The UI does not restore the removed Why Now column or deleted redundant workflow cards
<!-- AC:END -->
