---
id: TASK-119
title: Add guarded CI, branch-update, and merge operations
status: To Do
assignee: []
created_date: '2026-09-01 17:12'
labels:
  - github
  - ops
  - ci
  - merge
dependencies:
  - TASK-118
priority: high
project: GitHub Operations
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: CI recovery and merge completion remain fragmented across GitHub, and a stale or overpowered command could merge the wrong revision or bypass repository policy.

Evidence: The approved version 1 scope includes rerun failed checks, update branch, enable auto-merge, merge, and merge-queue handling while explicitly excluding admin bypass, ruleset edits, and branch deletion.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Users can rerun failed checks, update the branch, enable auto-merge, enqueue or merge when eligible, and see current command eligibility
- [ ] #2 Update-branch flow warns when approvals may become stale and refreshes observations after completion
- [ ] #3 Merge-family commands bind the exact expected head OID and never expose admin bypass, ruleset mutation, or branch deletion
- [ ] #4 Required checks, reviews, merge queue, permissions, rate limits, conflicts, and offline failures produce explicit non-success outcomes
- [ ] #5 Operation receipts and readback prove the resulting GitHub state without changing local task completion
<!-- AC:END -->
