---
id: TASK-29
title: >-
  Board card click/checkbox/drag were dead in the live app — dnd_drag_source
  shadowed clicks
status: Done
assignee: []
created_date: '2026-08-05 15:56'
updated_date: '2026-08-05 15:56'
labels:
  - board
  - bug
  - ux
dependencies: []
priority: high
ordinal: 29000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner report against the live 0.31 build: board card clicks, the bulk-select checkbox, and (transitively) drag all did nothing. TASK-24/26's 'UNDRIVABLE-BY-KITTEST' conclusion was wrong — it was a real defect, not a harness limitation. Root cause, confirmed by reading egui 0.31.1's own hit_test.rs: Ui::dnd_drag_source registers the draggable region as a second, Sense::drag()-only widget layered on top of (registered after) whatever the card's own content already registered, and egui's hit-test explicitly discards any click underneath a topmost pure-drag widget. Fixed by making the checkbox a non-overlapping sibling of a single Sense::click_and_drag() widget instead of a retroactive whole-card interact competing with dnd_drag_source's separate drag-only wrapper; drag payload/ghost now hand-rolled (board.rs: render_strip/paint_card/render_drag_ghost) since dnd_drag_source itself can't be reused for content with nested clickable children. Card click, checkbox click, right-click context menu, and full drag-and-drop are now all genuinely kittest-drivable (previously all four were either broken or only code-review-verified).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Fixed in board.rs: render_strip now checks ui.ctx().is_being_dragged(card_id) first and routes to render_drag_ghost (hand-rolled floating-ghost mirror of dnd_drag_source's dragging branch) when true; otherwise paint_card renders the checkbox as a sibling of a captured content_rect, and a single ui.interact(content_rect, card_id, Sense::click_and_drag()) (or Sense::click() for non-editable cards) drives click/drag/right-click, with Response::dnd_set_drag_payload replacing dnd_drag_source's automatic payload management. Verified via mise run ci (fmt+clippy -D warnings+full workspace test suite), both-theme legibility_audit, and 5 new real kittest interaction tests in backlog_controls.rs proving card click, non-editable card click, checkbox click, right-click context menu (both the synchronous focus-selection side effect AND the popup itself opening), and a full simulated drag-and-drop between columns all now work — previously all four were either broken (card/checkbox click, drag) or merely code-review-verified (context menu). Owner will click-test the live rebuild to confirm end to end.
<!-- SECTION:NOTES:END -->
