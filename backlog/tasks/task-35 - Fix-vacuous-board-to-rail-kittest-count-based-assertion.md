---
id: TASK-35
title: Fix vacuous board-to-rail kittest (count-based assertion)
status: Done
assignee: []
created_date: '2026-08-05 18:15'
updated_date: '2026-08-05 18:23'
labels:
  - test-quality
dependencies: []
priority: low
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Verifier finding (c7e6624, LOW): board_card_click_updates_the_rail_... passes even with a broken rail because the Board card renders the task id itself. Swap to the verifier's differential pattern: exactly 1 exact-match id label before click (the card), exactly 2 after (card + rail). Production behavior independently verified correct.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Fixed: board_card_click_updates_the_rail_to_show_the_clicked_tasks_detail and digest_card_click_updates_the_rail_to_show_the_clicked_tasks_detail (backlog_controls.rs) now assert exact-match label counts before/after the click (1 -> 2, card + rail) instead of a bare is_some() presence check, which the card's own unconditional id label always satisfied regardless of whether the rail updated. Also checked list_row_click_updates_the_rail_to_show_the_clicked_tasks_detail for the same pattern: confirmed empirically (0 exact matches before the click) that List's row never renders a bare id label (always "{id}  {title}" combined), so it was never vulnerable — strengthened to the same explicit count-based assertion (0 -> 1) for consistency anyway.
<!-- SECTION:NOTES:END -->
