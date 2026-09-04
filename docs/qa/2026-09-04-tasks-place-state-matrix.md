# Tasks place - state and stress matrix (TASK-127)

**Date:** 2026-09-04
**Surface:** the Tasks place (List and Board view modes), its sidebar counts,
and the shared task read model (`TasksReadState`).
**Why:** TASK-127's incident (2026-09-01) was a Board render that blanked the
native window for minutes at 496 open tasks, plus a sidebar count that could
disagree with the rows. AC3 requires every applicable state to be named and
bound to evidence, or named as a gap. This document is that binding.

Evidence keys: `T` = deterministic egui_kittest test (file:test),
`P` = perf smoke (`--ignored`, release), `L` = live native observation recorded
in TASK-127's implementation notes (dated), `S` = GPU screenshot test.

## Lifecycle

| State | Behaviour | Evidence |
|---|---|---|
| Cold, no snapshot | Compact centered "Loading task data" chip plus copy; never the false empty message | T `tasks_place.rs:cold_task_model_shows_loading_instead_of_a_false_empty_result` (also asserts chip < 180 px and shared center line); L 2026-09-01 cold capture 79 ms |
| Ready, zero rows | Authoritative empty copy only after a clean scan | T `tasks_place.rs:ready_task_model_can_report_a_real_empty_result`; `workers.rs:never_backlog_tracked_repo_does_not_count_as_a_failed_source` |
| Refreshing | Rows retained, "Refreshing task data" pill, `Refreshing` state on click | T `backlog_controls.rs:refresh_backlog_button_kicks_a_reload_and_sets_status`; L 2026-09-01 refresh 802 ms with 492/496 rows retained |
| Refresh failure (stale, rows retained) | Last-known rows visible, "Task data stale" pill naming the failed source count, edits disabled copy | T `tasks_place.rs:stale_task_model_keeps_last_known_rows_visible`; `workers.rs:failed_task_read_keeps_last_known_rows_and_marks_model_stale`, `vanished_task_source_keeps_last_known_rows_and_marks_model_stale`; L 2026-09-01 unreadable fixture published stale in 412 ms |
| Refresh failure, no rows | "Task data unavailable" with a Retry button; retry recovers count | L 2026-09-01 restart with unreadable source then Retry recovered count to 1 (temporary HOME fixture). Gap: no kittest test clicks Retry |
| Stale, mutation attempted | Every write intent against a failed source is refused with a status-line explanation; clean sibling sources stay writable | T `backlog_controls.rs:stale_source_refuses_an_acceptance_criterion_toggle`, `stale_source_refuses_a_board_drag`, `clean_sibling_source_stays_writable_while_another_is_stale` |
| Restart | Cold path above, then populated rows | L 2026-09-01 exact-revision probe restart reopened Board in 1.23 s after load |
| Read-only task | Board drag ignored with status message | Code guard only: `board.rs:apply_drop` `editable()` check (TASK-29). Gap: no test drives a drag on a read-only card |

## Content and scale

| State | Evidence |
|---|---|
| Zero / one / many rows | T `tasks_place.rs` group-by and filter suites use 1 to 3 rows; L 40-row Switchbard-only scope in 929 ms |
| Realistic maximum (496+) | P `tasks_place_perf_smoke.rs` 500 tasks, 200 frames, release: Board p50 1.171 ms / p95 1.253 ms / max 6.141 ms; List p50 1.143 ms / p95 1.282 ms (2026-09-04). Pre-fix Board p95 was 65.445 ms (debug) with a multi-minute native blank |
| Long content | S `qa_screenshots_tasks_place.rs` clamps card content at the 148 px geometry contract; live debug assertion on card height |
| Duplicate ids across repos | T `tasks_place.rs:group_by_repo_buckets_cross_repo_tasks_by_repo_name`; selection keys on `(repo, id)` |
| Sub-issues and orphans | T `tasks_place.rs:sub_issues_render_indented_and_always_expanded_with_no_collapse_affordance`, `orphaned_sub_issue_promotes_to_a_top_level_row` |
| Non-Latin scripts | N/A for this pass: task titles are user text rendered by egui's default font stack; no script-specific layout logic exists in the Tasks place |

## Scope and filters

| State | Evidence |
|---|---|
| Count and rows share one scope | T `tasks_place.rs:a_positive_scope_with_no_filter_matches_renders_an_honest_empty_state` ("0 of 1 · 1 open" plus explicit message, never blank) |
| All repos vs one repo | S `qa_screenshots_tasks_place.rs:tasks_place_repo_filter_screenshots_both_themes`; L Switchbard-only 40 rows / All repos 496, repeated toggles responsive, All repos restored in 1.20 s |
| Explicit filter add / remove | T `adding_a_filter_predicate_narrows_the_visible_tasks`, `removing_the_last_filter_predicate_restores_every_task_and_remembers_it_as_recent` |
| Migrated legacy scope | T `legacy_repo_picker_state_cannot_hide_repos_or_filter_values` |
| Persisted view state | T `tasks_place_state_persists_group_by_view_mode_and_filters_under_tasks_all`; `tasks_place_saved_views.rs` |
| View-mode switch keeps scope | T `switching_to_board_view_mode_keeps_the_same_scope_and_filters` |

## Container and input

| State | Evidence |
|---|---|
| Current and narrow window | T `facets_controls_remain_horizontal_across_supported_widths`; S standard and narrow Board shots, light and dark (2026-09-01) |
| Wide window | Gap: no dedicated wide-layout test; List columns are content-fit so nothing stretches, but this is unverified above 1280 px |
| Keyboard navigation | Gap: row selection is proven by click (`clicking_a_row_selects_it_the_same_way_the_boards_stroke_ring_selection_does`); no keyboard-only traversal test exists for the Tasks place |
| Pointer drag | T `backlog_controls.rs:board_drag_and_drop_between_columns_queues_a_status_change` |

## Combinations and transitions

| State | Evidence |
|---|---|
| Repeated scope changes | L 2026-09-01 repeated toggles stayed responsive |
| Tracked-repo set changes mid-session | `HiveApp::rebuild_worktrees` enters `Refreshing` without clearing rows (app.rs); no dedicated test. Gap |
| Two drops on one card | T `backlog_controls.rs` task-42 generation tests (`board_move_outcomes` suite) |
| Navigation while a save is in flight | T task-42 pending-move overlay suite |

## Open gaps

1. Retry button click in the empty-stale state has live evidence only.
2. No wide-layout or keyboard-only traversal test for the Tasks place.
3. Tracked-repo change entering `Refreshing` is asserted in code, not by a test.
4. Read-only card drag refusal is a code guard without a test.

None of these gaps involve blanking, panics, data loss, or writes against
stale data, which are the AC4 outcomes. They are recorded so nobody reads
this matrix as full approval of those states.
