---
id: TASK-24
title: 'Board: clicking a card opens the task detail view'
status: Done
assignee: []
created_date: '2026-08-05 14:02'
updated_date: '2026-08-05 15:56'
labels:
  - board
  - ux
dependencies: []
priority: medium
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner-requested UX (2026-08-05): clicking a Board lens card today only selects the task (sets backlog_view.selected_task) but the List lens's detail pane isn't visible from the Board lens, so nothing appears to happen. The owner expects the click to actually open/show that task's detail view. Likely: switch to a lens/mode where the detail pane is visible (e.g. jump to the List lens scoped to the task, mirroring how Digest's card click already does this), or add an inline detail affordance to the Board lens itself.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (commit 3e0cb64): board card click now sets selected_task + switches to the List lens, mirroring digest's card click. CORRECTION (2026-08-05, TASK-29): the click handler landed correctly in code, but was never actually reachable by the user — Ui::dnd_drag_source's own internal Sense::drag()-only wrapper widget shadowed the click in the real app (confirmed against the live 0.31 build, not just kittest). This session's own 'UNDRIVABLE-BY-KITTEST' conclusion for the card-click test was consequently wrong: it wasn't a harness limitation, it was this real defect. Fixed under TASK-29 (board.rs render_strip/paint_card restructuring) — card click is now genuinely kittest-driven (board_card_click_selects_the_task_and_jumps_to_list, backlog_controls.rs) and confirmed live.
<!-- SECTION:NOTES:END -->
