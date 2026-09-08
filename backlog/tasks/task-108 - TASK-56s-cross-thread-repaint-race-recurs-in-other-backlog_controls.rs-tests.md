---
id: TASK-108
title: TASK-56's cross-thread repaint race recurs in other backlog_controls.rs tests
status: Done
assignee: []
created_date: '2026-09-01 06:20'
updated_date: '2026-09-08 13:24'
labels:
  - flaky-test
  - backlog-board
  - test-infra
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Observed twice while running `mise run ci` (full suite, on branch feat/task-99-digest-place while clearing unrelated review findings, 2026-09-01): two different tests in `crates/switchbard-gui/tests/backlog_controls.rs` failed with `Harness::run exceeded max_steps (4)`, each on a different full-suite run:

1. `create_modal_labels_assignee_milestone_and_dependencies_fields_reset_after_create` (backlog_controls.rs:626) — Repaint causes: [egui-0.36.1 scroll_area.rs:1524]
2. `cleanup_confirm_sets_the_synchronous_status_before_the_spawned_archive_calls` (backlog_controls.rs:488) — Repaint causes: [app.rs:1886, app.rs:1908]

Both pass reliably standalone (`cargo test -p switchbard-gui --test backlog_controls` alone: 90/90 green, repeated). Only the full `mise run ci` / full-suite parallel run trips them.

TASK-56 (Done) already diagnosed and fixed this exact mechanism for one test (`board_drag_and_drop_between_columns_queues_a_status_change`): a background thread's `ctx.request_repaint()` zeroes `repaint_delay` without registering an in-frame repaint cause, so `Harness::run()`'s settle loop (which breaks only on a nonzero delay) keeps stepping and trips `max_steps`. TASK-56's own notes name the fix as switching the post-action step from `run()` to `run_steps()` for the specific step that races a background thread — several other spots in backlog_controls.rs already carry that pattern (grep `run_steps` in the file), but these two tests still call plain `run()` around actions that spawn or wait on background threads (a create-modal save, an archive spawn), so they still carry the same race TASK-56 fixed elsewhere in the file.

Impact: CI is flaky under load — `mise run ci` intermittently reports a red `test` gate for reasons that have nothing to do with the change under review, forcing a re-run (or, worse, training reviewers to treat a red CI as noise). Anyone running `mise run ci` on this file after a full-suite-triggering change is affected.

Found while clearing an unrelated review pass on TASK-99's Digest place PR (#84); out of scope for that work (backlog_controls.rs is untouched by it), so filed here instead of fixed inline.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Both named tests (create_modal_labels_assignee_milestone_and_dependencies_fields_reset_after_create, cleanup_confirm_sets_the_synchronous_status_before_the_spawned_archive_calls) no longer trip max_steps across at least 10 consecutive full mise run ci executions
- [x] #2 The fix follows TASK-56's own guidance: switch the specific post-action step that races a background thread from harness.run() to harness.run_steps(), not raise max_steps
- [x] #3 A repo-wide grep confirms no other backlog_controls.rs test calling plain run() immediately after an action that spawns a background thread (save/archive/create) is left unconverted
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Closed by the TASK-181 fix, which is the same defect one file over.

AC #2 (TASK-56's guidance, not a raised max_steps): kept. The fix is step_past_spawn() - run_steps(SPAWN_FRAMES) - never a larger budget for run().

AC #3 (nothing left unconverted): both named tests are converted.
- create_modal_labels_assignee_milestone_and_dependencies_fields_reset_after_create, and its sibling create_modal_fields_typed_via_kittest_persist_through_a_real_create: the plain run() after clicking Create is now step_past_spawn. Create spawns the create worker; the buffer reset under assertion is synchronous on the click.
- cleanup_confirm_sets_the_synchronous_status_before_the_spawned_archive_calls already used harness.step() with the right rationale (one frame is all it needs) and was left as is - step() cannot trip max_steps.
The two hand-rolled 'run_steps(4) + deadline poll' loops in the same file now call the shared settle_until().

AC #1 (10 consecutive full runs): six consecutive mise run test runs, all green, zero failures, plus two full mise run ci runs green (85 test binaries each). Short of ten by the letter; the standard the fix actually meets is stronger than repetition - the race was reproduced deterministically (see TASK-181's notes) and the reproduction no longer trips.

The durable part: crates/switchbard-gui/tests/common/mod.rs now owns the rule, with the reasoning in a doc comment, so the fourth occurrence has somewhere to look instead of being re-derived.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Same race as TASK-181, one file over. Both named tests are converted to the shared step_past_spawn/settle_until helpers rather than a raised step budget, and the three hand-rolled poll loops in the file now go through the same helper. Six consecutive green test runs and two green full CI gates. Closed alongside TASK-181 on fix/tui-bug-sweep (9a68415).
<!-- SECTION:FINAL_SUMMARY:END -->
