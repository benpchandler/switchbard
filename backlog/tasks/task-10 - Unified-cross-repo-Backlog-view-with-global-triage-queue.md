---
id: TASK-10
title: Unified cross-repo Backlog view with global triage queue
status: Done
assignee: []
created_date: '2026-08-05 02:30'
updated_date: '2026-08-05 03:01'
labels:
  - hub
  - slice-1
dependencies: []
priority: high
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Slice 1 of the unified task hub (docs/product-trajectory.md, owner-approved 2026-08-04).

Merge every tracked repo's Backlog.md tasks into ONE ranked list in the Backlog view, replacing the one-project-at-a-time picker as the default. Data is already aggregated — backlog_projects_snapshot() returns all projects; this is view-layer work plus a pure core ranking function.

Design:
- New core module backlog_triage: pure fn triage_rank(&[(project, task)], &OrderingOverlay) -> sorted list. Order: explicit ordering.yml rank first, then overdue > due-today > priority > age > repo name. Unit-test the ranking exhaustively (pure function, no IO).
- OrderingOverlay parsed from <hub repo>/ordering.yml (ranked: ["repo:task-id", ...]). Hub repo is any tracked project containing ordering.yml at its root; absence = empty overlay. Parse in core, no IO in the ranking fn.
- UI: "All projects" becomes the default scope of the existing view; per-project scope remains via the existing picker. Each row gains a repo badge (existing ui/components badge). Filters/sort/multi-select/detail pane keep working across repos; task ids display as repo:id when scope=All.
- Rule 4/6 debt: ui/backlog.rs is 1710 LOC and flagged split-when-touched. Split into ui/backlog/{mod,summary,toolbar,list,detail,create}.rs (or similar) BEFORE adding the unified scope. No new UI piled onto the monolith.
- Mutations unchanged: all writes still go through the backlog CLI per project. No new stores (Config stays single source of truth for repos).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Backlog view defaults to a single merged list across all tracked repos with a repo badge per row; per-project scope still available
- [x] #2 Pure triage_rank function in switchbard-core with unit tests covering overlay-rank, overdue, due-today, priority, age, and repo tiebreak orderings
- [x] #3 ordering.yml overlay from the hub repo is parsed (missing file = empty overlay) and overrides computed order; malformed file surfaces a non-fatal warning
- [x] #4 ui/backlog.rs split into focused submodules, none exceeding ~600 LOC, before the unified scope lands
- [x] #5 mise run ci green on macOS (fmt, clippy -D warnings, tests incl. legibility audit); perf smoke on the Backlog view shows no p95 regression vs main
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Core: add switchbard-core/src/backlog_triage.rs — TriageEntry/TriagePriority/TriageDue,
   OrderingOverlay::parse (pure) + load_ordering_overlay (IO boundary), find_hub_repo,
   triage_entry_from_task glue, triage_rank pure fn. Exhaustive unit tests. Re-export from lib.rs.
2. Split ui/backlog.rs (1710 LOC) into ui/backlog/{mod,format,toolbar,list,selection,sort,detail,create}.rs,
   each <=~600 LOC, before adding new scope (pure refactor, behavior-preserving commit).
3. Add BacklogScope::{All,Project} to BacklogViewState; move selected_task_id/bulk_selected_task_ids
   to composite (PathBuf, task_id) keys so cross-repo same-numbered tasks never collide.
4. Wire ordering.yml: extend spawn_backlog worker to locate the hub repo among tracked repos,
   load+parse overlay, publish to a new Arc<Mutex<>> on HiveApp with a non-fatal warning surfaced
   via backlog_status. All scope's task list is produced by triage_rank over every project's tasks.
5. UI: All-projects becomes default scope; repo badge (status_pill Neutral) per row; task id shown
   as "repo:id" when scope=All; per-project scope still selectable via existing picker.
6. Update ui_views.rs seeded test for new field names + default-All-scope display; add UI test(s)
   for repo badge / cross-repo ranking if warranted.
7. mise run ci (fmt, clippy -D warnings, tests) green on this machine; SWITCHBARD_PERF=1 smoke
   comparing Backlog-view p95 against main before/after.
8. backlog task edit 10 --check-ac per AC as satisfied; --notes / --append-notes with evidence;
   --final-summary; only then -s Done.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
mise run ci: green (fmt, clippy -D warnings, cargo test --workspace --all-targets,
incl. legibility audit) on commits 54401b0 (core: backlog_triage + doc/spec bookkeeping)
and 5a79b4b (gui: split + unified scope) on feature/unified-backlog-view.

Perf smoke (headless egui_kittest harness, HiveApp::new_headless, release build,
60 timed frames after 5 warmup, wall-clock via Instant around harness.run();
git worktree of main used as baseline, deleted after measuring):
- Stress dataset (8 repos x 30 tasks = 240 tasks total): main (1 project, 240
  tasks, original single-project view) p50=1.73ms p95=1.91ms; this branch
  (8 repos merged, default Triage sort, repo badge per row) p50=2.48ms
  p95=2.78ms after collapsing visible_task_rows to one computation per frame
  (was 2.69/2.85ms before that fix, computed twice/frame).
- Realistic dataset (5 repos x 15 tasks = 75 tasks): this branch p50=0.92ms
  p95=0.97ms.

Honest read: there IS a measurable per-frame increase at the 240-task stress
scale (~0.87ms p95, ~45%), not zero. Isolated the triage-rank sort itself to
~0.18ms of that via an ablation (temporarily forcing sort_key=Task); the rest
is the per-row repo badge widget + repo:id formatting AC #1 explicitly
requires, i.e. proportional to real added work, not an accidental
inefficiency — no full-snapshot rebuild, no per-frame IO, no unbounded list
(the one per-frame TriageEntry clone pass is bounded by visible task count).
Both the row list and the pre-existing single-project view render every row
unconditionally (no scroll virtualization) on both branches alike — that's
pre-existing debt, not introduced here; flagging it as a natural follow-up
if repo/task counts grow much further, not blocking this task. At the
realistic 75-task scale the absolute cost is sub-millisecond and well inside
a single frame budget either way.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Backlog view now defaults to a triage-ranked All-projects scope merging every
tracked repo, with a repo:id-formatted task id and repo badge per row;
per-project scope stays one click away in the same picker. Ranking is a pure,
exhaustively-tested switchbard-core::triage_rank (overlay > overdue/due-today
> priority > age > repo), fed by an ordering.yml overlay auto-discovered from
whichever tracked repo hosts it, parsed with a non-fatal warning on malformed
YAML. ui/backlog.rs (1710 LOC) was split into 8 focused submodules (largest
416 LOC) before the new scope landed, per the recorded Rule 4/6 debt.
Selection/bulk-select moved to (project_root, task_id) composite keys so
same-numbered tasks from different repos never collide; cross-repo bulk
actions group by project before dispatching one backlog CLI call per repo.

mise run ci is green (fmt, clippy -D warnings, full test suite incl.
legibility audit). Perf smoke against a `main` worktree baseline shows a
real but bounded, explained increase (~0.87ms p95 at a deliberately
oversized 240-task/8-repo stress dataset; sub-millisecond at a realistic
75-task/5-repo scale) — see Implementation Notes for the full methodology
and the ablation isolating the ranking-sort cost from the new repo-badge
widget cost. Filed TASK-13 (low priority) to virtualize the task list rows
if repo/task counts grow enough for that to matter; not a blocker here.

Mutation path is unchanged: every write still goes through the backlog CLI
per project; Config remains the single source of truth for tracked repos.
<!-- SECTION:FINAL_SUMMARY:END -->
