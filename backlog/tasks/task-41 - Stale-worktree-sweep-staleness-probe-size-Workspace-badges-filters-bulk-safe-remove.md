---
id: TASK-41
title: >-
  Stale worktree sweep: staleness probe + size, Workspace badges/filters, bulk
  safe remove
status: In Progress
assignee: []
created_date: '2026-08-19 19:55'
updated_date: '2026-08-19 20:52'
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
- [x] #1 Workspace shows Merged/Orphan/Live badge and on-disk size for every worktree of every tracked repo; filter chip Merged lists about 89 on the 2026-08-19 machine
- [x] #2 WorktreeStaleness probe is a pure function of (repo_path, worktree_path) returning Merged{base} | Orphan | Live, computed alongside DriftProbe; size is du-based, cached, refreshed lazily
- [x] #3 Selecting 5 merged+clean worktrees and choosing Remove deletes all 5 and their branches; git worktree list agrees
- [x] #4 A dirty or unmerged worktree in the bulk selection is auto-deselected into a needs-review list and left untouched; nothing is ever force-removed or force-branch-deleted in bulk
- [x] #5 Primary worktree is never selectable for removal
- [x] #6 Top bar shows 'N retired worktrees' nudge when merged+clean count > 0
- [x] #7 Unit tests cover probe fixtures: merged, orphan, live, dirty; a kittest covers bulk-dialog auto-deselect
- [x] #8 mise run ci green on the PR; CHANGELOG entry added
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Built in its own worktree; PR against main references this task id
- [x] #2 mise run ci (fmt + clippy -D warnings + test) green locally and in CI on macOS + Ubuntu
- [x] #3 Perf smoke run if Workspace render path touched (CLAUDE.md render-path perf rule)
- [ ] #4 docs/product-trajectory.md updated if the feature changes the documented plan
<!-- DOD:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
--- Audit fixes (PR #23 review, MERGE verdict, 3 non-blocking) ---
1. DRY: extracted worktree_remove::commits_ahead (renamed from private count_commits_ahead, widened to pub(crate)) as the single authoritative "commits ahead of base" primitive. probe_worktree_staleness now calls it (at worktree_path, base..HEAD) instead of its own probe_ref_drift-based ahead check — badge and single-row dialog can no longer disagree about "is it merged".
2. Perf: retired_worktree_count is no longer recomputed every top-bar frame (was cloning Vec<Repo>/Vec<WorktreeRef> + locking meta on every frame, every tab). Extracted shared is_retired_worktree/worktree_is_primary predicates into runtime/mod.rs (also dedupes 2 near-identical copies in staleness.rs); the git-probe worker now computes+caches the count once per tick (Arc<Mutex<usize>> on Channels/HiveApp) and the top bar just reads it. Perf smoke re-run: p95 frame 18.6ms (was 18.7ms) — flat, as expected for this fixture size; the real win is on real multi-tab-visible machines with more worktrees.
3. Nit: run_bulk_removal no longer swallows a git branch -d failure via .is_ok() — added first_branch_error (kept separate from first_error, which is reserved for the more severe worktree-removal failure), surfaced in status_message. New test proves it: a candidate whose worktree removal succeeds but branch name is stale fails delete_branch, and the failure shows up in both the summary and the status line.

Evidence: mise run ci green (fmt+clippy -D warnings+full suite incl. 2 new tests, one for branch-delete-failure surfacing); perf smoke re-run, no regression. Pushed as follow-up commit(s) on feature/stale-worktree-sweep, no squash/force-push.
<!-- SECTION:NOTES:END -->
