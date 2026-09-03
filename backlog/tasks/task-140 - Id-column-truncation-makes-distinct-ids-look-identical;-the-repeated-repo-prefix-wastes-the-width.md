---
id: TASK-140
title: Id column truncation makes distinct ids look identical; the repeated repo prefix wastes the width
status: To Do
assignee: []
created_date: '2026-09-03 00:35'
updated_date: '2026-09-03 01:33'
labels:
  - gui
  - ux
  - bug
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: in the budget repo, LED-648.10 and LED-648.11 render as 'LED-648.1' in the list's id column, so the owner sees three rows with the same id under one parent and cannot tell them apart or address them. Every repo with more than nine children under a parent will hit this. The prefix 'LED-' is the same on every row of a single-repo view and carries no information there.

Problem statement: the id column has a fixed width sized for two-level ids, and the repo prefix consumes four of its characters on every row. Truncation is silent: no ellipsis, no tooltip, no widening. Owner's proposed direction: dedupe values that are identical down a column, at least the id prefix within a single-repo view, so the width goes to the part that varies. Other options: elide the prefix only when every row shares it, widen the column to the longest id present, or show the full id on hover/focus.

Evidence: owner screenshot 2026-09-03 of the budget board, rows LED-648.10 and LED-648.11 displayed as LED-648.1. Owner's words: 'dedupe things that are exactly the same within a column or at least the id column, like LED- -- it's not really relevant within a repo -- should help with the truncation.'
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Two tasks with different ids never render with the same visible id in any list or board column
- [ ] #2 In a single-repo view the id column spends its width on the varying part of the id, not the shared prefix
- [ ] #3 Whatever is elided is recoverable (full id on hover, focus, or copy)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
sbt half shipped in PR feat/tui-compact-columns (2026-09-03): bare ids via Column::display_text, content-fit widths. The egui Tasks place still truncates; GUI half remains.
<!-- SECTION:NOTES:END -->
