---
id: TASK-26
title: 'Board: bulk select and bulk-edit properties (status/priority/labels)'
status: Done
assignee: []
created_date: '2026-08-05 14:02'
updated_date: '2026-08-05 15:56'
labels:
  - board
  - ux
dependencies: []
priority: medium
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner-requested UX (2026-08-05): the List lens already has multi-select (plain/shift-range click on bulk checkboxes) and a bulk-edit menu (Pending::bulk_save, one backlog CLI call per project) that reuses BacklogTaskPatch. Board lens has neither. Add the same bulk-select pattern to Board cards and reuse the existing bulk-edit machinery (not a parallel implementation) so status/priority/labels can be changed across a multi-card selection.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (commit 7813aed): board bulk-select checkbox + right-click bulk context menu, reusing List's selection/render_task_context_menu machinery unmodified. CORRECTION (2026-08-05, TASK-29): the checkbox and card click both landed correctly in code but were unreachable in the real app — Ui::dnd_drag_source's internal Sense::drag()-only wrapper widget shadowed clicks on everything nested inside it, checkbox included (confirmed against the live 0.31 build). This session's 'UNDRIVABLE-BY-KITTEST' conclusion was consequently wrong: a real defect, not a harness limitation. Fixed under TASK-29 — the checkbox is now a non-overlapping sibling of the card's click-and-drag region, and checkbox click, card click, right-click context menu, and drag-and-drop are all now genuinely kittest-driven (board_card_checkbox_click_toggles_bulk_selection, board_card_secondary_click_opens_the_bulk_context_menu, board_drag_and_drop_between_columns_queues_a_status_change — backlog_controls.rs) and confirmed live.
<!-- SECTION:NOTES:END -->
