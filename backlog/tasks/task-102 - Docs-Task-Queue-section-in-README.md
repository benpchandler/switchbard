---
id: TASK-102
title: 'Docs: Task Queue section in README'
status: In Review
assignee: []
created_date: '2026-09-01 02:24'
updated_date: '2026-09-01 02:28'
labels:
  - docs
  - dogfood
  - dispatched
dependencies: []
priority: low
project: Task Queue
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a '## Task Queue' section to README.md (after the existing feature overview) describing: teeing tasks up with 'sb queue send', the stack-rank order deciding pickup, the LangGraph orchestrator (link orchestrator/README.md) claiming and working tasks to a PR, and live progress in the Dispatches view. Keep it to one tight paragraph plus a 3-5 line example block. Match the README's existing tone.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 README.md contains a Task Queue section covering queue verbs, rank-ordered pickup, the orchestrator, and live progress
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Dispatch failed: unproven outcome: AC #1 unchecked: README.md contains a Task Queue section covering queue verbs, rank-ordered pickup, the orchestrator, and live progress

Dispatch PR: https://github.com/benpchandler/switchbard/pull/76
<!-- SECTION:NOTES:END -->
