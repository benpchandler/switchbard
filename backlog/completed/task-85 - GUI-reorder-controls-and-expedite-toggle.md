---
id: TASK-85
title: 'GUI: reorder controls and expedite toggle'
status: Done
assignee: []
created_date: '2026-08-31 22:01'
updated_date: '2026-09-01 00:12'
labels:
  - backlog
  - gui
  - design
dependencies:
  - TASK-82
  - TASK-84
priority: high
project: Stack Ranking
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Interactive GUI half (trajectory: 'Stack ranking'). Move up / move down controls on project rows and task rows in the backlog view, plus an expedite/unexpedite toggle on task rows; all mutations write through backlog/ordering.rs (one-writer invariant) and refresh via the existing rebuild path. v1 is buttons, not drag-and-drop, unless the design pass says otherwise. Run the design-state skill before building: enumerate states (ranked/unranked rows, first/last position, expedited, mixed sparse lists, empty ordering file) and bind each to evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 design-state matrix completed for the reorder controls before implementation, recorded in this task's notes
- [x] #2 Move up/down on project and task rows reorders within the sibling scope and persists via backlog/ordering.rs
- [x] #3 Expedite toggle adds/removes the task from the lane and the row marker updates in place
- [x] #4 First/last rows disable the no-op direction; unranked rows can be pulled into the ranked set by moving them
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Design-state matrix (recorded before implementation, per AC #1):
1. Empty/missing ranking.yml - lens renders today's order; move-up offered (enters ranked set), move-down disabled. Evidence: core is_empty short-circuit test + GUI ordering test.
2. Partially ranked project list - ranked projects lead in rank order within their initiative group, unranked keep name sort. Evidence: GUI unit test.
3. Partially ranked task rows - ranked members first, then fallback; group rows re-sorted by repo.tasks position (the computed-order authority), not the toolbar sort key. Evidence: GUI unit test.
4. Expedited row - visible marker pill from theme semantics, no click needed; expedited rows lead their status tier. Evidence: GUI test + visual pass.
5. First/last ranked row - move-up disabled at rank 0; move-down on the LAST ranked row unranks it (hover text says so). Evidence: core move-semantics tests.
6. Unranked row - move-up enters the ranked set at its bottom (first press jumps the unranked tail - truthful for sparse rank); move-down disabled with tooltip. Evidence: core tests + tooltip.
7. Sub-issue rows - moves anchor against SCOPE siblings (parent's sub-issue list), never display neighbors. Evidence: core tests with sub-issue fixtures.
8. Stale rank entries - ignored on read, pruned on next write; controls act on pruned view. Evidence: core tests (green).
9. Concurrent mutation - standard spawn_* thread, core write, refresh_backlog_repo_cache, status line, repaint; failures land in the status surface. Evidence: existing pattern + status text.
10. Rapid clicks - each press independent op on the re-read file; core returns Unchanged when placement already true. Evidence: core idempotence tests.
11. Cross-repo scope - project rank resolves per owning repo (min across scoped for name-merged groups); task moves write to the row's own repo root. Evidence: GUI ordering test.
12. Malformed ranking.yml - loads empty with warning; a move fails closed into the status line. Evidence: core tests (green).
13. ~100 ranked items - order precomputed from snapshot, no per-frame IO; SWITCHBARD_PERF smoke before close. Evidence: perf ledger run.
14. Narrow window/long titles - fixed-width glyph buttons precede the flexible title so they never clip first. Evidence: visual pass.
15. Keyboard-only - GAP (pre-existing: the lens has no focus-order contract); explicitly deferred, not silently skipped.
16. Triage list lens - deliberately untouched; cross-repo rank composition is trajectory-deferred.
Decisions forced by the matrix: move semantics live in CORE (rank_task_move/rank_project_move) so GUI and future CLI share one testable authority; down-on-last-ranked = unrank and up-on-unranked = enter-ranked-at-bottom are the sparse-rank-honest arrow meanings.

Shipped. Move semantics live in core (rank_task_move/rank_project_move, unit-tested): up swaps ranked siblings / enters the ranked set at its bottom for unranked; down swaps / unranks the last-ranked; all against SCOPE siblings, never display neighbors. GUI: triangle_button arrows on Projects-lens task rows and project headers (disabled states + hover text per the matrix), expedite toggle in the detail rail next to Refine/Dispatch, all through the standard Pending -> spawn_* -> refresh path (spawn_backlog_rank_move_task/_project, spawn_backlog_expedite_set). Perf A/B: arrows+pills cost ~3.1ms p95 at a hostile 400-rows-all-ranked debug fixture (~9us/row); dominant lens cost remains pre-existing unvirtualized rows (TASK-13). Ledger: docs/perf/runs/2026-09-01-projects-rank-smoke.json. Known gap from the matrix: keyboard-only focus order (pre-existing lens-wide, state 15) deferred.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reorder controls + expedite toggle in the GUI, over core-owned move semantics. Arrows on Projects-lens project headers and task rows (rank_arrows: up disabled at rank 0, down disabled while unranked, hover text explains the sparse-rank meanings incl. down-on-last = unrank and up-on-unranked = enter ranked set); expedite/unexpedite toggle in the detail rail with lane guidance. Mutations ride the one-writer path: Pending fields -> HiveApp spawn_* threaded mutators -> switchbard_core rank_task_move/rank_project_move/expedite_task -> refresh_backlog_repo_cache -> status line + repaint. Evidence: core move-semantics tests, accesskit test for marker+toggle, perf smoke with A/B and ledger entry; design-state matrix recorded in notes before implementation.
<!-- SECTION:FINAL_SUMMARY:END -->
