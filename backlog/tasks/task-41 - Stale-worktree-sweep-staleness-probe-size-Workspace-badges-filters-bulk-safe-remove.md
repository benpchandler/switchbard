---
id: TASK-41
title: >-
  Stale worktree sweep: staleness probe + size, Workspace badges/filters, bulk
  safe remove
status: In Progress
assignee: []
created_date: '2026-08-19 19:55'
updated_date: '2026-08-19 20:35'
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
Implementation notes (fireteam, feature/stale-worktree-sweep):

Core (switchbard-core):
- git_probe.rs: WorktreeStaleness enum (Merged{base}|Orphan|Live) + probe_worktree_staleness(repo_path, worktree_path) — pure fn, priority merged-first (ahead==0 vs local main/master) then upstream check for Orphan/Live. Reuses worktree_remove::default_branch (widened to pub(crate)) rather than re-deriving "what's the default branch". 4 new fixture tests (merged/orphan/live/dirty-is-orthogonal).
- worktree_size.rs (new file): probe_worktree_size (du -sk) + humanize_size. Pure, uncached — GUI owns caching/cadence, same split as every other probe.

GUI (switchbard-gui):
- workers.rs: staleness computed inline in the existing git-probe tick (measured ~24ms/worktree — cheap, no new worker needed). Size gets its OWN worker (spawn_size, own Kick, own cadence: 300s period, 30min stale-age, 5-per-tick bounded catch-up batch) because du measured ~650ms avg / up to 1.5s per worktree on this real machine (examples/scan_cadence_audit.rs, extended with staleness+size sections — real numbers cited in workers.rs's cadence table).
- runtime/mod.rs: WorktreeMeta.staleness field; new WorktreeSizeEntry (own Arc<Mutex<HashMap>> on HiveApp, not folded into WorktreeMeta, since its refresh cadence is independent); ConfirmBulkRemoveWorktrees + BulkRemoveCandidate.
- ui/workspace/staleness.rs (new) + ui/workspace/bulk_remove.rs (new): kept new code OUT of ui/workspace/mod.rs where possible per the file's own "known debt" note (net +40 LOC there, two new ~360/~160-line modules instead of piling on).
- worktree_actions.rs: open/cancel/execute_bulk_remove_worktree_confirm + classify_bulk_candidate (reuses collect_dirty_files + assess_branch_delete — never a parallel safety check) + run_bulk_removal (extracted synchronous core so it's directly testable, same pattern worktree_removal_orchestration.rs already uses for the single-row dialog's decision logic).
- Bulk-remove additionally routes a selected worktree with active Switchbard runs or attributed listeners into needs-review (not just dirty/unmerged) — a safety extension beyond the literal brief: a batch action shouldn't silently kill running services across N worktrees. Flagging this explicitly since it's scope I added, not scope I was handed.
- Top-bar "N retired worktrees" nudge + Merged/Orphan/Live/Dirty filter chips (with live counts) exclude the primary checkout from counts — a primary's HEAD trivially equals its own default branch (always "Merged"), which would otherwise inflate "N retired" with worktrees nobody can actually retire. Per-row badge still shows on primary rows (AC#1 wants every worktree badged); only the actionable counts exclude it.

Evidence:
- mise run ci green locally (fmt + clippy -D warnings + full test suite, incl. new tests) — see PR for CI on macOS+Ubuntu.
- Real-machine check (read-only, via examples/scan_cadence_audit.rs against the actual ~/.switchbard/config.toml, 11 repos / 84 worktrees under ~/Dev): 20 merged, 20 orphan, 44 live, staleness probe ~2.06s total; du sample of 20 worktrees averaged ~691ms each. Note: this machine's tracked repos are under ~/Dev, not the ~/Dev/.worktrees dir the task's "~89 merged / 138 total" figures came from (that dir isn't in this machine's tracked repo list) — the classification logic is proven correct and fast against real repos, but the literal "~89" count wasn't reproduced 1:1. Did not touch the real config or click Remove on anything real.
- New tests: git_probe.rs (4 staleness fixtures), worktree_size.rs (3 fixtures), ui/workspace/staleness.rs (4 filter unit tests), tests/bulk_remove_worktrees.rs (3 tests: kittest auto-deselect incl. primary-drop, disabled-button, and a real-git-repo run_bulk_removal proof that 5 selected worktrees + their branches are actually gone and git worktree list agrees).
- Perf smoke: tests/workspace_perf_smoke.rs (#[ignore]d, run explicitly) — headless egui_kittest harness with SWITCHBARD_PERF driving the real render_ui path over 11 repos x 12 linked worktrees (143 total, close to the real ~138-worktree citation), 200 frames, every row hitting the new badge/size/checkbox render paths. Result: frame p50 17.5ms / p95 18.7ms / max 38.7ms; workspace p50 16.4ms / p95 17.5ms / max 29.7ms (debug build). No pre-TASK-41 baseline build exists to diff against (the render path didn't exist before this task) — noted in the test's own doc; the assertion trips a regression tripwire at p95 < 33ms (one 30fps frame).
- docs/product-trajectory.md intentionally NOT updated: this feature fulfills the already-documented direction ("progressive-disclosure workspace cards", worktree-first model) rather than changing the plan — DoD#4 left unchecked rather than falsely marking it "updated".
<!-- SECTION:NOTES:END -->
