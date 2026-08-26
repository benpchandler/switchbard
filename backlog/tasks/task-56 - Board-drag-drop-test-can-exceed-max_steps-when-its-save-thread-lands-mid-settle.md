---
id: TASK-56
title: >-
  Board drag/drop test can exceed max_steps when its save thread lands
  mid-settle
status: To Do
assignee: []
created_date: '2026-08-26 01:40'
labels:
  - flaky-test
  - backlog-board
  - test-infra
dependencies: []
ordinal: 56000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Observed once during a full `mise run ci` on 2026-08-25: `board_drag_and_drop_between_columns_queues_a_status_change` failed with "Harness::run exceeded max_steps (4). Repaint causes: []". Not reproduced since — 5 isolated runs, 6 full-binary runs, and 8 concurrent full-binary runs all green — so this is filed as a named flake, not a fixed bug.

Mechanism (read from egui_kittest 0.36.1 lib.rs _try_run, not yet proven):

The settle loop breaks only when root_viewport_output().repaint_delay != ZERO. A background thread calling ctx.request_repaint() sets that delay to ZERO without registering an in-frame repaint cause, which is exactly the empty `repaint_causes: []` in the panic. Dropping a card spawns a real backlog-CLI save thread (the test's own assert accepts both "saved TASK-2" and "save TASK-2 failed"), and that thread requests a repaint when it finishes. If it lands inside drag_and_drop's final harness.run() settle window, the loop keeps stepping and trips max_steps at 4.

Ruled out: predicted_dt is a fixed step_dt (250ms), not a measured frame time, so LANDING_FLASH_REPAINT_INTERVAL's 300ms clears kittest's saturating_sub deterministically regardless of machine load. The tight-margin note in board.rs is accurate but is not this failure.

If it recurs, the fix is probably to stop using plain run() for the post-release step specifically — a repaint requested from another thread is not something a settle loop can ever settle — rather than raising max_steps, which would only widen the window. The sibling test board_drag_failure_rolls_back_the_card_and_reloads_the_cache deliberately takes on this same background-thread race and may already show the right pattern.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The test no longer trips max_steps under repeated full-suite CI runs
- [ ] #2 The fix addresses the cross-thread repaint rather than raising max_steps
<!-- AC:END -->
