---
id: TASK-101
title: 'IA V2: Goals place - index with inline check-in + goal page with Inputs card'
status: In Review
assignee: []
created_date: '2026-09-01 02:24'
updated_date: '2026-09-01 05:31'
labels:
  - ia
  - gui
  - goals
dependencies: []
priority: medium
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Goals surfaces (trajectory: IA V2 + Weekly goals/input goals). Index: pace rows, pre-filled cumulative check-in for manual goals, edit target, new goal. Goal page: week card, history, and the Inputs card over TASK-92's attach/detach (attached tasks/projects with detach, attach affordance).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Inputs card lists attached tasks/projects with attach/detach wired to attach_goal_inputs/detach_goal_inputs
- [x] #2 Manual-goal check-in input pre-fills the current value (cumulative semantics)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the real Goals place (crates/switchbard-gui/src/ui/places/goals.rs), replacing TASK-96's interim body (ui::backlog::digest::render_goals_place, deleted; the Digest lens's own goals section it wrapped, render_goals_section, is untouched and keeps serving Place::Digest).

Index: one row per goal (table via egui::Grid), pace chip (status_pill/StatusKind, same mapping as the Digest goal card), actual/target, inline cumulative check-in for manual goals pre-filled with the current actual (Q11=B, via the shared backlog_view.goal_checkin_drafts map), "automatic" text for measured goals, pencil edit-target icon, plus-icon "New goal" wired to the (now pub(crate), Snapshot/Pending-decoupled) ui::backlog::goal_create modal. Empty state matches mock 7a (No goals this week / + New goal / Roll last week).

Goal page: crumb, header with pace chip + favorite star, this-week card (actual/target, % week elapsed, meter, Roll into next week, edit target), history card (bars per REAL recorded goals.yml week only - no invented history), Inputs card (attached tasks/projects, per-input counts, detach icon, + Attach task or project picker) over TASK-92's attach_goal_inputs/detach_goal_inputs.

Core: added switchbard_core::edit_goal_target (line-surgical, fails closed, 1 new core test) for the edit-target affordance - no prior core surface existed for it. New HiveApp spawn methods: spawn_goal_edit_target, spawn_goal_attach_input, spawn_goal_detach_input, spawn_goal_roll (all refresh the backlog cache after write, same pattern as spawn_goal_checkin).

Selection state: HiveApp::goals_view (GoalsPlaceState: selected_goal, edit_target draft, attach_input draft) - session-only, additive. Favoriting a goal now also selects its page (navigate_to_favorite).

Repo scope: index and the New Goal/Attach pickers all honor app.repo_scope (crate::runtime::path_in_scope); the goal page looks a specific goal up unscoped (same "widen to find the selected object" pattern the Digest strip uses for tasks).

Accessibility: added theme::icon_button_label (WidgetInfo::labeled + on_hover_text) and three new painted icons (pencil, plus, check) since none of the existing icon-only buttons had a real AccessKit name before - closes the "AccessKit label = verb name" obligation the IA V2 trajectory entry named for implementation.

No filter-key changes: nothing here needs persisted facet state beyond session-only selection/drafts, so UiConfig.filters gets no new "goals" key.

Evidence: mise run ci green. New tests: crates/switchbard-gui/tests/goals_place.rs (7 kittest tests - index rows/pace chips, empty state, manual check-in prefill+real-fixture-repo submit round trip, edit-target real-fixture-repo round trip, goal page cards, no-inputs state, attach/detach real-fixture-repo round trip) plus 1 new core test (edit_target_is_surgical_idempotent_and_fails_closed) and an updated nav_ia_v2.rs assertion (the interim-body check now asserts the real empty-state instead). Screenshots both themes + narrow width in docs/qa/screenshots/goals_index_*.png and goals_goal_page_*.png (via UPDATE_SNAPSHOTS=1 cargo test -p switchbard-gui --test qa_screenshots -- --ignored).

Perf: no shared/perf-sensitive render path touched (Ops/Servers workspace, workers.rs cadence loops) - Goals place is a new, isolated module; app.rs edits are additive (new struct field + spawn methods + one routing-line change). No perf smoke run; noting the decision per CLAUDE.md's render-path rule rather than skipping silently.

PR #81: https://github.com/benpchandler/switchbard/pull/81
<!-- SECTION:NOTES:END -->
