---
id: TASK-118
title: Add guarded pull-request review operations
status: To Do
assignee: []
created_date: '2026-09-01 17:12'
labels:
  - github
  - ops
  - review
  - backend
dependencies:
  - TASK-117
priority: high
project: GitHub Operations
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Users must leave Switchbard for routine review work, while an unguarded write path could submit the wrong review against a changed pull request.

Evidence: The approved version 1 scope includes open diff, comment, approve, and request changes, with explicit permission, self-review, stale-head, and post-submit confirmation states.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Open diff, comment, approve, and request-changes actions are exposed only when their capability is Allowed
- [ ] #2 Self-authored approval is disabled and Denied or Unknown capabilities explain why no action is available
- [ ] #3 Every mutation binds repository, pull-request number, viewer, and expected head OID and rejects a changed head before submission
- [ ] #4 Mutations run off the UI thread, require appropriate confirmation, and return a durable operation receipt
- [ ] #5 The app reads GitHub back after submission and distinguishes Confirmed, Rejected, and OutcomeUnknown results
<!-- AC:END -->
