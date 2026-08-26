---
id: TASK-50
title: Board columns stretched by one card's untruncated label list
status: Done
assignee: []
created_date: '2026-08-26 00:39'
labels: []
dependencies: []
priority: high
ordinal: 50000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Large dead gaps right of the cards in every populated column. render_labels_and_age put labels.join(', ') in a ui.horizontal with no truncation; a horizontal reports its content's intrinsic width as its own minimum, so one card with several labels pushed the column's scroll content past COLUMN_WIDTH while every other card still painted at its own set_width. Measured on the real board: To Do 468->282, In Progress 363->280. max_rect hid it entirely (reports the constraint, not what was painted). Found by logging the drop zone's painted rect in the running app; no fixture reproduced it because synthetic labels are short enough to fit. Landed in PR #33.
<!-- SECTION:DESCRIPTION:END -->
