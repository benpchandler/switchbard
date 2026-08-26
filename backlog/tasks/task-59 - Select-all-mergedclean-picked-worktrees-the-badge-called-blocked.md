---
id: TASK-59
title: Select all merged+clean picked worktrees the badge called blocked
status: Done
assignee: []
created_date: '2026-08-26 15:29'
labels:
  - bug
  - workspace
  - consistency
dependencies: []
ordinal: 59000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reported from the live app: 'Select all merged+clean' selected a worktree whose own row badge read 'remove blocked'.

is_retired_worktree was a fourth definition of 'safe to remove', missed when the other three were consolidated into removal_safety. It checked two things - staleness == Merged, and not dirty - where the badge applies five. A worktree that was merged and clean but had a listener, a Switchbard-started service, a live dispatch run, or a git lock passed the selector and failed the badge.

A bulk-select that hands you rows the app then refuses to remove is worse than no bulk-select.

It now evaluates the same RemovalSafety every other surface does, and only a Safe verdict counts. Both callers supply the process counts: the Workspace view from its per-frame snapshot, and the git-probe worker (which feeds the top-bar 'N retired' nudge) from its own channel handles, snapshotted once per tick rather than re-locked per worktree.

Added attached_processes_for as the single derivation of 'what is holding this worktree', so the nudge count and HiveApp::attached_processes cannot disagree.

An unprobed worktree still never counts - now because it evaluates to Checking rather than as a side effect of two Options being None.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Select all merged+clean never selects a worktree whose badge reads blocked
- [ ] #2 The top-bar retired count matches what the button selects
- [ ] #3 A locked or busy worktree is excluded from both
- [ ] #4 Render-path p95 does not regress
<!-- AC:END -->
