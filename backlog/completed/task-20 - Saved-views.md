---
id: TASK-20
title: Saved views
status: Done
assignee: []
created_date: '2026-08-05 03:55'
updated_date: '2026-08-05 05:15'
labels:
  - hub
  - beyond-parity
dependencies:
  - TASK-15
priority: medium
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Named filter+sort+lens combos persisted additively in ~/.switchbard/config.toml (engagement state only, never in repos).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
SavedView added to switchbard_core::config (UiConfig.saved_views, additive, no new store) — lens/sort_key/sort_direction stored as plain strings since core has zero UI deps and can't name the GUI's enums; unrecognized values fall back to that enum's default so an older saved view degrades gracefully. GUI: saved_views.rs renders a 'View' combo + Save/Delete in the Backlog toolbar (visible under every lens, since lens is part of what's saved). Saving under an existing name overwrites it. Fixed a naming collision before wiring it up: the core field was originally called project_filter, which collided in meaning with the GUI's own unrelated project_filter (a free-text search string) — renamed to selected_project.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Save/apply/delete all functional, persisted through Config::save. Covered by a kittest test for save+delete (re-apply via the combo isn't exercised — egui's ComboBox trigger has no accessible label in this harness; apply_saved_view is a handful of direct field assignments with no branching or CLI call, noted in the test).
<!-- SECTION:FINAL_SUMMARY:END -->
