---
id: TASK-13
title: Virtualize Backlog task list rows for large repo/task counts
status: To Do
assignee: []
created_date: '2026-08-05 03:01'
labels:
  - hub
dependencies: []
priority: low
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Both the pre-existing single-project Backlog view and the new All-projects unified scope (task-10) render every visible task row unconditionally inside the egui::ScrollArea each frame — no windowing/virtualization. Perf smoke on task-10 measured ~2.5-2.8ms p95 render time for an 8-repo x 30-task (240 total) stress dataset, sub-millisecond at a realistic 5-repo x 15-task scale. Not a regression blocker for task-10, but if tracked repo/task counts grow substantially, only rendering the rows scrolled into view (egui show_rows or manual clipping) would keep render cost flat instead of linear in total visible task count.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Backlog task list only builds/paints widgets for rows within (or near) the visible scroll viewport
- [ ] #2 Perf smoke at 500+ tasks across repos stays within the same order of magnitude as today's realistic-scale numbers
<!-- AC:END -->
