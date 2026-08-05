---
id: TASK-30
title: >-
  Freshly created task sometimes missing after create — periodic scan clobbers a
  fresher cache write
status: Done
assignee: []
created_date: '2026-08-05 16:18'
updated_date: '2026-08-05 16:18'
labels:
  - backlog
  - bug
  - concurrency
dependencies: []
priority: high
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner report: a task created through the Create modal sometimes didn't appear on the Board afterward. Investigated the three sub-questions from the mission brief with a real-fixture kittest test — CLI default status ('To Do') is a standard Board column, TASK-25's column union always includes BACKLOG_STATUSES regardless of config.yml, and spawn_backlog_create's refresh_backlog_project_cache correctly updates the same shared cache both List and Board read every frame. None reproduced the symptom in a single-threaded harness. The actual root cause: workers.rs's periodic backlog-scan worker (spawn_backlog, BACKLOG_PERIOD=30s) did a wholesale '*ch.backlog_projects.lock().unwrap() = projects' replace every cycle. Since collect_backlog_projects scans every tracked repo sequentially (real multi-repo wall time), a scan that started reading a project before a create finished, but finishes applying its stale result after spawn_backlog_create's fresher single-project refresh, silently reverts that project — clobbering the newly created task out of the shared cache in every lens, not just Board. Not reproducible in a synchronous kittest harness (no periodic worker thread); proven instead via a deterministic unit test of the merge function.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Fixed: new merge_backlog_projects (workers.rs) applies a fresh scan's results onto the shared cache per-entry, comparing each project's own loaded_at_unix timestamp instead of blind-overwriting — a stale scan can never revert a genuinely newer write, whichever order the two locks land in. Required bumping BacklogProject::loaded_at_unix (core/backlog.rs) from second- to millisecond-precision, since two loads of the same project easily land in the same second (a periodic scan and a just-completed mutation's own reload), which would make a second-granularity timestamp tie exactly in the case that matters most. Repo-removal correctness preserved (roots is the authoritative tracked-repo set; anything outside it is dropped from cache). Proven with 3 deterministic unit tests in workers.rs (merge_keeps_a_newer_cached_snapshot_over_a_stale_scan_result, merge_applies_a_genuinely_newer_scan_result, merge_drops_cache_entries_for_repos_no_longer_tracked) — sanity-checked by temporarily reverting to the old blind-extend logic and confirming the race test fails as expected, then restoring the fix. Also added create_modal_task_is_visible_in_both_list_and_board_against_a_real_fixture_repo (backlog_controls.rs) proving the ordinary non-racing create-then-visible path end to end in both lenses, the baseline this fix protects.
<!-- SECTION:NOTES:END -->
