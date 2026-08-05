---
id: TASK-10
title: Unified cross-repo Backlog view with global triage queue
status: In Progress
assignee: []
created_date: '2026-08-05 02:30'
updated_date: '2026-08-05 02:38'
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
- [ ] #1 Backlog view defaults to a single merged list across all tracked repos with a repo badge per row; per-project scope still available
- [ ] #2 Pure triage_rank function in switchbard-core with unit tests covering overlay-rank, overdue, due-today, priority, age, and repo tiebreak orderings
- [ ] #3 ordering.yml overlay from the hub repo is parsed (missing file = empty overlay) and overrides computed order; malformed file surfaces a non-fatal warning
- [ ] #4 ui/backlog.rs split into focused submodules, none exceeding ~600 LOC, before the unified scope lands
- [ ] #5 mise run ci green on macOS (fmt, clippy -D warnings, tests incl. legibility audit); perf smoke on the Backlog view shows no p95 regression vs main
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
