---
id: TASK-62
title: 'Format fork: record the switchbard-owned-writes decision in the trajectory doc'
status: Done
assignee:
  - '@claude'
created_date: '2026-08-28 18:40'
updated_date: '2026-08-28 18:41'
labels:
  - format-fork
dependencies: []
priority: high
ordinal: 61000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Supersede the 2026-08-04 system-of-record entry (docs/product-trajectory.md, Unified task hub: "all mutations still write through the backlog CLI"). New owner decision (2026-08-28): switchbard-core becomes the sole writer implementation for Backlog.md-format task files; the format is forked at its current on-disk shape; external writers (backlog CLI, backlog MCP) are deprecated for tracked repos once the native write layer lands and is swapped in.

Invariant to record: one writer implementation (switchbard-core write layer), many frontends (GUI, switchbard-dispatch, thin task CLI). Files stay where they are, readable by anything that speaks Backlog.md, until a divergence task says otherwise.

Module header docs that cite the CLI as authority (mutations, dispatch, refine) are updated when the swap lands (see the swap task), not here.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The trajectory doc records the fork decision, dated 2026-08-28, with the one-writer-implementation invariant and the task sequencing
- [x] #2 The 2026-08-04 unified-task-hub entry cross-references its supersession instead of silently contradicting it
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Amended the 2026-08-04 unified-task-hub parenthetical to cross-reference the supersession (repos-as-system-of-record explicitly unchanged).
- Added the "Backlog format fork (owner-approved 2026-08-28)" Planned entry: rationale (one-fact-two-sources), the one-writer-implementation invariant, and the TASK-62..68 sequencing.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Recorded the format-fork decision in docs/product-trajectory.md: new dated Planned entry carrying the one-writer-implementation invariant and the TASK-62..68 sequencing, plus a cross-reference from the superseded half of the 2026-08-04 entry. Module docs citing the CLI as write authority are deliberately untouched until the TASK-65 swap.
<!-- SECTION:FINAL_SUMMARY:END -->
