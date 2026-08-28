---
id: TASK-65
title: 'Format fork: swap mutations.rs to the native write layer'
status: Done
assignee:
  - '@claude'
created_date: '2026-08-28 18:40'
updated_date: '2026-08-28 20:19'
labels:
  - format-fork
dependencies:
  - TASK-63
  - TASK-64
priority: high
ordinal: 64000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Re-implement the nine mutation functions in backlog/mutations.rs on top of backlog::write plus the ID allocator, with unchanged signatures, so GUI, dispatch, and refine callers do not change. Includes the file-move dispositions (archive -> backlog/archive/tasks/, complete -> backlog/completed/ with the Done-only rule mutations.rs already documents) and create filename convention. Behavior parity notes: label add/remove reads the file at write time (freshness semantics of --add-label); --ac semantics stay append-only. Update the module header docs that cite the CLI as authority (mutations, dispatch, refine). Delete parse_created_task_id stdout scraping - create returns the id directly. Replace tests/backlog_cli_mutations.rs real-CLI round trips with native-layer equivalents.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 All nine mutation functions write natively with unchanged signatures; no caller outside backlog/ changes
- [x] #2 Archive and complete file moves implemented with the Done-only rule preserved
- [x] #3 parse_created_task_id and its pinned-output tests deleted; create returns the new task id directly
- [x] #4 Module header docs in mutations, dispatch, and refine no longer name the CLI as the write authority
- [x] #5 tests/backlog_cli_mutations.rs replaced by native-write-layer round trips; mise run ci green
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions and findings during the swap:
- swap_backlog_label is now STRICT (fails when the task does not carry the source label) via the new write::swap_task_label - a claim is a race for a token, and the CLI half-winning (adding the target anyway) was the double-dispatch window. Release paths already treat swap errors as best-effort, so callers hold.
- Subtasks keep the CLI decimal-child convention (TASK-7 -> TASK-7.1): write_new_task_file now takes a string id and the allocator grew child-ordinal allocation over the same worktree+branch scan. Nested subtask parents are rejected (one decimal level, matching everything on disk).
- Status validation kept at the facade with the CLI message shape (Invalid status: X. Valid statuses are: ...) - the status-vocabulary offer flow keys off that refusal. TASK-68 moves it into the write layer proper.
- AC1 nuance: the eight edit-shaped functions kept all callers unchanged; the one create caller in the GUI dropped its parse_created_task_id stdout scrape, which AC3 mandates.
- Two real bugs found by the existing suites during the swap: (1) tmp+rename bypassed the read-only bit a locked task file carries - atomic_write now refuses read-only files and preserves the mode across the rename; (2) the id-reservation had a create_new-then-write-content gap a rival could misread as a stale claim and steal - staleness is now judged from mtime, which exists atomically with the claim (concurrent-create test caught two racers minting one id).
- Four GUI tests raced the native write speed (transient status messages and a repaint budget the slow CLI subprocess used to mask); fixed by accepting either race order, real on-disk fixtures for the two dispatch-toggle tests, and run_steps where the asserted state is synchronous.
- FOR TASK-67: qa_reverify* and backlog_controls fixtures still shell the real backlog CLI to BUILD fixtures (backlog init / task create); they must migrate to native creates before the mise pin is removed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Swapped mutations.rs onto the native write layer: all nine mutation functions now resolve task ids to files and write through backlog::write + the allocator, unchanged signatures, CLI subprocess gone from the write path. Archive/complete file moves with the Done-only rule, strict label-swap claim semantics, decimal subtask ids, status validation with the CLI message shape, create returns the id (parse_created_task_id deleted). Module docs in backlog/mutations/dispatch/refine/parse re-attributed; tests/backlog_cli_mutations.rs replaced by tests/backlog_mutations.rs (20 native round trips). Two real bugs fixed along the way (read-only bypass via rename; reservation steal race, now mtime-based). mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
