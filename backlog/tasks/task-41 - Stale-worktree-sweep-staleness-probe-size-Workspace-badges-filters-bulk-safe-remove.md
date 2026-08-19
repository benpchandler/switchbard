---
id: TASK-41
title: >-
  Stale worktree sweep: staleness probe + size, Workspace badges/filters, bulk
  safe remove
status: To Do
assignee: []
created_date: '2026-08-19 19:55'
labels:
  - workspace
dependencies: []
ordinal: 41000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make Switchbard the place to see and retire dead worktrees: which worktrees are merged/orphaned, how big they are, and remove the safe ones in bulk as a view + one action. Verified 2026-08-19: ~/Dev/.worktrees holds 138 worktrees / 141 GB; vs origin/main: 89 merged, 17 no-upstream (orphan), 30 live, 36 dirty (any class). Workspace today shows branch, dirty/clean, ahead/behind (DriftProbe, git_probe.rs), activity, listeners — not merged-into-main, size, or bulk action. The merged check exists only inside the Remove dialog (assess_branch_delete / BranchDeleteAssessment in worktree_remove.rs; dialog UI ui/workspace/mod.rs ~L1485-1500). Board already has bulk select/edit (TASK-26) — reuse its selection pattern. Core deliverable: WorktreeStaleness probe — Merged{base} | Orphan (no upstream) | Live — per worktree alongside DriftProbe, plus on-disk size (du, cached, lazily refreshed); pure fn of (repo_path, worktree_path). UI: badge + size column in Workspace rows; filter chips All/Merged/Orphan/Live/Dirty. Bulk remove: multi-select -> one Remove dialog reusing per-row collect_dirty_files + assess_branch_delete; clean+merged removable ('also delete branch' default on); dirty/unmerged auto-deselected into a 'needs review' list, never force-removed in bulk. Nudge: top-bar count 'N retired worktrees' when merged+clean > 0. Constraints: CLAUDE.md, AGENTS.md, power-of-10-overrides.md; never force-remove/delete unmerged in bulk; primary worktree (is_primary_worktree) never selectable. Out of scope: the one-time cleanup of today's 89/17; dispatch/launchd work (TASK-11/12). Note: if ~/.switchbard/config.toml is empty after an app quit, that is the TASK-22 repro — note it, don't absorb it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Workspace shows Merged/Orphan/Live badge and on-disk size for every worktree of every tracked repo; filter chip Merged lists about 89 on the 2026-08-19 machine
- [ ] #2 WorktreeStaleness probe is a pure function of (repo_path, worktree_path) returning Merged{base} | Orphan | Live, computed alongside DriftProbe; size is du-based, cached, refreshed lazily
- [ ] #3 Selecting 5 merged+clean worktrees and choosing Remove deletes all 5 and their branches; git worktree list agrees
- [ ] #4 A dirty or unmerged worktree in the bulk selection is auto-deselected into a needs-review list and left untouched; nothing is ever force-removed or force-branch-deleted in bulk
- [ ] #5 Primary worktree is never selectable for removal
- [ ] #6 Top bar shows 'N retired worktrees' nudge when merged+clean count > 0
- [ ] #7 Unit tests cover probe fixtures: merged, orphan, live, dirty; a kittest covers bulk-dialog auto-deselect
- [ ] #8 mise run ci green on the PR; CHANGELOG entry added
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Built in its own worktree; PR against main references this task id
- [ ] #2 mise run ci (fmt + clippy -D warnings + test) green locally and in CI on macOS + Ubuntu
- [ ] #3 Perf smoke run if Workspace render path touched (CLAUDE.md render-path perf rule)
- [ ] #4 docs/product-trajectory.md updated if the feature changes the documented plan
<!-- DOD:END -->
