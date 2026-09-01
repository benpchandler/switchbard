---
id: TASK-125
title: Render Tasks and Dispatch immediately from last-known read models
status: To Do
assignee: []
created_date: '2026-09-01 17:44'
updated_date: '2026-09-01 17:45'
labels:
  - cold-start
  - tasks
  - dispatch
  - cache
dependencies:
  - TASK-124
priority: high
project: Instant Cold Start
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Tasks and Dispatch currently begin from empty maps and wait for the staggered Backlog worker, so a cold launch temporarily hides the local work plane and run history.

Evidence: app.rs initializes backlog_repos, ordering, and dispatch_runs empty; workers.rs starts the Backlog refresh around eight seconds after launch and Dispatch consumes that in-memory cache.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Render cached Tasks, ordering, and Dispatch summaries on the first frame while repository loads and run inspection are blocked.
- [ ] #2 Preserve original Backlog loaded-at timestamps so hydrated data cannot overwrite a newer targeted refresh or mutation result.
- [ ] #3 Cached rows are visibly read-only: edit, reorder, archive, refine, dispatch, retry, and kill controls remain unavailable until the owning repository or run has refreshed from live authority.
- [ ] #4 A successful live empty load removes deleted tasks and runs; a failed load retains cached rows with explicit stale and error state.
- [ ] #5 Document local retention and privacy behavior for cached task content, enforce the approved bounds and permissions, and exclude logs and prompts.
- [ ] #6 Cover empty, one, many, long-content, high-volume, partial-failure, and rapid-refresh transitions without losing selections or pending in-session mutations.
<!-- AC:END -->
