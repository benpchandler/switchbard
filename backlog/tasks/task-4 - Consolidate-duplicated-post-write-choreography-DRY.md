---
id: TASK-4
title: Consolidate duplicated post-write choreography (DRY)
status: Done
assignee:
  - '@removal-fixes'
created_date: '2026-06-21 01:49'
updated_date: '2026-06-21 02:03'
labels:
  - mutation-arch
  - audit
  - maintainability
dependencies: []
priority: medium
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit finding #4. The 'persist -> re-derive runtime state -> set status -> wake workers' sequence is hand-rolled with freehand variation: add_repo_from_path (app.rs:303-306) kick_all; remove_repo (app.rs:318-322) scanner_kick only; move_repo (app.rs:559-560) no status/no kick; apply_created_worktree (worktree_actions.rs:108-117) refresh_worktrees_from_disk + kick_all. 'What must happen after a repos mutation' has no single authoritative representation. remove_repo kicking only the scanner leaves probe/detection/agent-context caches stale until their own tick (they self-prune, so it works, but looks accidental).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A single helper (e.g. after_repos_mutation) encapsulates save_config + rebuild_worktrees + kick_all + config_status.set
- [x] #2 add_repo_from_path, remove_repo, and move_repo call the helper; behavior differences are intentional and documented, not accidental
- [x] #3 Rename (label-only) path is left as-is; no behavior regressions in existing tests
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Unit test feasibility: after_repos_mutation calls save_config (disk I/O), rebuild_worktrees (takes Arc<Mutex> locks), kick_all (Kick channels), and config_status.set — all of which interact with the full HiveApp struct. new_headless + render_ui is the established harness for testing HiveApp behaviour end-to-end (see ui_views.rs), but testing 'kick_all was called' would require either mock channels or observable side effects that don't exist in the headless harness. The helper is 4 lines that directly delegate to three already-tested primitives (save_config, rebuild_worktrees, kick_all), so there is no novel logic to isolate. No new test added; no regression in existing tests (all pass).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Introduced after_repos_mutation(&self, status) as the single authoritative definition of 'what must happen after a repos mutation': save_config + rebuild_worktrees + kick_all + config_status.set. add_repo_from_path and remove_repo now call it. move_repo intentionally does not: a reorder leaves the worktree set unchanged so kicking workers would be noise; it runs save_config + rebuild_worktrees + config_status.set only, documented in its own comment. Two deliberate behaviour changes vs pre-refactor: (1) remove_repo now does kick_all instead of scanner-only, immediately pruning probe/detection/agent-context caches; (2) move_repo now emits a 'reordered repos' status that was previously missing. CI green; no regressions.
<!-- SECTION:FINAL_SUMMARY:END -->
