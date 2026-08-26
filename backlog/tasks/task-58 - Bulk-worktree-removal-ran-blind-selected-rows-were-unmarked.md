---
id: TASK-58
title: Bulk worktree removal ran blind; selected rows were unmarked
status: Done
assignee: []
created_date: '2026-08-26 14:51'
labels:
  - ux
  - workspace
dependencies: []
ordinal: 58000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two reports from the live app, one fix each.

1. Selecting rows only ticked a 14px checkbox. 'Select all merged+clean' marks an arbitrary subset of a long list, and a checkbox is not an answer to 'what did that just take?' at a glance. The row itself now carries a cool selection wash plus an outline, reusing the same Frame fill mechanism the primary-worktree tint already uses. Selection wins over the primary tint; the two can never actually collide, since primaries render no checkbox and are dropped from the candidate list.

2. Confirming a bulk removal closed the dialog and then showed nothing until the whole list refreshed at the end. Each candidate is its own git worktree remove (plus maybe a git branch -d), so a nine-worktree sweep is many seconds that are indistinguishable from a hang.

Wired the existing sync::Progress channel - the one the Backlog bulk actions already use - rather than building a second progress mechanism. Its own channel, not the shared bulk_progress: they are independent surfaces, and Progress::begin resets, so sharing would let one sweep silently reset the other's bar.

run_bulk_removal takes an on_item_done callback rather than a Progress handle, keeping its documented freedom from shared state and egui. It fires on every exit path including a failed removal, because the bar measures position in the batch, not success - a bar that stalls on the first failure says 'still working' about work that already stopped.

The bar takes the button's place while a sweep is live, matching the Backlog toolbar's rule: offering to start a second removal mid-run is offering a race over the same worktree list.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A bulk-selected worktree row is visually distinct from an unselected one
- [ ] #2 Every label on a selected row still clears WCAG AA
- [ ] #3 A bulk removal shows determinate progress while it runs
- [ ] #4 Progress advances on failed removals, not only successful ones
- [ ] #5 The remove button is unavailable while a sweep is in flight
<!-- AC:END -->
