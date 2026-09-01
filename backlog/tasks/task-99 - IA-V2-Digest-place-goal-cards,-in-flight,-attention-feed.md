---
id: TASK-99
title: 'IA V2: Digest place - goal cards, in-flight, attention feed'
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
The landing place (trajectory: IA V2). Goal cards lead (existing goal statuses), then in-flight tasks, then the attention feed: rows computed from owning objects (PR probe, run reaper, server watch, port scan, removal_safety) with inline icon actions (review/merge, open mock, retry, restart, remove, kill) that reuse those surfaces' command verbs. Nothing stored on tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Feed rows computed live from existing probes; each deep-links to its owning surface
- [x] #2 Inline actions invoke the same verbs as the owning surfaces (no second implementation)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Named gaps (mission brief §3), do not fabricate: no PR probe exists anywhere in switchbard-core, so PR feed rows are omitted entirely (not built, not stubbed). Server-exited rows are also omitted: the scanner is a point-in-time snapshot with no history, so there is no cheap live evidence that a listener present earlier is now gone (vs. never having run there) - inventing an uptime log would fabricate exactly what the brief forbids. Both gaps are recorded in ui/places/digest.rs's own module doc.

Run-row action scoping (a real narrowing from the brief's own text, within layout decision rights): Retry only offers on Failed runs (the actual existing verb, HiveApp::spawn_backlog_dispatch_toggle - detail_lists.rs's render_dispatch_toggle). Orphaned runs (agent verifiably gone) offer Log only - no recovery verb exists anywhere in the app for that state today, not even in the Dispatches view itself, which shows the same blurb and no button. Stalled runs (still running, past stale_after) offer Kill, reusing HiveApp::dispatch_kill_confirm and spawn_kill_dispatch - the exact same confirm state and verb the Dispatches view's own Kill button uses (asserted in tests/digest_place.rs).

Confirm-gating for Kill/Remove (mission brief §4): the owning surface's OWN single-listener Kill (ui::workspace) has no confirm step today, only its bulk 'kill all' does; Digest still confirm-gates the port Kill locally (new DigestViewState::port_kill_confirm, one-at-a-time like dispatch_kill_confirm) before calling the identical HiveApp::spawn_kill - an added safety step on Digest's own summary surface, not a change to Ops. Worktree Remove deep-links to Ops and opens the real HiveApp::open_remove_worktree_confirm dialog there (the confirm state only renders inside ui::workspace::render), rather than re-implementing that dialog a second time.

Goal cards: reused render_goal_card/goal_pace_pill verbatim from ui/backlog/digest.rs (pace pill, meter+today-tick, check-in draft plumbing) via a new self-contained render_goal_cards_for_digest_place, wrapping rather than forking; render_goals_section (Goals place's interim body) is untouched. Bug found and fixed during the screenshot QA pass: an initial version laid cards out with ui.horizontal_wrapped to match the mock's side-by-side goalrow, but render_goal_card's frame claims its whole row's width to pin the favorite star flush right (every other caller stacks vertically) - the second card rendered invisibly on top of the first. Cards now stack vertically, one per line, matching the shared component's real width contract; caught by pixel screenshots, not by the kittest-only assertions (the widget was present in the accessibility tree either way - see tests/digest_place.rs's second_goal_def doc). 'Roll last week' (mock §7a) is the first GUI wiring of switchbard_core::roll_goals via new HiveApp::spawn_goal_roll - the CLI's goal roll already exercised the same core function.

Perf: collect_task_rows (ui/places/digest.rs) locks backlog_repos/dispatch_runs directly rather than cloning the whole cache - an initial version calling HiveApp::backlog_repos_snapshot() twice (once for in-flight rows, once for the run feed) cost ~9ms of central-panel p95 alone at digest_perf_smoke.rs's fixture density, before any widget paints. The goal-cards call (reused machinery, not this task's to optimize) still pays that clone once per frame. Measured after the fix: central p95 ~3-6ms (11 repos x 40 tasks, 22 dispatch-labeled, 12 unattributed listeners, 11 retired worktrees, debug build) - see digest_perf_smoke.rs's own doc for the full numbers and how to reproduce (cargo test -p switchbard-gui --test digest_perf_smoke -- --ignored --nocapture).

PR #84: https://github.com/benpchandler/switchbard/pull/84 - branch feat/task-99-digest-place, rebased on origin/main (a29256b..55279f4, TASK-101 Goals place merged) with conflicts resolved: render_digest_place/render_goals_place (interim bodies) deleted per upstream's own TASK-101 deletion of render_goals_place and this task's own supersession of render_digest_place; render_goal_cards_for_digest_place updated to call the new pub(crate) goal_create::render_goal_modal(repo_options, known_project_names, fixed_target) API; my own duplicate HiveApp::spawn_goal_roll dropped in favor of TASK-101's identical method of the same name (their Goals place needed it too, landed first).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
PR #84: the Digest landing place - goal cards lead (reusing render_goal_card verbatim), then in-flight tasks, then an attention feed computed live from existing probes (dispatch runs, port scan, removal_safety) with inline actions that call the owning surfaces' own verbs (no second implementation). Pre-merge review fixes (04a086b): un-nested the backlog_repos/dispatch_runs lock acquisition in collect_task_rows to match the app's one-mutex-at-a-time discipline; fixed a stale nav.rs module doc pointing at the retired interim Goals body; replaced tofu-rendering glyph buttons with theme.rs painted icon primitives paired with AccessKit labels; removed stray em dashes from user-visible feed text. mise run ci green. Named gaps, recorded and not fabricated: no PR-probe feed rows (no PR probe exists in core) and no server-exited rows (scanner has no history to detect a listener going away).
<!-- SECTION:FINAL_SUMMARY:END -->
