---
id: TASK-132
title: 'sbt: one ColumnSpec table for name, header, width, filter field, glyph legend'
status: To Do
assignee: []
created_date: '2026-09-02 19:26'
labels:
  - tui
  - refactor
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: adding the ball column required edits in six files (config, tasks, paint, sort, view, ball) with a match arm each; a missed arm is a compile error at best and a silently wrong width or filter at worst. Evidence: commit 8df0b8b (ball column) diff across crates/switchbard-tui/src/{config,tasks,paint,sort,view}.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Column facts (name, header, width, filter_field, categorical, vocabulary) come from one table in config.rs; view.rs, sort.rs, paint.rs, tasks.rs consult it rather than matching on Column themselves
- [ ] #2 Adding a column is one table row plus its value accessor
<!-- AC:END -->
