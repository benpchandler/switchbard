---
id: TASK-80.3
title: Build the Task Queue GitHub delivery backend
status: To Do
assignee: []
created_date: '2026-08-31 21:45'
updated_date: '2026-09-01 15:35'
labels:
  - backend
  - github
  - task-queue
dependencies:
  - TASK-80.2
parent_task_id: TASK-80
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the approved Task Queue authority contract below the UI: separate domain identities and Project memberships, ordered source bindings, a read-only GitHub delivery adapter, bounded versioned snapshots, deterministic source-band projection, one shared local queue selector, generic reference resolution, and atomic idempotent dispatch-success recording. Reuse authenticated gh access while preserving per-field Unknown and source-level freshness without writing GitHub or creating shadow tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Domain types keep native tasks, source bindings, Project memberships, GitHub artifacts, generic links, and queue projections distinct; the GitHub adapter exposes no mutation capability.
- [ ] #2 Ordered github.com Project bindings round-trip as default-empty additive config, refresh is bounded to five 100-item pages off the render path, and per-binding versioned cache failure is isolated and disposable.
- [ ] #3 Authentication, scope, rate limit with reset time, network, unsupported host, partial connection, and MissingOrInaccessible responses remain distinguishable; prior successful evidence becomes Stale and no-prior-success becomes Unavailable.
- [ ] #4 One core selector preserves load_backlog_repo stack order for GUI, sb queue, and fallback dispatch, then composes configured Project bands and suppresses linked or adopted standalone projections.
- [ ] #5 RecordDispatchSuccess validates a canonical PR URL and atomically records dispatched label, In Review, one note, and one reference; same-URL replay is a no-op and conflicting replay or write failure has zero task-file effect.
- [ ] #6 Synthetic and integration tests cover the governing valid and invalid fixtures, existing task/config compatibility, new-format opaque node identities, transfer/rename, cache bounds, generation races, partial observations, dispatch failure injection, and no GitHub writes.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Governing contract (owner-approved 2026-09-01): docs/decisions/task-queue-authority-model/. This task is the first production slice and must make that package verifier progress honestly; it may not reinterpret the source-band, generic-link, freshness, or outcome-authority decisions.
<!-- SECTION:NOTES:END -->
