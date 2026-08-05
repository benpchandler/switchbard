---
id: TASK-38
title: Unify List/Milestones row selection with Board's stroke-based indicator
status: To Do
assignee: []
created_date: '2026-08-05 18:27'
labels:
  - ux
  - design
dependencies: []
priority: medium
ordinal: 38000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Design differentiation pass (TASK-37) follow-up: Board's card selection uses a border-stroke indicator specifically because layering egui's stock visuals().selection.bg_fill at partial alpha produced a muddy composite that failed WCAG AA on the dark theme. List's row title (Button::selected()) and Milestones' rows (selectable_label()) still use that same stock fill mechanism, so the visual selection treatment is inconsistent across lenses even though no legibility_audit fixture currently shows a failure there. Unify all three to the same proven-AA-safe stroke approach.
<!-- SECTION:DESCRIPTION:END -->
