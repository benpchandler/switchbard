---
id: TASK-96
title: 'IA V2: sidebar shell - places nav, multi-select repo scope, favorites'
status: Done
assignee: []
created_date: '2026-09-01 02:24'
updated_date: '2026-09-01 08:18'
labels:
  - ia
  - gui
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First slice of the places-and-objects IA (trajectory: Information architecture V2). Replace the lens tab row with the sidebar: places Digest / Tasks / Command / Goals / Ops, a multi-select repo scope switcher that every place aggregates over, a FAVORITES group (explicitly favorited objects with type glyphs, no auto-population), the ambient dispatch lamp footer, and the collapsed icon rail at narrow width. Places route to the existing surfaces during transition. Includes the UiConfig.filters re-key from lens names to place/view names (unmatched old keys dropped).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Sidebar renders places + multi-select scope; every surface aggregates over the checked repo set
- [x] #2 FAVORITES holds only explicitly favorited objects; favoriting is an action on the object, glyphs by type, no pins
- [x] #3 Filters persist under place/view keys; stale lens keys dropped; perf smoke on render paths
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
IA V2 sidebar shell landed. Implementation:

- runtime/mod.rs: Place{Digest,Tasks,Command,Goals,Ops} + TasksView{All,Dispatches} replace ViewTab everywhere (HiveApp.view_tab removed). migrate_filter_keys() pure-migrates UiConfig.filters using the directive's 5-pair map; unmatched keys dropped, idempotent (3 unit tests). repo_in_scope() is the one scope-membership definition every surface calls.
- config.rs (core): UiConfig.repo_scope: Vec<String> and UiConfig.favorites: Vec<FavoriteRef> (FavoriteRef{kind,repo,key}, FavoriteKind{Project,Task,Goal,View}), serde defaults, round-trip + default-empty tests.
- ui/nav.rs (new): the places panel. Brand, repo-scope popup (checkbox per tracked repo, canonicalizes to empty=All when every repo ends up checked), FAVORITES group (absent when empty), 5 place rows with glyph+count badge (Tasks/Goals/Ops counted, Digest/Command carry none per directive), Tasks subviews (All tasks/Dispatches with running lamp), footer ambient dispatch lamp reusing ui::dispatch::DispatchSummary. Collapses to a 44px icon-rail panel below 720px window width (viewport_rect, egui 0.36's replacement for the removed screen_rect()), tooltips carrying place names.
- theme.rs: Glyph enum + painted_glyph() (Digest/Tasks/Command/Goals/Ops/Project/View, painter-drawn) and favorite_star_button() (two overlapping triangles, not a 5-point outline - egui's convex_polygon is convex-only). Deliberately NOT literal Unicode glyphs (house/list/lightning/target/gear/square/magnifier code points): font coverage for this exact set was never verified, and painting sidesteps the risk entirely, consistent with this file's existing convention for the earlier geometric/arrow set that rendered as tofu on a stock install.
- top_bar.rs: "view:" tab row removed; per-place filter row now keyed off Place (Ops/Tasks show it, Command/Digest/Goals don't - Command has its own richer facet bar already).
- app.rs render_ui: nav.rs renders first (docks before sidebar.rs/central), then Place match routes to the existing surfaces (Digest->render_digest_place, Tasks/All->backlog::render, Tasks/Dispatches->dispatch::render, Command->agents::render, Goals->render_goals_place, Ops->workspace::render). HiveApp::toggle_favorite/is_favorited/navigate_to_favorite added.
- backlog/digest.rs: render_digest_place (Digest's body only, no Tasks-place lens-tab chrome) and render_goals_place (interim: just the goals section) - both pub(crate), share Snapshot/rail/Pending plumbing with the full render().
- backlog/mod.rs: scoped_repos() applies repo_scope as the outer filter, selected_repo as the inner one. apply_saved_view_by_name() wrapper exposes saved_views' private apply for favorite-view navigation.
- Repo scope wired into all 4 named surfaces: backlog scoped_repos, workspace/mod.rs Snapshot::collect repos filter, agent_context.rs + agent_hooks.rs snapshot repos filter, dispatch/mod.rs collect_rows root skip. Top bar's dispatch chip and nav's footer lamp deliberately stay UNSCOPED (both read the same summarize_dispatch - directive's "global actions row" wording for the chip, and the two ambient indicators must never disagree).
- Star affordances (favorite_star_button) added: task detail rail header (detail.rs), goal cards (digest.rs render_goal_card), saved-views bar (saved_views.rs, only once a named view is active), project rows (projects.rs, keyed by rank_root).
- Fixed a real bug found in the in-flight draft before landing: BacklogViewState/AgentContextViewState's restore_filters/persist_filters still hardcoded the pre-migration "backlog"/"agents.context"/"agents.hooks" keys after active_filter_key() was re-keyed to "tasks.all"/"command.context"/"command.hooks" - the free-text query and the facets for the same place would have silently split across two different filters-map entries. Renamed both sides to match; added migration+restore round-trip coverage.

Evidence:
- mise run ci green (fmt + clippy --workspace --all-targets -D warnings + test), full switchbard-gui suite (all targets, 0 failures) plus switchbard-core/switchbard-task.
- New tests/nav_ia_v2.rs (14 tests): place default+routing (all 5 places + both Tasks subviews render their distinct body), click-navigation (Tasks always lands on All, subview reset), repo-scope narrowing proven independently on Ops/Digest/Command/Dispatches-list, favorites (empty->absent, populate->visible+navigates, idempotent toggle), filter-key migration end-to-end through HiveApp::new_headless (mapped/dropped), narrow-width rail collapse (SWITCHBARD absent -> SB present, place labels move to tooltip-only).
- Fixed 2 pre-existing tests broken by the place-default change (Digest replacing the old Servers default) that weren't caught by the ViewTab grep sweep since they never referenced ViewTab directly: workspace_perf_smoke.rs (place default + CSV column-index shift from the new nav_ms column) and bulk_remove_worktrees.rs (a row-click-coordinate helper that scanned the whole window by y-coordinate and started picking up nav.rs's/the legacy sidebar's own full-height panel nodes - re-anchored to the row's own checkbox as the left boundary).
- Perf: docs/perf/runs/2026-09-01-task-96-ia-v2-workspace-smoke.json. True before/after via an isolated git worktree at the pre-TASK-96 commit (55361a0) running the identical workspace_perf_smoke.rs: baseline frame p95 26.83ms / workspace p95 25.01ms vs post-change frame p95 27.01ms / workspace p95 24.945ms - within noise, no regression, both well under the 33ms/30fps budget.
- Visual QA: docs/qa/screenshots/nav_expanded_{light,dark}.png and nav_rail_narrow_{light,dark}.png (full qa_screenshots.rs re-run, all 42 shots regenerated). Reviewed against VISUAL_QA_CHECKLIST.md - place rows align, active-state highlight (card fill + stroke) reads correctly in both themes, favorite glyphs distinguishable, count badges align right, rail correctly shows only glyphs+tooltips with brand shrunk to "SB". Narrow-width toolbar crowding observed in the Tasks-place body at sub-900px width is pre-existing (reproduced identically in the untouched backlog_narrow_window screenshot, 900x700, which is above the nav's own 720px rail threshold) - not caused by this change, out of this task's scope (ui::backlog::toolbar owns it).

Design-state matrix (dimensions covered / gaps):
- Lifecycle: default (Digest landing, tested) / empty (0 favorites -> no header, tested; 0 repos -> scope popup says "No repos tracked", not separately screenshotted) / active (place highlight, tested+screenshotted).
- Content and scale: 0/1/many repos in scope (0=all repos semantics tested; 1 tested via narrowing tests+screenshot; many not stress-tested beyond the existing 11-repo perf fixture, which renders fine). Long repo/task names: no dedicated truncation test - nav's favorite row and place labels use egui's default truncate()/no-wrap, same convention as the rest of this app; not independently proven for this task, named gap.
- Container: narrow rail below 720px (tested+screenshotted, both themes) / wide expanded (tested+screenshotted).
- Keyboard/focus: N/A - not exercised; nav rows use plain click Sense, no keyboard-nav test written. Named gap for a future pass (matches the mock's own note that the runtime half of accessibility lands with implementation).
- Combinations: place x scope (Digest+Ops+Command+Dispatches all proven independently against a narrowed scope); Tasks<->Dispatches subview transition preserving/resetting state (tested).
- Failure/perf: perf smoke run and compared to a real pre-change baseline (see above); no interaction-failure states apply here (no network/IO in nav.rs itself).

Post-review fixes applied (independent review, MERGE-WITH-FIXES) before merge:
- MAJOR 1: summarize_dispatch was unscoped while the Dispatches list was already scoped - scoped it too, and made the top-bar chip scoped as well (one scoping rule everywhere). New test dispatch_badge_and_lamp_match_the_scoped_dispatches_list proves the chip and the lamp both flip to the scoped count together.
- MAJOR 2: widened repo_in_scope to a Path-based path_in_scope (repo_in_scope now a thin Repo-typed wrapper) and routed every hand-rolled repo_scope.is_empty()||repo_scope.contains(..) call site through it (nav.rs counts+checkbox state, backlog::scoped_repos, dispatch::collect_rows/summarize_dispatch). Grepped clean.
- MINOR 1: summarize_dispatch now computed once per frame in HiveApp::render_ui and passed to top_bar::render/nav::render as a parameter (both now pub(crate), matching the pub(crate) DispatchSummary type).
- MINOR 2: HiveApp::toggle_favorite no longer calls save_config() directly - the end-of-frame diff already owns persistence for favorites, so the direct call was a double disk write per star click.
- NIT accepted, noted in the PR body: the bare agents filter key drops on upgrade, per the decision record's own documented behavior.
- Added the missing Goals-place scoping test the reviewer flagged.
- Re-verified: mise run ci green (43/43 test-result blocks, 0 failures).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
PR #80: sidebar shell replacing the lens tab row with places (Digest/Tasks/Command/Goals/Ops), multi-select repo scope every surface aggregates over, FAVORITES group, ambient dispatch lamp, and narrow-width icon rail. Filter keys migrated from lens to place/view names via a pure, idempotent migrate_filter_keys (unmatched keys dropped). Pre-merge review fixes: scoped summarize_dispatch and the top-bar chip to match the already-scoped Dispatches list (one scoping rule via new path_in_scope); computed the dispatch summary once per frame instead of per-panel; stopped a double disk write on favorite-toggle; added the missing Goals-place scoping test. mise run ci green, 14 new nav_ia_v2.rs tests, perf smoke within noise of the pre-change baseline. Named gaps: long repo/task name truncation not independently stress-tested; no keyboard-nav test written.
<!-- SECTION:FINAL_SUMMARY:END -->
