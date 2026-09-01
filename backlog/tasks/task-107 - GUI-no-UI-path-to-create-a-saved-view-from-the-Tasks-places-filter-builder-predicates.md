---
id: TASK-107
title: 'GUI: no UI path to create a saved view from the Tasks place''s filter-builder predicates'
status: To Do
assignee: []
created_date: '2026-09-01 06:07'
labels:
  - gui
  - ia
dependencies: []
priority: low
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: before TASK-97, the legacy Backlog view's toolbar had a "Save current as..." field (ui::backlog::saved_views::render_saved_views_bar) that persisted the current filter/sort/lens combination as a named SavedView. TASK-97's Tasks place deliberately does not reuse that toolbar bar (docs/product-trajectory.md's "Information architecture V2" entry: saved filters are now first-class named views surfaced through the sidebar's FAVORITES group, TASK-96). *Loading* an existing saved view still works unchanged (ui::backlog::apply_saved_view_by_name, called from HiveApp::navigate_to_favorite's FavoriteKind::View arm) — but there is now no UI anywhere to *create* a new saved view from the Tasks place's own state (group-by field, view mode, filter-builder predicates), which is also a different shape than the legacy SavedView struct's four fixed facets (status/priority/project/label) and needs its own persistence design, not a reuse of the old one.

Evidence: found removing crates/switchbard-gui/tests/backlog_controls.rs's saved_view_persists_across_a_simulated_restart, crates/switchbard-gui/tests/qa_reverify_2026_08_05.rs's saved_view_round_trips_milestone_and_label_filters_through_a_real_reload, and crates/switchbard-gui/tests/ui_views.rs's saved_view_can_be_saved_and_deleted during TASK-97 (2026-09-01) — all three drove the now-unreachable toolbar bar.

Options needing a decision: what a "saved view" persists for the Tasks place (group-by field + view mode + Vec<FilterPredicate>, likely a new config shape distinct from the legacy SavedView struct) and where its create affordance lives (a "Save current filters" button in the Tasks place facets row that writes into the sidebar's FAVORITES/saved-views list, vs. some other entry point).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A user can save the Tasks place's current group-by + view mode + filter-builder predicates as a named view from within the Tasks place
- [ ] #2 The saved view appears in the sidebar's FAVORITES/saved-views list and re-applies correctly via the existing FavoriteKind::View / navigate_to_favorite path
- [ ] #3 Persistence shape is documented (new struct vs. reusing/extending SavedView) with a stated reason
<!-- AC:END -->
