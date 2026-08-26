---
id: TASK-55
title: Collapse three disagreeing definitions of safe-to-remove into one
status: Done
assignee: []
created_date: '2026-08-26 01:36'
labels:
  - refactor
  - safety
  - workspace
dependencies: []
ordinal: 55000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Workspace row badge, the bulk-remove sweep, and the single-row confirm dialog each carried their own answer to "is this worktree safe to remove", and they disagreed.

The row ran three checks (linked worktree, files clear, no processes). The sweep ran five (status verifiable, not dirty, not detached HEAD, branch merged, nothing running). The dialog re-derived merged-ness a third way, from BranchDeleteAssessment::needs_force(). The same worktree could read "remove ok" in green on the row and land in the sweep's "needs review" list in the same frame.

Three accuracy defects fell out of that split:

- The row promised "remove ok" on a locked worktree. git worktree list reports the locked flag; worktree.rs parsed it and threw it away, so git refused a removal the badge had already blessed.
- The 'no processes' check counted only attributed listeners and services this Switchbard instance had started. A dispatched agent is neither, so a worktree with a live agent writing in it read as "nothing running here".
- probe_worktree_staleness documented a Live fallback on git failure while the code fell through to Orphan, the most retire-me-looking badge in the set. A failed git call nominated a worktree for cleanup on no evidence.

Replaced with switchbard-core/src/removal_safety.rs: five named checks (NotPrimary, NotLocked, FilesClear, WorkLanded, NoProcesses) over tri-state Fact<T> inputs, yielding one of four verdicts (Primary, Checking, Safe, Blocked). RemovalIntent selects whether WorkLanded is required, which is what reconciles the sweep (deletes branches, so unlanded commits are at risk) with the single-row dialog (leaves the branch, so they are not) without two rule tables.

The invariant: only RemovalVerdict::Safe is ever acted on without an explicit force gesture. Fact::Pending and Fact::Unavailable are separate variants precisely so a probe still in flight renders as 'checking' while a probe that failed blocks — collapsing them is how a pending probe becomes either a false accusation or a false green.

Also retired the 'remove 2/3' score in favour of an answer. A fraction told users how close they were without telling them what to do.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One definition of safe-to-remove exists in switchbard-core and all three surfaces evaluate it
- [ ] #2 An unanswered check can never produce a Safe verdict
- [ ] #3 A probe still in flight renders as checking, not as a blocker
- [ ] #4 A locked worktree blocks removal instead of failing at git
- [ ] #5 A live dispatch run blocks removal of its worktree
- [ ] #6 A failed staleness probe no longer classifies a worktree as Orphan
<!-- AC:END -->
