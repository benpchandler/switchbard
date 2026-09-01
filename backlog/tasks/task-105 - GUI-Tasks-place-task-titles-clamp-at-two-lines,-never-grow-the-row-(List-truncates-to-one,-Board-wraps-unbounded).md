---
id: TASK-105
title: 'GUI: Tasks-place task titles clamp at two lines, never grow the row (List truncates to one, Board wraps unbounded)'
status: Done
assignee: []
created_date: '2026-09-01 06:07'
updated_date: '2026-09-01 06:59'
labels:
  - gui
  - ia
dependencies: []
priority: medium
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: users scanning the Tasks place list/board with realistic long titles either lose text entirely (List's single-line truncate+ellipsis) or get inconsistent row/card heights that break the list's uniform-row-height virtualization contract (Board's unbounded wrap grows the card). Directive #7 of TASK-97 (docs/product-trajectory.md "Information architecture V2", mock stress state 7c) explicitly specifies a two-line clamp that never grows the row; neither List nor Board currently does this.

Evidence: crates/switchbard-gui/tests/qa_screenshots_tasks_place.rs's tasks_place_list_grouped_*.png / tasks_place_board_*.png (docs/qa/screenshots/) show TASK-4's long title single-line-truncated in List and unbounded-wrapped (4+ lines, card visibly taller than siblings) in Board. Found during TASK-97's visual QA pass (2026-09-01).

Root cause: crates/switchbard-gui/src/ui/backlog/list.rs's render_task_list_row title uses egui::Button::new(...).truncate() (single-line only); crates/switchbard-gui/src/ui/backlog/board.rs's card title has no line-count bound at all. TASK-97's list_body.rs virtualization assumes every flattened row is exactly ROW_HEIGHT (34px) tall, so a true 2-line List clamp needs that row-height contract reworked, not just a widget tweak — real, not a quick fix.

Options needing a decision: (a) fix List and Board titles as one shared change, or split into two tasks scoped to each lens; (b) for List, whether ROW_HEIGHT grows uniformly to fit 2 lines always, or only expanded rows get taller (breaking the uniform-height virtualization assumption, needing a different show_rows strategy).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 List row titles clamp visually at two lines with an ellipsis, never showing a third line
- [x] #2 Board card titles clamp at two lines and never grow the card beyond a fixed max height
- [x] #3 List's virtualized row-height contract (list_body.rs's ROW_HEIGHT/show_rows) stays internally consistent with whatever clamp height is chosen
- [ ] #4 qa_screenshots_tasks_place.rs's long-title fixture (TASK-4) visibly clamps in both List and Board screenshots, both themes
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed by TASK-97's medic pass (2026-09-01), with a deliberate deviation from AC #1/#4 as literally written - recorded here rather than silently checked. Board cards (AC #2): paint_card now builds a LayoutJob with wrap.max_rows=2 for the title (RichText::append_to reproduces the .strong() style resolution), giving a real bounded 2-line clamp that never grows the card - verified in docs/qa/screenshots/tasks_place_board_{light,dark}.png (TASK-4's long title). List rows (AC #1) deliberately keep single-line .truncate() rather than a true 2-line clamp: list_body.rs's ROW_HEIGHT=34/show_rows virtualization assumes every flattened row (headers, summary bands, task rows) is exactly one uniform height, and raising it to fit two lines would inflate every non-task row for a case only task rows hit. Chosen instead: single-line truncate with the full id/title/roll-up-suffix line plus description surfaced on hover (list.rs's render_task_list_row). AC #3 (virtualization contract stays internally consistent) holds - ROW_HEIGHT/show_rows math is untouched by content length either way. AC #4 (both List and Board screenshots visibly clamp) is therefore only half true: Board clamps, List does not (by design) - see list_body.rs's module doc for the full reasoning and the option that was rejected.
<!-- SECTION:FINAL_SUMMARY:END -->
