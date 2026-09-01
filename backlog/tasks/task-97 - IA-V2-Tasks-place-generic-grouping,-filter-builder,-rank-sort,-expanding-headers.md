---
id: TASK-97
title: 'IA V2: Tasks place - generic grouping, filter builder, rank sort, expanding headers'
status: In Review
assignee: []
created_date: '2026-09-01 02:24'
updated_date: '2026-09-01 06:09'
labels:
  - ia
  - gui
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The primary work list (trajectory: IA V2). Group-by generic over every field (project, status, initiative, priority, label, repo, ...), filter-builder + recent filters (no hardcoded chips), Sort: rank via the stack-ranking order, group headers with computed roll-ups that EXPAND IN PLACE for summary (roll-up, goal pace, description) - the project page is cut per the decision record. Stroke-ring selection (unify with TASK-38). Sub-issues indent in place. List/Board as view modes sharing facet state. Inherits TASK-13 virtualization obligations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Group-by works over any field with computed roll-up headers; no project page anywhere
- [x] #2 Expanded header shows summary inline; stack rank appears only as a sort option
- [x] #3 Selection uses the Board's stroke ring in list rows; List/Board share facets
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented crates/switchbard-gui/src/ui/places/tasks/{mod,state,fields,groups,filters,header,list_body}.rs — the Tasks place body (Place::Tasks/TasksView::All), replacing the interim ui::backlog::render routing. Reuses ui::backlog::{sort,list,board,selection,rail,create,search,status_migration,toolbar} (widened to pub(crate)/pub) rather than forking.

Generic group-by: one TaskField enum (Project/Status/Initiative/Priority/Label/Repo/Assignee/Parent/Source) drives both grouping and the filter builder via fields::field_values — no hardcoded per-field UI. Project grouping reuses compute_hierarchy_rollup (defined-but-empty projects included); other fields get a plain done/total roll-up. Expanding a header's caret reveals an in-place summary band (remaining count, meter, goal-pace chip via compute_goal_statuses + GoalDef.scope/inputs, description) — never navigates, matching Q12=B. Rank sort added as BacklogTaskSortKey::Rank (sort.rs's sort_by_rank), ordering by each row's already-computed position in repo.tasks (RepoRanking::sort_tasks's output) — sort-only, no rank column/page. List body is a flattened, uniform-height row list rendered through egui::ScrollArea::show_rows (TASK-13 pattern) — perf smoke: 400 tasks, 200 frames, p50 10.7ms / p95 14.4ms (budget 40ms), well under projects_rank_perf_smoke's ~26-28ms baseline on a similarly-sized unvirtualized fixture. Sub-issues always expanded (Q9=A): child lookup uses switchbard_core::children against the full repo (not the filtered set) so a Done child stays visible nested under its parent even while Done tasks are hidden elsewhere, matching tree.rs's pre-existing "expand reveals the whole sub-tree" rule.

Selection: List rows now wrap in a stroke ring (theme::selected_row_stroke()) on selection, same authority board.rs's cards and Ops rows already used. Found and fixed a real WCAG regression along the way: an initial tint+stroke treatment (matching the mock's CSS) pushed warn_orange "High" priority text below 4.5:1 contrast in dark theme when composited under the tint — legibility_audit.rs caught it. Fixed by going stroke-only for List rows too, matching board.rs's own pre-existing "translucent overlay failed WCAG AA on dark theme" reasoning (its comment, not new). No theme palette values were touched.

Also fixed along the way (all reused/shared code, not new surfaces): list.rs's row title no longer repo-prefixes ("demo:TASK-1 title" -> "TASK-1 title") since directive #9 always shows a separate repo badge column now, making the prefix redundant — this was a real bug the always-on repo badge exposed. Added the "expedited" per-row pill to list.rs's shared row renderer (it only existed in the now-cut Projects lens's own renderer before) and the "N selected - Clear" bulk indicator to the Tasks place's Sort row (also previously Projects/List-lens-toolbar-only). digest.rs's "View all" click now navigates to Place::Tasks + pushes a Status filter predicate instead of setting the now-inert backlog_view.lens/status_filter (Digest and Tasks are separate places since TASK-96).

Test fallout from the routing change: backlog_controls.rs, legibility_audit.rs, ui_views.rs, qa_reverify_2026_08_05.rs all had fixtures depending on the old Place::Tasks+BacklogLens routing. Fixed mechanically (added tasks_place.view_mode alongside legacy backlog_view.lens=Board sites), rewrote Digest-lens fixtures to Place::Digest, rewrote Projects-lens/milestone fixtures against the new group-by model, and removed (with reasoning comments) the handful of tests whose surface is genuinely cut and has no replacement in the Tasks place: Portfolio lens, Statistics lens, and the saved-views toolbar "Save current as..." bar (saved filters now live in the sidebar's FAVORITES group per the trajectory doc — loading a saved view still works via navigate_to_favorite; creating one from the Tasks place's own filter-builder predicates has no UI path yet, a real gap, not silently decided).

Evidence: mise run ci green. New tests: crates/switchbard-gui/tests/tasks_place.rs (14 tests: group-by x4 fields with computed roll-ups, expanding header, rank sort vs a fixture RepoRanking, filter add/remove/persist, stroke-ring selection, sub-issue indentation, List/Board facet sharing, group-by disabled in Board mode) plus fields.rs/state.rs/groups.rs unit tests (~15). Screenshots (both themes): docs/qa/screenshots/tasks_place_{list_grouped,header_expanded,board,narrow}_{light,dark}.png.

Named gaps (not silently decided, filing follow-ups): (1) title clamp - directive #7 asks for a 2-line title clamp; List truncates to one line (legacy list.rs behavior, unchanged) and Board wraps unbounded, growing the card (legacy board.rs behavior, unchanged) - neither matches mock 7c's exact spec, both pre-existing, real widget-height rework needed. (2) Narrow width (~700px) - sidebar correctly collapses to the icon rail, but the list's title column and the persistent detail rail compete for space and title text can go fully invisible; a responsive column-width fix is needed, likely shared with the legacy List lens. (3) Creating a saved view from the Tasks place's filter-builder predicates has no UI path (loading one still works).

PR #85 opened: https://github.com/benpchandler/switchbard/pull/85 (branch feat/task-97-tasks-place). Base a29256b (post-TASK-96); origin/main has since advanced with TASK-101 (Goals place, PR #81) touching overlapping files (app.rs, runtime/mod.rs, digest.rs, ui/backlog/mod.rs, ui/places/mod.rs) - rebase anticipated before merge per the mission brief, not yet requested.
<!-- SECTION:NOTES:END -->
