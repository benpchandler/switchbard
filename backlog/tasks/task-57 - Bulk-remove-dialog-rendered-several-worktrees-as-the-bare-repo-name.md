---
id: TASK-57
title: Bulk-remove dialog rendered several worktrees as the bare repo name
status: Done
assignee: []
created_date: '2026-08-26 13:40'
labels:
  - bug
  - workspace
  - ux
dependencies: []
ordinal: 57000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reported from the live app: the bulk-remove confirmation offered to remove 9 clean/merged worktrees, and several lines read as the repo itself, so it looked like an offer to delete primary checkouts.

The primaries were never at risk. Verified: open_bulk_remove_worktree_confirm drops anything is_primary_worktree accepts, that check canonicalizes both paths, and the four suspect worktrees are real directories under ~/.treehouse/budget-404c3c/{1,2,3,4}/budget with canonical paths distinct from /Users/bpc/Dev/budget. Only /Users/bpc/Dev/budget is git's primary and it was excluded.

The bug was the name. inferred_worktree_name used path.file_name() alone, and that layout nests each checkout under a per-worktree parent while naming the leaf after the repo. The leaf therefore names the repo, not the worktree: four worktrees rendered identically, and identically to the primary.

A name that cannot distinguish two things is not doing the one job a name has, and a destructive confirmation is the worst place to find that out.

Fix, two layers:
- worktree_display_name falls back to the branch when the leaf merely repeats the repo name, and to parent/leaf when there is no branch (detached HEAD). A configured alias still wins.
- The bulk-remove dialog shows the branch inline and the full path on hover. It is the one dialog that deletes things, so it states identity twice over rather than trusting a single string to be unique.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Two worktrees of the same repo never render the same display name
- [ ] #2 A worktree never renders as the bare repo name
- [ ] #3 The bulk-remove dialog shows branch and path per candidate
- [ ] #4 Primary checkouts remain excluded from the bulk-remove candidate list
<!-- AC:END -->
