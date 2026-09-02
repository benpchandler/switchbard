---
id: TASK-130
title: 'sbt: one ViewState struct shared by App, SavedView, and resume'
status: To Do
assignee: []
created_date: '2026-09-02 19:26'
labels:
  - tui
  - refactor
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: the same five fields are enumerated by hand in view_label, resume_state/resume_from, save_view, switch_view, and SavedView; each new view attribute (columns, glyphs, paint) touched all of them and one was missed at least twice during development. This is the one-fact-two-sources shape the repo keeps re-learning. Evidence: crates/switchbard-tui/src/app.rs view_label/resume_state/save_view/switch_view; crates/switchbard-tui/src/views.rs SavedView; git log feat/tui-2 for the glyph_columns commit touching all of them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 filter, sort, columns, glyph_columns, and paint live in one ViewState used by App, SavedView, view_label, resume_state, and save_view
- [ ] #2 Adding a field to ViewState is the only change needed for it to save, resume, and affect the custom label
- [ ] #3 resume across self-restart uses the same serialization as views.lua, not a tab-separated string
<!-- AC:END -->
